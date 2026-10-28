//! Diagnostics for the TP Remote engine: single-line `key=value` events with an allowlist
//! of field names and secret redaction.
//!
//! Mandatory logging rules for every phase (docs/security/BOOT-02-security-review.md §4):
//! 1. Log typed [`Event`]s whose field names come from [`ALLOWED_KEYS`]. A field with any
//!    other name is still written, but its value is replaced by [`REDACTED`].
//! 2. Never pass passwords, one-time codes, keys, tokens, clipboard or screen contents,
//!    raw keyboard input or authentication payloads, not even to an allowed field.
//!    Secret-bearing values use `tp_contracts::secret::Secret`, which cannot be formatted.
//! 3. Text from peers, libraries or the OS goes through [`FieldValue::text`]: control
//!    characters replaced, secret-looking patterns scrubbed, length capped. Scrubbing is
//!    a safety net for mistakes, not permission to log free text.
//! 4. No persistent log storage and no remote telemetry exist in this phase.
//!
//! The scrubber recognises common patterns only (see [`scrub`]); it cannot recognise a
//! secret that looks like ordinary text. Rules 1–3 are what keep secrets out of logs.
#![forbid(unsafe_code)]

use std::fmt;
use std::io::Write;
use std::sync::{Arc, Mutex};

use tp_contracts::id::SessionRef;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    const fn as_str(self) -> &'static str {
        match self {
            Level::Info => "info",
            Level::Warn => "warn",
            Level::Error => "error",
        }
    }
}

/// Field names whose values are written as given. Everything else is redacted.
pub const ALLOWED_KEYS: &[&str] = &[
    "version",
    "profile",
    "pid",
    "result",
    "error_code",
    "category",
    "session",
    "state",
    "from",
    "to",
    "cause",
    "reason",
    "effects",
    "capability",
    "event_kind",
    "outcome",
    "role",
    "command",
    "count",
    "message",
    // CAP-02A local preview: counters, sizes and fixed identifiers only. Target labels
    // (window titles) and pixels are never logged (CAP-02A security review §5).
    "generation",
    "target_kind",
    "width",
    "height",
    "elapsed_ms",
    "frames",
    "previewed",
    "skipped",
    "dropped",
    "painted",
    "fps",
    "preview_fps",
];

pub const REDACTED: &str = "[REDACTED]";

const MAX_TEXT_CHARS: usize = 160;

/// Words that mark the value after them (`word=…`, `word: …`, `"word":…`) as secret.
const SENSITIVE_WORDS: &[&str] = &[
    "password",
    "passwd",
    "passphrase",
    "pwd",
    "secret",
    "token",
    "otp",
    "passcode",
    "code",
    "pin",
    "apikey",
    "api_key",
    "privatekey",
    "private_key",
    "credential",
    "cookie",
    "authorization",
    "sessionkey",
    "session_key",
    "clipboard",
    "keystroke",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    /// Programmer-chosen identifier such as a state or reason name.
    Static(&'static str),
    /// Untrusted text, already cleaned by [`FieldValue::text`].
    Text(String),
    U64(u64),
    Bool(bool),
    /// Logged as `<8-hex-char tag>/g<generation>`, never the full identifier.
    Session(SessionRef),
}

impl FieldValue {
    /// Cleans untrusted text: control characters replaced, secrets scrubbed, length capped.
    pub fn text(raw: &str) -> FieldValue {
        FieldValue::Text(clean_text(raw))
    }

    fn render(&self) -> String {
        match self {
            FieldValue::Static(s) => (*s).to_owned(),
            FieldValue::Text(t) => t.clone(),
            FieldValue::U64(n) => n.to_string(),
            FieldValue::Bool(b) => b.to_string(),
            FieldValue::Session(s) => format!("{}/g{}", s.id.tag(), s.generation.get()),
        }
    }
}

/// A typed diagnostic event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    level: Level,
    name: &'static str,
    fields: Vec<(&'static str, FieldValue)>,
}

impl Event {
    pub fn new(level: Level, name: &'static str) -> Event {
        Event {
            level,
            name,
            fields: Vec::new(),
        }
    }

    pub fn info(name: &'static str) -> Event {
        Event::new(Level::Info, name)
    }

    pub fn warn(name: &'static str) -> Event {
        Event::new(Level::Warn, name)
    }

    pub fn error(name: &'static str) -> Event {
        Event::new(Level::Error, name)
    }

    pub fn field(mut self, key: &'static str, value: FieldValue) -> Event {
        self.fields.push((key, value));
        self
    }

    pub fn render(&self) -> String {
        render_line(
            self.level,
            self.name,
            self.fields
                .iter()
                .map(|(key, value)| (*key, value.render())),
        )
    }
}

/// Destination of rendered lines.
pub trait Sink: Send + Sync {
    fn write_line(&self, line: &str);
}

/// Writes to stderr. Write failures are ignored: a GUI process may have no stderr, and
/// diagnostics must never take the application down.
#[derive(Debug, Default, Clone, Copy)]
pub struct StderrSink;

impl Sink for StderrSink {
    fn write_line(&self, line: &str) {
        let _ = writeln!(std::io::stderr().lock(), "{line}");
    }
}

/// Keeps lines in memory (tests and in-app diagnostics). Never written to disk.
#[derive(Debug, Default)]
pub struct MemorySink {
    lines: Mutex<Vec<String>>,
}

impl MemorySink {
    pub fn new() -> MemorySink {
        MemorySink::default()
    }

    pub fn lines(&self) -> Vec<String> {
        self.lines.lock().map(|l| l.clone()).unwrap_or_default()
    }
}

impl Sink for MemorySink {
    fn write_line(&self, line: &str) {
        if let Ok(mut lines) = self.lines.lock() {
            lines.push(line.to_owned());
        }
    }
}

#[derive(Clone)]
pub struct Logger {
    sink: Arc<dyn Sink>,
}

impl Logger {
    pub fn new(sink: Arc<dyn Sink>) -> Logger {
        Logger { sink }
    }

    pub fn stderr() -> Logger {
        Logger::new(Arc::new(StderrSink))
    }

    pub fn log(&self, event: &Event) {
        self.sink.write_line(&event.render());
    }
}

impl fmt::Debug for Logger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Logger")
    }
}

/// Formats one event line from string fields, e.g.
/// `tp-remote level=info event=app.start pid=42`. Values are treated as untrusted text.
pub fn format_event(level: Level, event: &str, fields: &[(&str, &str)]) -> String {
    render_line(
        level,
        event,
        fields.iter().map(|(key, value)| (*key, clean_text(value))),
    )
}

/// Writes a [`format_event`] line to stderr.
pub fn emit(level: Level, event: &str, fields: &[(&str, &str)]) {
    StderrSink.write_line(&format_event(level, event, fields));
}

fn render_line<'a>(
    level: Level,
    event: &str,
    fields: impl Iterator<Item = (&'a str, String)>,
) -> String {
    let mut line = format!(
        "tp-remote level={} event={}",
        level.as_str(),
        identifier(event)
    );
    for (key, value) in fields {
        line.push(' ');
        line.push_str(&identifier(key));
        line.push('=');
        if ALLOWED_KEYS.contains(&key) {
            line.push_str(&value_token(&value));
        } else {
            line.push_str(REDACTED);
        }
    }
    line
}

fn clean_text(raw: &str) -> String {
    let printable: String = raw
        .chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect();
    let scrubbed = scrub(&printable);
    if scrubbed.chars().count() > MAX_TEXT_CHARS {
        let mut capped: String = scrubbed.chars().take(MAX_TEXT_CHARS).collect();
        capped.push('…');
        capped
    } else {
        scrubbed
    }
}

/// Replaces secret-looking parts of free text:
/// - anything containing a PEM `PRIVATE KEY` marker → the whole text;
/// - the value after `word=`, `word:` or `"word":` for any word in the sensitive list
///   (case-insensitive, also inside URLs and JSON), up to the end of the token;
/// - the token after a bare sensitive word, `Bearer` or `Basic`.
pub fn scrub(text: &str) -> String {
    if text.contains("PRIVATE KEY") {
        return "[REDACTED:private-key]".to_owned();
    }
    let mut out: Vec<String> = Vec::new();
    let mut redact_next = false;
    for token in text.split(' ') {
        // ASCII lowercasing keeps byte offsets, so offsets into `lower` index `token`.
        let lower = token.to_ascii_lowercase();
        if redact_next && !token.is_empty() {
            if lower == "bearer" || lower == "basic" {
                // "Authorization: Bearer <token>": the secret is after the scheme.
                out.push(token.to_owned());
            } else {
                out.push(REDACTED.to_owned());
                redact_next = false;
            }
            continue;
        }
        if let Some(start) = sensitive_value_start(&lower) {
            if start < token.len() {
                out.push(format!("{}{REDACTED}", &token[..start]));
            } else {
                // "password:" — the value is the next token.
                out.push(token.to_owned());
                redact_next = true;
            }
            continue;
        }
        let bare = lower.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
        if bare == "bearer" || bare == "basic" || SENSITIVE_WORDS.contains(&bare) {
            out.push(token.to_owned());
            redact_next = true;
            continue;
        }
        out.push(token.to_owned());
    }
    out.join(" ")
}

/// Byte offset where a secret value starts inside `lower` (an ASCII-lowercased token).
fn sensitive_value_start(lower: &str) -> Option<usize> {
    let bytes = lower.as_bytes();
    let mut best: Option<usize> = None;
    for word in SENSITIVE_WORDS {
        let mut from = 0;
        while let Some(found) = lower[from..].find(word) {
            let end = from + found + word.len();
            let mut i = end;
            if bytes.get(i) == Some(&b'"') || bytes.get(i) == Some(&b'\'') {
                i += 1;
            }
            if matches!(bytes.get(i), Some(b'=') | Some(b':')) {
                let start = i + 1;
                best = Some(best.map_or(start, |b: usize| b.min(start)));
                break;
            }
            from = end;
        }
    }
    best
}

fn identifier(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "_".to_owned()
    } else {
        cleaned
    }
}

fn value_token(raw: &str) -> String {
    let escaped: String = raw
        .chars()
        .flat_map(|c| match c {
            '"' => vec!['\\', '"'],
            '\\' => vec!['\\', '\\'],
            c if c.is_control() => vec!['?'],
            c => vec![c],
        })
        .collect();
    let needs_quotes = escaped.is_empty() || escaped.contains([' ', '=', '"']);
    if needs_quotes {
        format!("\"{escaped}\"")
    } else {
        escaped
    }
}

/// Descriptor for this crate.
pub const MODULE: tp_contracts::module::ModuleDescriptor = tp_contracts::module::ModuleDescriptor {
    crate_name: "tp-diagnostics",
    role: "diagnostics",
    state: tp_contracts::module::ModuleState::Partial,
    owner_phase: "NET-03",
};

#[cfg(test)]
mod tests;
