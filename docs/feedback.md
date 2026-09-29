# In-app feedback

**Help → Send feedback…** is an optional way to send a message to the maintainer.
It sends the message you enter, an optional reply email, the app version, your
operating system, and a random submission identifier. It does not attach notes,
file paths, keys, or logs. Providing an email does not subscribe you to anything.

The form currently uses an existing service at
`https://mithril.techstackstudio.com/api/feedback`. That address retains its
original domain. A network connection and an available service are needed only
when you choose to send feedback; building the app and working with notes do not
require the service or a website checkout.

You can also [report a bug or suggest a feature on GitHub](https://github.com/travisluong/markdown-note/issues).

## Sending and retrying

Your message can contain up to 5,000 Unicode characters, and the optional email
up to 254. The complete request is also limited to 16 KiB. An unsuccessful send
keeps the draft so you can retry. Retrying an unchanged message reuses its
identifier to prevent duplicate submissions; editing it creates a new request.

Closing and reopening the form retains drafts and pending results for the current
app session. Only confirmed success clears the inputs. There is no persisted
queue or automatic retry after restarting. Sending feedback does not interrupt
editing or autosave.

## Implementation and local checks

`src/feedback.rs` owns validation, request serialization, retry identity, and the
injectable transport. `src/feedback_ui.rs` owns the session's form state and
background task. HTTP requests have a 15-second timeout, disabled redirects,
bounded response reading, and an explicit JSON acknowledgment. Errors use fixed
messages and do not expose server bodies or log the submitted text.

Release builds use the endpoint above and ignore overrides. Development and test
builds allow a separately supplied local test service, for example in Git Bash:

```bash
export MARKDOWN_NOTE_FEEDBACK_ENDPOINT='http://127.0.0.1:3137/api/feedback'
```

Only loopback HTTP endpoints are accepted; remote overrides and credentials in
the URL are rejected. No service credentials are compiled into the application.

The ordinary Rust suite checks validation, retry identity, HTTP results, timeouts,
and redirects using local test servers. Headless GPUI tests check form behavior,
focus, retained drafts, document content, and autosave. They do not require the
hosted service. A separate backend integration test is ignored by default and
needs an explicitly configured local test service; do not point it at production.

See [contribution checks](../CONTRIBUTING.md#validate-a-change) for the commands.
