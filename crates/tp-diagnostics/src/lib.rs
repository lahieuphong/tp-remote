//! Diagnostics for the TP Remote engine.
//!
//! BOOT-01 provides one thing: single-line `key=value` events written to stderr, used
//! by the shell for lifecycle and IPC events. The `tracing` integration, the secret
//! redaction layer, metrics and resource gauges (ARCH-00 §5.6) are not implemented yet,
//! so callers must not pass secrets to [`emit`].
#![forbid(unsafe_code)]

use std::io::Write;

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

/// Formats one event line, e.g. `tp-remote level=info event=app.start pid=42`.
///
/// `event` and field keys are programmer-chosen identifiers. Values are escaped so a
/// value can never start a new line or a new field.
pub fn format_event(level: Level, event: &str, fields: &[(&str, &str)]) -> String {
    let mut line = format!(
        "tp-remote level={} event={}",
        level.as_str(),
        identifier(event)
    );
    for (key, value) in fields {
        line.push(' ');
        line.push_str(&identifier(key));
        line.push('=');
        line.push_str(&value_token(value));
    }
    line
}

/// Writes an event line to stderr. Write failures are ignored: a GUI process may have
/// no stderr, and diagnostics must never take the application down.
pub fn emit(level: Level, event: &str, fields: &[(&str, &str)]) {
    let line = format_event(level, event, fields);
    let _ = writeln!(std::io::stderr().lock(), "{line}");
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
mod tests {
    use super::*;

    #[test]
    fn formats_plain_fields() {
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
    fn keys_and_events_are_restricted_to_identifier_characters() {
        let line = format_event(Level::Error, "a b\n", &[("k=v", "x")]);
        assert_eq!(line, "tp-remote level=error event=a_b_ k_v=x");
    }

    #[test]
    fn empty_and_quoted_values_are_escaped() {
        assert_eq!(
            format_event(Level::Info, "e", &[("a", ""), ("b", "say \"hi\"")]),
            "tp-remote level=info event=e a=\"\" b=\"say \\\"hi\\\"\""
        );
    }
}
