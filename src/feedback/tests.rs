use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;

#[test]
fn validates_unicode_email_and_encoded_size() {
    for message in ["", "  \n", "bad\0text"] {
        assert_eq!(Payload::new(message, ""), Err(Failure::Invalid));
    }
    assert!(Payload::new(&"é".repeat(5000), "").is_ok());
    assert_eq!(Payload::new(&"a".repeat(5001), ""), Err(Failure::Invalid));
    assert_eq!(Payload::new(&"🙂".repeat(5000), ""), Err(Failure::TooLarge));
    for email in ["x", "x@@y.com", "x@y", "x @y.com", "x@y.\0com"] {
        assert_eq!(Payload::new("message", email), Err(Failure::Invalid));
    }
    assert!(Payload::new("ok", &format!("{}@x.com", "x".repeat(248))).is_ok());
    assert_eq!(
        Payload::new("ok", &format!("{}@x.com", "x".repeat(249))),
        Err(Failure::Invalid)
    );
    let payload = Payload::new("<b>literal</b>", " Contact+tag@Example.com ").unwrap();
    assert_eq!(payload.email.as_deref(), Some("Contact+tag@Example.com"));
    let json: serde_json::Value = serde_json::from_slice(&payload.body().unwrap()).unwrap();
    assert_eq!(json.as_object().unwrap().len(), 5);
    assert_eq!(json["message"], "<b>literal</b>");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(
            &Payload::new("anonymous", "").unwrap().body().unwrap()
        )
        .unwrap()
        .as_object()
        .unwrap()
        .len(),
        4
    );
}
#[test]
fn retries_retain_identity_and_new_payloads_get_new_identity() {
    let mut session = Session::default();
    let first = session.begin("hello", "").unwrap().unwrap();
    assert!(session.begin("hello", "").unwrap().is_none());
    for failure in [
        Failure::Offline,
        Failure::Timeout,
        Failure::RateLimited,
        Failure::Server,
    ] {
        session.finish(Err(failure));
        assert_eq!(session.begin("hello", "").unwrap().unwrap(), first);
    }
    session.finish(Err(Failure::Server));
    let changed = session.begin("hello!", "").unwrap().unwrap();
    assert_ne!(changed.submission_id, first.submission_id);
    session.finish(Ok(()));
    assert_ne!(
        session.begin("hello!", "").unwrap().unwrap().submission_id,
        changed.submission_id
    );
}
fn server(
    status: u16,
    body: &'static str,
    delay: Duration,
) -> (String, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}/api/feedback", listener.local_addr().unwrap());
    let task = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut bytes = Vec::new();
        let mut chunk = [0; 4096];
        loop {
            let count = stream.read(&mut chunk).unwrap();
            if count == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..count]);
            if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                let length: usize = headers
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: "))
                    .unwrap()
                    .parse()
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    break;
                }
            }
        }
        std::thread::sleep(delay);
        let _ = write!(
            stream,
            "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nLocation: /redirect-target\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        String::from_utf8(bytes).unwrap()
    });
    (address, task)
}
#[test]
fn real_transport_checks_acknowledgment_status_and_never_follows_redirects() {
    let payload = Payload::new("wire test", "").unwrap();
    for (status, body, expected) in [
        (200, "{\"ok\":true}", Ok(())),
        (200, "{}", Err(Failure::Server)),
        (302, "", Err(Failure::Server)),
        (429, "", Err(Failure::RateLimited)),
        (409, "", Err(Failure::Conflict)),
        (503, "", Err(Failure::Server)),
        (400, "", Err(Failure::Invalid)),
        (413, "", Err(Failure::TooLarge)),
    ] {
        let (address, task) = server(status, body, Duration::ZERO);
        assert_eq!(
            send_to(&payload, &address, Duration::from_secs(2)),
            expected
        );
        let request = task.join().unwrap();
        assert!(request.starts_with("POST /api/feedback "));
        assert!(!request.to_lowercase().contains("\r\norigin:"));
        assert!(!request.to_lowercase().contains("authorization:"));
        assert!(request.contains(&payload.submission_id.to_string()));
    }
    let (address, task) = server(200, "{\"ok\":true}", Duration::from_millis(150));
    assert_eq!(
        send_to(&payload, &address, Duration::from_millis(40)),
        Err(Failure::Timeout)
    );
    task.join().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    assert!(matches!(
        send_to(&payload, &address, Duration::from_secs(1)),
        Err(Failure::Offline | Failure::Timeout)
    ));
}

/// Invoked only by markdown-note-web/tests/cross-repository.mjs against an isolated DB.
#[test]
#[ignore = "requires local Next.js and an isolated Postgres test database"]
fn cross_repository_feedback_with_local_backend() {
    let endpoint = std::env::var("MARKDOWN_NOTE_FEEDBACK_ENDPOINT").expect("test endpoint required");
    assert!(endpoint.starts_with("http://127.0.0.1:"));
    let mut payload =
        Payload::new("Cross-repository test 🙂 <plain text>", "test@example.com").unwrap();
    payload.submission_id =
        Uuid::parse_str(&std::env::var("MARKDOWN_NOTE_FEEDBACK_TEST_ID").unwrap()).unwrap();
    assert_eq!(HttpTransport.send(&payload), Ok(()));
    assert_eq!(HttpTransport.send(&payload), Ok(()));
}
