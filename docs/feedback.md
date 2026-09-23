# In-app feedback

Help → **Send feedback…** opens a native GPUI modal. It submits only the message,
optional reply email, compiled app version, OS name and a random submission UUID.
No notes, paths, keys or logs are attached. Email does not subscribe the sender.

`src/feedback.rs` owns validation, request serialization, retry identity, session
request status and the injectable `Transport` interface. `HttpTransport` uses
reqwest on a GPUI background executor, with a 15-second total network timeout,
redirects disabled, bounded response reading and explicit JSON acknowledgment.
Failures are mapped to fixed messages and never expose server bodies or log input.
`Payload` permits a nonblank message of at most 5,000 Unicode scalar characters,
an optional email of at most 254, and a serialized JSON body of at most 16 KiB.
The byte limit can be reached before the character limit. Server validation is
always authoritative. Feedback is literal plain text.

`src/feedback_ui.rs` owns application-session input entities, subscriptions,
transport and task handle in `Notes.feedback`. The disposable modal only projects
this state. A send disables editing and repeat submission. An unchanged retry
retains its UUID; changed payloads get a new one. Closing and reopening preserves
inputs and pending/completed state. Only acknowledged success clears the inputs.
Completion never opens a dialog or changes focus. An error leaves an explicit retry
button; there is no persisted queue or automatic retry on a later launch. Dismissal
restores workspace focus, including Cancel and Root-driven close. Existing busy
and modal guards apply; document owners and autosave are independent.

The release endpoint is compiled as:
`https://mithril.techstackstudio.com/api/feedback`.
Release builds ignore endpoint overrides. Debug/test builds allow
`MITHRIL_FEEDBACK_ENDPOINT=http://127.0.0.1:3137/api/feedback` (or another loopback
HTTP endpoint, including localhost/IPv6 loopback). Non-loopback, credentials and
non-HTTP overrides are rejected. No API secret is compiled into the application.

## Headless verification

```powershell
cargo check --locked --offline
cargo fmt --check
cargo clippy --locked --offline --all-targets --features editor-diagnostics -- -D warnings
cargo test --locked --offline --features editor-diagnostics
```

Pure tests cover Unicode/byte/email bounds, duplicate sends, payload identity,
HTTP acknowledgment, timeouts, offline failure, rate limits, conflicts and disabled
redirects using only a local TCP server. GPUI TestPlatform tests cover the Help
menu, anonymous/contact submissions, errors/retry, close/reopen during sending,
focus, retained document text/selection/dirty state, and autosave while the modal
is open. They never launch native application windows or inject desktop input.

If a running development app locks `target/debug/mithril.exe`, leave it running
and use `--target x86_64-pc-windows-msvc` for the full Windows test suite. This
creates a separate target output. TestPlatform suites can be run serially with
`-- --test-threads=1` when diagnosing asynchronous filesystem fixture timing.

The adjacent Next.js repository contains `tests/cross-repository.mjs`. Its
`npm run test:cross` starts a local website backed
by a separate database ending in `_test`, calls the ignored library test
`feedback::tests::cross_repository_feedback_with_local_backend`, submits twice
through the real desktop transport, and verifies exactly one Postgres row. It
also checks both real signup forms. Do not invoke that test against production.
It runs a library test executable, never the desktop app.

## Launch handoff

Publish and verify the website backend **before** distributing a feedback-enabled
build. Verify both forms and a native feedback submission, then inspect stored
rows directly in Postgres. No deployment or release is performed by this change.
Record the actual public launch date and review **four weeks after that date**.
The owner should review distinct signup interests and overlap, feedback, and
follow-up conversations about repeat use, replaced tools and obstacles. Signup
counts do not establish active use. No automation or outreach is created.
