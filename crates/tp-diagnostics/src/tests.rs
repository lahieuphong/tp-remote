//! Logging and redaction tests (T-UNI-18 / T-LOG-01, T-LOG-03 rendering part).

use std::sync::Arc;

use tp_contracts::id::{Generation, SessionId, SessionRef};

use super::*;

/// Recognisable synthetic secrets. None of them may ever appear in a log line.
const SYNTHETIC_SECRETS: &[&str] = &[
    "424242919",
    "Hunter2-SYNTHETIC",
    "sk_live_SYNTHETIC_TOKEN",
    "eyJSYNTHETICJWT.payload.sig",
    "MIIEvSYNTHETICKEYMATERIAL",
    "CLIPBOARD-SYNTHETIC-CONTENT",
    "Kq9SYNTHETICicepwd",
];

fn assert_no_secret(line: &str) {
    for secret in SYNTHETIC_SECRETS {
        assert!(
            !line.contains(secret),
            "secret {secret:?} leaked into: {line}"
        );
    }
}

fn session() -> SessionRef {
    SessionRef {
        id: SessionId::parse("0123456789abcdef0123456789abcdef").expect("id"),
        generation: Generation::FIRST,
    }
}

#[test]
fn formats_plain_allowed_fields() {
    assert_eq!(
        format_event(
            Level::Info,
            "app.start",
            &[("pid", "42"), ("profile", "debug")]
        ),
        "tp-remote level=info event=app.start pid=42 profile=debug"
    );
}

#[test]
fn values_cannot_inject_lines_or_fields() {
    let line = format_event(
        Level::Warn,
        "ipc.error",
        &[("message", "bad\nevent=forged result=ok")],
    );
    assert!(!line.contains('\n'));
    assert_eq!(
        line,
        "tp-remote level=warn event=ipc.error message=\"bad?event=forged result=ok\""
    );
}

#[test]
fn event_names_and_keys_are_restricted_to_identifier_characters() {
    let line = format_event(Level::Error, "a b\n", &[("result", "x")]);
    assert_eq!(line, "tp-remote level=error event=a_b_ result=x");
}

#[test]
fn empty_and_quoted_values_are_escaped() {
    assert_eq!(
        format_event(
            Level::Info,
            "e",
            &[("result", ""), ("message", "say \"hi\"")]
        ),
        "tp-remote level=info event=e result=\"\" message=\"say \\\"hi\\\"\""
    );
}

// T-LOG-01: fields outside the allowlist are redacted whatever their value.
#[test]
fn unlisted_field_names_are_redacted() {
    let line = format_event(
        Level::Info,
        "login",
        &[
            ("password", "Hunter2-SYNTHETIC"),
            ("otp", "424242919"),
            ("clipboard", "CLIPBOARD-SYNTHETIC-CONTENT"),
            ("frame", "pixels"),
            ("k=v", "x"),
            ("result", "ok"),
        ],
    );
    assert_no_secret(&line);
    assert_eq!(
        line,
        "tp-remote level=info event=login password=[REDACTED] otp=[REDACTED] \
         clipboard=[REDACTED] frame=[REDACTED] k_v=[REDACTED] result=ok"
    );
}

// T-LOG-01: secret-looking text inside allowed fields is scrubbed.
#[test]
fn secrets_inside_allowed_free_text_are_scrubbed() {
    let texts = [
        "login failed password=Hunter2-SYNTHETIC for user",
        "login failed password: Hunter2-SYNTHETIC",
        "the one-time code 424242919 was wrong",
        "GET https://relay.example/turn?user=a&token=sk_live_SYNTHETIC_TOKEN",
        "Authorization: Bearer eyJSYNTHETICJWT.payload.sig",
        r#"{"user":"a","password":"Hunter2-SYNTHETIC"}"#,
        "-----BEGIN PRIVATE KEY----- MIIEvSYNTHETICKEYMATERIAL -----END PRIVATE KEY-----",
        "clipboard: CLIPBOARD-SYNTHETIC-CONTENT",
        "a=ice-pwd:Kq9SYNTHETICicepwd",
        "PIN=424242919",
    ];
    for text in texts {
        let line = Event::warn("peer.error")
            .field("message", FieldValue::text(text))
            .render();
        assert_no_secret(&line);
        assert!(line.contains("[REDACTED"), "nothing redacted in: {line}");
    }
}

#[test]
fn ordinary_context_survives_scrubbing() {
    let text = "connect failed: host unreachable after 3 attempts (state=connecting)";
    assert_eq!(scrub(text), text);
    let line = Event::info("session.transition")
        .field("session", FieldValue::Session(session()))
        .field("from", FieldValue::Static("connecting"))
        .field("to", FieldValue::Static("connected"))
        .field("effects", FieldValue::U64(2))
        .render();
    assert_eq!(
        line,
        "tp-remote level=info event=session.transition session=01234567/g1 from=connecting \
         to=connected effects=2"
    );
}

#[test]
fn sessions_are_logged_by_tag_never_by_full_identifier() {
    let line = Event::info("x")
        .field("session", FieldValue::Session(session()))
        .render();
    assert!(!line.contains("0123456789abcdef0123456789abcdef"));
}

#[test]
fn long_text_is_capped() {
    let long = "x".repeat(10_000);
    let FieldValue::Text(clean) = FieldValue::text(&long) else {
        panic!("text");
    };
    assert!(clean.chars().count() <= MAX_TEXT_CHARS + 1);
}

#[test]
fn memory_sink_collects_rendered_lines() {
    let sink = Arc::new(MemorySink::new());
    let logger = Logger::new(sink.clone());
    logger.log(&Event::info("a").field("password", FieldValue::text("Hunter2-SYNTHETIC")));
    logger.log(&Event::info("b"));
    let lines = sink.lines();
    assert_eq!(lines.len(), 2);
    for line in &lines {
        assert_no_secret(line);
    }
    assert_eq!(format!("{logger:?}"), "Logger");
}
