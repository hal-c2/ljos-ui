//! `docs/design/` and the rule table say the same thing: a finding links to
//! `<page>#<code>`, so every rule needs its section there, every section its
//! sources, and every `limits::` name a page cites must still exist, or an
//! author following a link lands nowhere or on a number that is gone.

use std::collections::BTreeSet;
use std::path::PathBuf;

use ljos_design::RULES;

fn design_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/design")
}

fn read(name: &str) -> String {
    let path = design_dir().join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The guideline pages: every rule's page, and the README every finding of
/// the model itself points at.
fn pages() -> BTreeSet<&'static str> {
    let mut pages: BTreeSet<_> = RULES.iter().map(|r| r.area.page()).collect();
    pages.insert("README.md");
    pages
}

fn anchor(code: &str) -> String {
    format!("<a id=\"{code}\"></a>")
}

/// The text from a rule's anchor to the next anchor or second-level heading.
fn section<'a>(text: &'a str, code: &str) -> Option<&'a str> {
    let start = text.find(&anchor(code))?;
    let rest = &text[start + anchor(code).len()..];
    let end = [rest.find("<a id=\""), rest.find("\n## ")]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(rest.len());
    Some(&rest[..end])
}

#[test]
fn every_rule_has_its_section_on_its_page() {
    let mut missing = Vec::new();
    for rule in RULES {
        let text = read(rule.area.page());
        match section(&text, rule.code) {
            None => missing.push(format!("{} has no anchor", rule.guide())),
            Some(body) => {
                let heading = body.trim_start().lines().next().unwrap_or("");
                if !heading.starts_with('#') || !heading.contains(rule.code) {
                    missing.push(format!("{}: no heading naming the code", rule.guide()));
                }
                if !body.contains("Sources:") || !body.contains("https://") {
                    missing.push(format!("{}: no sources", rule.guide()));
                }
            }
        }
    }
    assert!(missing.is_empty(), "{missing:#?}");
}

#[test]
fn every_page_lists_its_sources() {
    for page in pages().into_iter().chain(["agent-brief.md"]) {
        let text = read(page);
        let sources = text
            .split("\n## Sources")
            .nth(1)
            .unwrap_or_else(|| panic!("{page} has no Sources section"));
        assert!(
            sources.contains("https://"),
            "{page}'s Sources section names no URL"
        );
    }
}

#[test]
fn an_anchor_shaped_like_a_rule_is_a_rule_of_its_page() {
    const PREFIXES: &[&str] = &[
        "a11y-", "color-", "text-", "layout-", "focus-", "motion-", "content-",
    ];
    for page in pages() {
        let text = read(page);
        for piece in text.split("<a id=\"").skip(1) {
            let id = &piece[..piece.find('"').unwrap()];
            if !PREFIXES.iter().any(|p| id.starts_with(p)) {
                continue;
            }
            assert!(
                RULES.iter().any(|r| r.code == id && r.area.page() == page),
                "{page}#{id} is no rule of {page}; a renamed rule leaves its old section behind"
            );
        }
    }
}

#[test]
fn the_agent_brief_names_every_rule() {
    let brief = read("agent-brief.md");
    let missing: Vec<_> = RULES
        .iter()
        .filter(|r| !brief.contains(&format!("[{}]", r.code)))
        .map(|r| r.code)
        .collect();
    assert!(missing.is_empty(), "agent-brief.md leaves out {missing:?}");
    assert!(
        brief.lines().count() < 150,
        "agent-brief.md is meant to stay under 150 lines"
    );
}

/// `NAME` and `module::NAME` for every constant in `src/limits.rs`, and each
/// module on its own.
fn limit_names() -> BTreeSet<String> {
    let source =
        std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/limits.rs"))
            .unwrap();
    let mut names = BTreeSet::new();
    let mut module: Option<String> = None;
    for line in source.lines() {
        if let Some(rest) = line.strip_prefix("pub mod ") {
            let name = rest.trim_end_matches(" {").trim().to_owned();
            names.insert(name.clone());
            module = Some(name);
        } else if line == "}" {
            module = None;
        } else if let Some(rest) = line.trim_start().strip_prefix("pub const ") {
            let name = &rest[..rest.find(':').unwrap()];
            names.insert(match &module {
                Some(m) => format!("{m}::{name}"),
                None => name.to_owned(),
            });
        }
    }
    names
}

#[test]
fn every_limit_a_page_names_exists() {
    let names = limit_names();
    assert!(names.contains("TEXT_CONTRAST") && names.contains("motion::FREQUENT_MAX_MS"));
    let mut unknown = Vec::new();
    let entries = std::fs::read_dir(design_dir()).unwrap();
    for entry in entries {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "md") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for piece in text.split("limits::").skip(1) {
            let name: String = piece
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == ':')
                .collect();
            let name = name.trim_end_matches(':');
            // `limits::…` in prose names the module, not a constant.
            if !name.is_empty() && !names.contains(name) {
                unknown.push(format!("{}: limits::{name}", path.display()));
            }
        }
    }
    assert!(unknown.is_empty(), "{unknown:#?}");
}
