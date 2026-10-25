//! The log an agent can ask for: the newest lines the application wrote,
//! each with a number so that a later request can ask for what is newer.
//!
//! A line may repeat what a hidden field holds (`console.log(password.text)`).
//! It is masked with every secret known when it is written, again when a
//! secret becomes known later, and once more when it is told: a line kept
//! is never the plaintext of a secret the inspector has seen.

use std::collections::VecDeque;
use std::fmt::Write as _;

use ljos_engine::{LogEntry, LogLevel, SourcePlace};
use ljos_mcp_core::ToolReply;
use serde_json::{Value, json};

use super::args::{Level, LogsArgs, MAX_LOG_LINES};
use super::redact::{SecretBook, mask_text};

/// Lines kept. Older ones are forgotten: the inspector is not a log store.
const KEPT: usize = 1000;
/// A message is cut after this many characters, so that one line cannot
/// take the whole answer.
const MAX_MESSAGE_CHARS: usize = 2000;

#[derive(Clone, Debug)]
struct Line {
    seq: u64,
    level: LogLevel,
    category: String,
    message: String,
    file: Option<String>,
    line: Option<u32>,
}

#[derive(Debug, Default)]
pub(crate) struct LogRing {
    lines: VecDeque<Line>,
    /// The number of the newest line ever kept; numbers start at 1.
    last: u64,
    /// The secrets the lines are masked with.
    book: SecretBook,
}

fn level_of(l: LogLevel) -> Level {
    match l {
        LogLevel::Debug => Level::Debug,
        LogLevel::Info => Level::Info,
        LogLevel::Warning => Level::Warning,
        LogLevel::Critical => Level::Critical,
    }
}

/// `Main.qml:12:3: message` or `file:///a/Main.qml:12: message`: the file
/// and the line a warning of the engine starts with.
fn leading_place(message: &str) -> Option<(String, u32)> {
    let head = message.split_whitespace().next()?;
    let at = head.find(".qml:")? + ".qml".len();
    let (file, rest) = head.split_at(at);
    let line: String = rest[1..].chars().take_while(char::is_ascii_digit).collect();
    Some((file.to_string(), line.parse().ok()?))
}

fn clip(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_string(),
    }
}

impl Line {
    fn mask(&mut self, secrets: &[String]) {
        if secrets.is_empty() {
            return;
        }
        mask_text(&mut self.message, secrets);
        mask_text(&mut self.category, secrets);
        if let Some(file) = self.file.as_mut() {
            mask_text(file, secrets);
        }
    }
}

impl LogRing {
    /// The secrets known so far: the faults are masked with them too.
    pub(crate) fn book(&self) -> &SecretBook {
        &self.book
    }

    /// Makes `secrets` known, and masks the lines kept with what is new
    /// among them: a line written before its secret was seen must not
    /// show it once the secret is forgotten.
    pub(crate) fn learn<'a>(&mut self, secrets: impl IntoIterator<Item = &'a String>) {
        let new = self.book.learn(secrets);
        if !new.is_empty() {
            self.lines.iter_mut().for_each(|l| l.mask(&new));
        }
    }

    pub(crate) fn push(&mut self, entry: &LogEntry, at: Option<&SourcePlace>) {
        let (file, line) = match at {
            Some(at) => (Some(at.file.clone()), Some(at.line)),
            None => leading_place(&entry.message).map_or((None, None), |(f, l)| (Some(f), Some(l))),
        };
        self.last += 1;
        if self.lines.len() == KEPT {
            self.lines.pop_front();
        }
        // Masked before it is cut: a cut could split a secret.
        let mut kept = Line {
            seq: self.last,
            level: entry.level,
            category: entry.category.clone(),
            message: entry.message.clone(),
            file,
            line,
        };
        kept.mask(self.book.secrets());
        kept.message = clip(&kept.message, MAX_MESSAGE_CHARS);
        self.lines.push_back(kept);
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.lines.len()
    }

    /// The newest lines that fit `args`, oldest first, in at most `budget`
    /// bytes of text and as many of JSON.
    pub(crate) fn answer(&self, args: &LogsArgs, budget: usize) -> ToolReply {
        let least = args.level.unwrap_or(Level::Debug);
        let since = args.since.unwrap_or(0);
        let limit = args.limit.unwrap_or(MAX_LOG_LINES).clamp(1, MAX_LOG_LINES);
        let fitting: Vec<&Line> = self
            .lines
            .iter()
            .filter(|l| l.seq > since && level_of(l.level) >= least)
            .collect();
        let total = fitting.len();
        let masked: Vec<Line> = fitting[total.saturating_sub(limit)..]
            .iter()
            .map(|l| {
                let mut l = (*l).clone();
                l.mask(self.book.secrets());
                l
            })
            .collect();
        let masked: Vec<&Line> = masked.iter().collect();
        let mut shown: &[&Line] = &masked;
        loop {
            let (text, value) = self.write(shown, total);
            // The oldest lines go first: the newest are what is asked for.
            if shown.len() <= 1 || (text.len() <= budget && value.to_string().len() <= budget) {
                return ToolReply::from(value).with_text(text);
            }
            shown = &shown[shown.len().div_ceil(4)..];
        }
    }

    fn write(&self, shown: &[&Line], total: usize) -> (String, Value) {
        let truncated = shown.len() < total;
        let mut text = String::new();
        let mut entries = Vec::with_capacity(shown.len());
        for l in shown {
            let _ = write!(text, "{} {}", l.seq, l.level);
            if let (Some(file), Some(line)) = (&l.file, l.line) {
                let _ = write!(text, " {file}:{line}");
            }
            // One line of the log is one line of the answer, whatever the
            // application wrote into it.
            let _ = writeln!(
                text,
                " {}",
                l.message.replace('\n', "\\n").replace('\r', "\\r")
            );
            entries.push(json!({
                "seq": l.seq,
                "level": l.level.to_string(),
                "category": l.category,
                "message": l.message,
                "file": l.file,
                "line": l.line,
            }));
        }
        if shown.is_empty() {
            text.push_str("(no log lines)\n");
        } else if truncated {
            let _ = writeln!(
                text,
                "({} of {total} lines, the newest; `since: {}` gives what comes after)",
                shown.len(),
                self.last
            );
        } else {
            let _ = writeln!(
                text,
                "({} lines; `since: {}` gives what comes after)",
                shown.len(),
                self.last
            );
        }
        let value = json!({
            "entries": entries,
            "count": shown.len(),
            "total": total,
            "truncated": truncated,
            "next_since": self.last,
        });
        (text, value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(level: LogLevel, message: &str) -> LogEntry {
        LogEntry {
            level,
            category: "qml".into(),
            message: message.into(),
        }
    }

    #[test]
    fn the_newest_hundred_are_listed_oldest_first() {
        let mut ring = LogRing::default();
        for i in 0..150 {
            ring.push(&entry(LogLevel::Debug, &format!("line {i}")), None);
        }
        let reply = ring.answer(&LogsArgs::default(), 20_000);
        let v = &reply.structured;
        assert_eq!(v["count"], 100);
        assert_eq!(v["total"], 150);
        assert_eq!(v["truncated"], true);
        assert_eq!(v["entries"][0]["message"], "line 50");
        assert_eq!(v["entries"][99]["message"], "line 149");
        assert_eq!(v["next_since"], 150);
    }

    #[test]
    fn level_and_since_narrow_the_answer() {
        let mut ring = LogRing::default();
        ring.push(&entry(LogLevel::Debug, "a"), None);
        ring.push(&entry(LogLevel::Warning, "b"), None);
        ring.push(&entry(LogLevel::Critical, "c"), None);
        let warn = LogsArgs {
            level: Some(Level::Warning),
            ..LogsArgs::default()
        };
        assert_eq!(ring.answer(&warn, 20_000).structured["count"], 2);
        let later = LogsArgs {
            since: Some(2),
            ..LogsArgs::default()
        };
        let reply = ring.answer(&later, 20_000);
        assert_eq!(reply.structured["count"], 1);
        assert_eq!(reply.structured["entries"][0]["message"], "c");
        assert_eq!(reply.structured["truncated"], false);
    }

    #[test]
    fn the_place_comes_from_the_caller_or_from_the_message() {
        let mut ring = LogRing::default();
        let at = SourcePlace {
            file: "Main.qml".into(),
            line: 6,
            column: 0,
        };
        ring.push(&entry(LogLevel::Debug, "hello"), Some(&at));
        ring.push(
            &entry(
                LogLevel::Warning,
                "file:///a/Other.qml:12:3: Unable to assign",
            ),
            None,
        );
        ring.push(&entry(LogLevel::Warning, "no place here"), None);
        let reply = ring.answer(&LogsArgs::default(), 20_000);
        let e = &reply.structured["entries"];
        assert_eq!(e[0]["file"], "Main.qml");
        assert_eq!(e[0]["line"], 6);
        assert_eq!(e[1]["file"], "file:///a/Other.qml");
        assert_eq!(e[1]["line"], 12);
        assert_eq!(e[2]["file"], Value::Null);
        assert!(
            reply
                .text
                .as_deref()
                .unwrap()
                .contains("1 debug Main.qml:6 hello")
        );
    }

    #[test]
    fn a_long_log_is_cut_to_the_budget_and_old_lines_are_forgotten() {
        let mut ring = LogRing::default();
        let long = "x".repeat(5000);
        for _ in 0..1100 {
            ring.push(&entry(LogLevel::Debug, &long), None);
        }
        assert_eq!(ring.len(), KEPT);
        let reply = ring.answer(&LogsArgs::default(), 20_000);
        assert!(reply.text.as_deref().unwrap().len() <= 20_000);
        assert!(reply.structured.to_string().len() <= 20_000);
        assert_eq!(reply.structured["truncated"], true);
        assert!(reply.structured["count"].as_u64().unwrap() >= 1);
    }

    fn told(ring: &LogRing) -> (String, String) {
        let reply = ring.answer(&LogsArgs::default(), 20_000);
        (reply.structured.to_string(), reply.text.unwrap_or_default())
    }

    #[test]
    fn a_secret_known_when_a_line_is_written_is_never_kept() {
        let mut ring = LogRing::default();
        ring.learn(&["hunter2".to_string()]);
        ring.push(&entry(LogLevel::Debug, "typed hunter2"), None);
        ring.push(&entry(LogLevel::Debug, "an ordinary line"), None);
        assert_eq!(ring.lines[0].message, "typed <redacted>");
        let (json, text) = told(&ring);
        for answer in [&json, &text] {
            assert!(!answer.contains("hunter2"), "{answer}");
            assert!(answer.contains("typed <redacted>"), "{answer}");
            assert!(answer.contains("an ordinary line"), "{answer}");
        }
    }

    #[test]
    fn a_line_written_before_its_secret_was_seen_is_masked_once_it_is() {
        let mut ring = LogRing::default();
        ring.push(&entry(LogLevel::Warning, "hunter2 and swordfish"), None);
        // The field held one, then the other; both stay secrets.
        ring.learn(&["hunter2".to_string()]);
        ring.learn(&["swordfish".to_string()]);
        assert_eq!(ring.lines[0].message, "<redacted> and <redacted>");
        let (json, text) = told(&ring);
        for answer in [&json, &text] {
            assert!(!answer.contains("hunter2"), "{answer}");
            assert!(!answer.contains("swordfish"), "{answer}");
        }
    }

    #[test]
    fn the_seq_and_the_level_are_not_masked() {
        let mut ring = LogRing::default();
        ring.learn(&["1".to_string()]);
        ring.push(&entry(LogLevel::Debug, "pin 1"), None);
        let reply = ring.answer(&LogsArgs::default(), 20_000);
        assert!(
            reply
                .text
                .as_deref()
                .unwrap()
                .starts_with("1 debug pin <redacted>")
        );
        assert_eq!(reply.structured["entries"][0]["seq"], 1);
    }

    #[test]
    fn a_masked_line_stays_masked_once_its_secret_is_forgotten() {
        let mut ring = LogRing::default();
        ring.push(&entry(LogLevel::Debug, "pin hunter2"), None);
        ring.learn(&["hunter2".to_string()]);
        let others: Vec<String> = (0..5000).map(|i| format!("other{i}")).collect();
        ring.learn(&others);
        let (json, text) = told(&ring);
        assert!(
            !json.contains("hunter2") && !text.contains("hunter2"),
            "{json}"
        );
    }
}
