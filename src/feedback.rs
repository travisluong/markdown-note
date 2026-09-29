//! Session-only feedback policy and transport. Never reads documents or configuration files.
use serde::{Deserialize, Serialize};
use std::{io::Read, time::Duration};
use uuid::Uuid;

pub const ENDPOINT: &str = "https://mithril.techstackstudio.com/api/feedback";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const OS: &str = std::env::consts::OS;
pub const BODY_LIMIT: usize = 16 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Payload {
    pub submission_id: Uuid,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    pub app_version: String,
    pub os: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Invalid,
    TooLarge,
    Offline,
    Timeout,
    RateLimited,
    Conflict,
    Server,
}
impl Failure {
    pub fn message(self) -> &'static str {
        match self {
            Self::Invalid => {
                "Enter a nonblank message (up to 5,000 characters) and a valid optional email (up to 254 characters)."
            }
            Self::TooLarge => {
                "This message exceeds the 16 KiB request limit. Please shorten it and try again."
            }
            Self::Offline => {
                "Could not connect. Your draft is retained; check your connection and retry."
            }
            Self::Timeout => {
                "The request timed out. Your draft is retained; retry to check or save the same submission."
            }
            Self::RateLimited => "Too many submissions. Wait a minute, then retry.",
            Self::Conflict => {
                "This submission ID conflicts with saved feedback. Edit your message before sending again."
            }
            Self::Server => {
                "Feedback could not be confirmed. Your draft is retained; please retry later."
            }
        }
    }
}
fn valid_email(email: &str) -> bool {
    if email.chars().count() > 254 || email.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.contains('@')
        && domain
            .split_once('.')
            .is_some_and(|(a, b)| !a.is_empty() && !b.is_empty())
}
impl Payload {
    pub fn new(message: &str, email: &str) -> Result<Self, Failure> {
        let email = email.trim();
        if message.trim().is_empty()
            || message.chars().count() > 5000
            || message.contains('\0')
            || (!email.is_empty() && !valid_email(email))
        {
            return Err(Failure::Invalid);
        }
        let value = Self {
            submission_id: Uuid::new_v4(),
            message: message.into(),
            email: (!email.is_empty()).then(|| email.into()),
            app_version: VERSION.into(),
            os: OS.into(),
        };
        value.body()?;
        Ok(value)
    }
    pub fn body(&self) -> Result<Vec<u8>, Failure> {
        let bytes = serde_json::to_vec(self).map_err(|_| Failure::Invalid)?;
        if bytes.len() > BODY_LIMIT {
            return Err(Failure::TooLarge);
        }
        Ok(bytes)
    }
}
#[derive(Default)]
pub struct Session {
    pub sending: bool,
    pub result: Option<Result<(), Failure>>,
    pending: Option<Payload>,
}
impl Session {
    pub fn begin(&mut self, message: &str, email: &str) -> Result<Option<Payload>, Failure> {
        if self.sending {
            return Ok(None);
        }
        let mut payload = Payload::new(message, email)?;
        if let Some(previous) = &self.pending
            && previous.message == payload.message
            && previous.email == payload.email
        {
            payload.submission_id = previous.submission_id;
        }
        self.pending = Some(payload.clone());
        self.sending = true;
        self.result = None;
        Ok(Some(payload))
    }
    pub fn finish(&mut self, result: Result<(), Failure>) {
        self.sending = false;
        if result.is_ok() {
            self.pending = None;
        }
        self.result = Some(result);
    }
}
pub trait Transport: Send + Sync {
    /// Blocking work: call only on a background executor.
    fn send(&self, payload: &Payload) -> Result<(), Failure>;
}
pub struct HttpTransport;
impl Transport for HttpTransport {
    fn send(&self, payload: &Payload) -> Result<(), Failure> {
        send_to(payload, &endpoint()?, Duration::from_secs(15))
    }
}
fn endpoint() -> Result<String, Failure> {
    #[cfg(debug_assertions)]
    if let Ok(value) = std::env::var("MARKDOWN_NOTE_FEEDBACK_ENDPOINT") {
        let url = url::Url::parse(&value).map_err(|_| Failure::Invalid)?;
        let loopback = match url.host() {
            Some(url::Host::Domain("localhost")) => true,
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            _ => false,
        };
        if url.scheme() != "http"
            || !loopback
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(Failure::Invalid);
        }
        return Ok(value);
    }
    Ok(ENDPOINT.into())
}
fn send_to(payload: &Payload, endpoint: &str, timeout: Duration) -> Result<(), Failure> {
    let body = payload.body()?;
    let client = reqwest::blocking::Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| Failure::Offline)?;
    let response = client
        .post(endpoint)
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .map_err(|error| {
            if error.is_timeout() {
                Failure::Timeout
            } else {
                Failure::Offline
            }
        })?;
    match response.status().as_u16() {
        200 => {
            // Bound even a misconfigured server response; never display its submitted content.
            let mut bytes = Vec::new();
            response
                .take(1025)
                .read_to_end(&mut bytes)
                .map_err(|error| {
                    if error.kind() == std::io::ErrorKind::TimedOut {
                        Failure::Timeout
                    } else {
                        Failure::Server
                    }
                })?;
            if bytes.len() > 1024 {
                return Err(Failure::Server);
            }
            let value: serde_json::Value =
                serde_json::from_slice(&bytes).map_err(|_| Failure::Server)?;
            if value.get("ok").and_then(|v| v.as_bool()) == Some(true) {
                Ok(())
            } else {
                Err(Failure::Server)
            }
        }
        400 | 415 => Err(Failure::Invalid),
        413 => Err(Failure::TooLarge),
        409 => Err(Failure::Conflict),
        429 => Err(Failure::RateLimited),
        _ => Err(Failure::Server),
    }
}

#[cfg(test)]
mod tests;
