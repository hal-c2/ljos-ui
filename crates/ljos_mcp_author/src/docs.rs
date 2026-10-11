//! `docs_search`: the sections of the project's documentation that say the
//! most about a question.
//!
//! What is searched is `docs/` (every `.md` below it), `features/README.md`
//! and the reasons the ledger gives for its rows. A document is split at
//! its headings, and a section is the unit that is ranked and returned, so
//! an answer points at a few paragraphs and not at a file of a hundred
//! pages. The index is built for each call: the files are small, and an
//! index that is kept would be stale the moment an agent edits a document.
//!
//! A section may cite a page of Qt's documentation, which is passed on. A
//! link to Qt's source is never passed on (CLEAN_ROOM.md).

use std::{collections::HashMap, sync::Arc};

use ljos_mcp_core::{CallCtx, SurfaceError, ToolHandler, typed};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Ledger, Project, tools};

const DEFAULT_LIMIT: usize = 8;
const MAX_LIMIT: usize = 20;
const SNIPPET_CHARS: usize = 280;
/// How much more a word in a heading says than a word in the text.
const HEADING_WEIGHT: f64 = 3.0;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SearchArgs {
    /// Words to look for: `binding tiers`, `hot reload`, `Accessible.name`.
    query: String,
    /// Results in one answer: 8 unless given, 20 at most.
    limit: Option<usize>,
    /// `next_cursor` of the previous answer.
    cursor: Option<String>,
}

#[derive(Serialize, JsonSchema)]
struct Hit {
    /// The document: a resource that can be read in full.
    uri: String,
    /// The same resource narrowed to this section (`...#L120`): read this
    /// when the document is long. Null for a ledger row.
    section: Option<String>,
    /// The headings that lead to the section, outermost first; for a
    /// ledger row `Module.Type (status)`.
    title: String,
    /// The 1-based line of the section's heading. Null for a ledger row.
    line: Option<u32>,
    /// Higher says more about the query. Only comparable within an answer.
    score: f64,
    /// The lines of the section that say the most about the query.
    snippet: String,
    /// Pages of Qt's documentation the section cites.
    qt_docs: Vec<String>,
}

#[derive(Serialize, JsonSchema)]
pub(crate) struct SearchOut {
    query: String,
    /// Best first.
    results: Vec<Hit>,
    /// Sections that match at all, over every page of the answer.
    total: usize,
    next_cursor: Option<String>,
}

/// A part of a document with a heading of its own.
struct Section {
    uri: String,
    /// Whether `uri#L<line>` reads the section alone.
    addressable: bool,
    title: String,
    line: Option<u32>,
    heading: String,
    level: usize,
    body: String,
    /// The body is the list of the headings below, written here: it is
    /// shown, and not searched, because each of those is found by itself.
    outline: bool,
}

/// How deep a heading is, if `line` is one (`## Tags` is 2).
fn heading(line: &str) -> Option<(usize, &str)> {
    let level = line.bytes().take_while(|b| *b == b'#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let rest = line[level..].strip_prefix([' ', '\t'])?;
    Some((level, rest.trim().trim_end_matches('#').trim_end()))
}

fn fence(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with("```") || line.starts_with("~~~")
}

/// The sections of a Markdown document, in order. A `#` inside a fenced
/// block is code, not a heading.
fn sections(uri: &str, name: &str, text: &str) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    let mut path: Vec<(usize, String)> = Vec::new();
    let mut fenced = false;
    let mut current = Section {
        uri: uri.to_owned(),
        addressable: true,
        title: name.to_owned(),
        line: Some(1),
        heading: name.to_owned(),
        level: 0,
        body: String::new(),
        outline: false,
    };
    for (index, line) in text.lines().enumerate() {
        if fence(line) {
            fenced = !fenced;
        }
        match heading(line).filter(|_| !fenced) {
            Some((level, words)) => {
                path.retain(|(above, _)| *above < level);
                path.push((level, words.to_owned()));
                let title: Vec<&str> = path.iter().map(|(_, words)| words.as_str()).collect();
                let next = Section {
                    uri: uri.to_owned(),
                    addressable: true,
                    title: title.join(" > "),
                    line: Some(index as u32 + 1),
                    heading: words.to_owned(),
                    level,
                    body: String::new(),
                    outline: false,
                };
                out.push(std::mem::replace(&mut current, next));
            }
            None => {
                current.body.push_str(line);
                current.body.push('\n');
            }
        }
    }
    out.push(current);
    // What stands before the first heading is a section only if it says
    // something.
    if out
        .first()
        .is_some_and(|first| first.level == 0 && first.body.trim().is_empty())
    {
        out.remove(0);
    }
    // A heading with nothing under it but more headings says what they are.
    for index in 0..out.len() {
        if !out[index].body.trim().is_empty() {
            continue;
        }
        let level = out[index].level;
        let below: Vec<&Section> = out[index + 1..]
            .iter()
            .take_while(|s| s.level > level)
            .collect();
        let Some(nearest) = below.iter().map(|s| s.level).min() else {
            continue;
        };
        let names: Vec<&str> = below
            .iter()
            .filter(|s| s.level == nearest)
            .take(12)
            .map(|s| s.heading.as_str())
            .collect();
        out[index].body = format!("Sections: {}.\n", names.join("; "));
        out[index].outline = true;
    }
    out
}

/// The section of `text` that starts at the 1-based `line`: down to the
/// next heading that is not below it. From a line that is not a heading,
/// the rest of the section it is in.
pub(crate) fn section_at(text: &str, line: u32) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let start = (line as usize)
        .checked_sub(1)
        .filter(|at| *at < lines.len())?;
    // Fences are counted from the top: a line number may point into one.
    let mut fenced = false;
    let mut level = 6;
    let mut end = lines.len();
    for (index, text) in lines.iter().enumerate() {
        if fence(text) {
            fenced = !fenced;
        }
        let Some((depth, _)) = heading(text).filter(|_| !fenced) else {
            continue;
        };
        if index == start {
            level = depth;
        } else if index > start && depth <= level {
            end = index;
            break;
        }
    }
    Some(lines[start..end].join("\n") + "\n")
}

/// Letters as they are compared: lower case, and without the marks that
/// are easy to leave out when typing (`Ljós` is found by `ljos`).
fn folded(c: char, out: &mut String) {
    for c in c.to_lowercase() {
        match c {
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => out.push('a'),
            'é' | 'è' | 'ê' | 'ë' => out.push('e'),
            'í' | 'ì' | 'î' | 'ï' => out.push('i'),
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ø' => out.push('o'),
            'ú' | 'ù' | 'û' | 'ü' => out.push('u'),
            'ý' | 'ÿ' => out.push('y'),
            'ç' => out.push('c'),
            'ñ' => out.push('n'),
            'ð' => out.push('d'),
            'þ' => out.push_str("th"),
            'æ' => out.push_str("ae"),
            'ß' => out.push_str("ss"),
            c => out.push(c),
        }
    }
}

/// A word as it is indexed: `Tiers` and `tier` are the same word.
fn stem(mut word: String) -> String {
    if word.len() > 3 && word.ends_with('s') && !word.ends_with("ss") {
        word.pop();
    }
    word
}

/// The words of `text`. A name with underscores (`qml_check`) is a word,
/// and so is each of its parts. Links are left out: their words are an
/// address, not what the section is about.
fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for piece in text.split_whitespace() {
        if piece.contains("://") {
            continue;
        }
        let mut word = String::new();
        let flush = |word: &mut String, out: &mut Vec<String>| {
            let whole = std::mem::take(word);
            let whole = whole.trim_matches('_');
            if whole.is_empty() {
                return;
            }
            if whole.contains('_') {
                out.extend(
                    whole
                        .split('_')
                        .filter(|p| !p.is_empty())
                        .map(|p| stem(p.to_owned())),
                );
            }
            out.push(stem(whole.to_owned()));
        };
        for c in piece.chars() {
            if c.is_alphanumeric() || c == '_' {
                folded(c, &mut word);
            } else {
                flush(&mut word, &mut out);
            }
        }
        flush(&mut word, &mut out);
    }
    out
}

/// Words that are in every question and say nothing about it.
const STOP: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "by", "can", "do", "doe", "for", "from", "how", "i",
    "in", "is", "it", "of", "on", "or", "that", "the", "thi", "to", "what", "when", "where",
    "which", "why", "with",
];

/// The words of a query that are looked for, each once.
fn terms(query: &str) -> Vec<String> {
    let all = words(query);
    let mut kept: Vec<String> = Vec::new();
    for word in all.iter().filter(|w| !STOP.contains(&w.as_str())) {
        if !kept.contains(word) {
            kept.push(word.clone());
        }
    }
    // A query of nothing but such words is still a query.
    if kept.is_empty() {
        for word in all {
            if !kept.contains(&word) {
                kept.push(word);
            }
        }
    }
    kept
}

/// How well a word of a document answers a term: the word itself counts
/// in full, a longer word that starts with it (`paint` in `painting`) half.
fn answers(term: &str, word: &str) -> f64 {
    if word == term {
        1.0
    } else if term.len() >= 4 && word.starts_with(term) {
        0.5
    } else {
        0.0
    }
}

struct Indexed {
    section: Section,
    heading: Vec<String>,
    body: Vec<String>,
    /// How often each word is in the section, a heading's words counted
    /// [`HEADING_WEIGHT`] times.
    counts: HashMap<String, f64>,
    length: f64,
}

fn indexed(section: Section) -> Indexed {
    let heading = words(&section.heading);
    let body = if section.outline {
        Vec::new()
    } else {
        words(&section.body)
    };
    let mut counts: HashMap<String, f64> = HashMap::new();
    for word in &heading {
        *counts.entry(word.clone()).or_default() += HEADING_WEIGHT;
    }
    for word in &body {
        *counts.entry(word.clone()).or_default() += 1.0;
    }
    let length = (heading.len() + body.len()) as f64;
    Indexed {
        section,
        heading,
        body,
        counts,
        length,
    }
}

/// Whether `first` is directly followed by `second` somewhere in `words`.
fn adjacent(words: &[String], first: &str, second: &str) -> bool {
    words
        .windows(2)
        .any(|pair| answers(first, &pair[0]) > 0.0 && answers(second, &pair[1]) > 0.0)
}

/// How much the word of a document counts next to another's. The research
/// reports are what the architecture was decided from, and where the two
/// differ the architecture is right, so a report must say clearly more
/// about a question to be shown first.
fn standing(uri: &str) -> f64 {
    if uri.starts_with("ljos://docs/research/") {
        0.6
    } else {
        1.0
    }
}

/// The sections that say something about `terms`, best first, each with
/// its score (BM25: a word counts for more the fewer sections have it, and
/// for less the longer the section is).
fn ranked(index: &[Indexed], terms: &[String]) -> Vec<(f64, usize)> {
    const K1: f64 = 1.2;
    const B: f64 = 0.75;
    let count = index.len() as f64;
    let average = (index.iter().map(|d| d.length).sum::<f64>() / count.max(1.0)).max(1.0);
    let mut scores = vec![0.0f64; index.len()];
    let mut matched = vec![0usize; index.len()];
    for term in terms {
        let frequencies: Vec<f64> = index
            .iter()
            .map(|d| {
                d.counts
                    .iter()
                    .map(|(word, n)| answers(term, word) * n)
                    .sum()
            })
            .collect();
        let having = frequencies.iter().filter(|f| **f > 0.0).count() as f64;
        let rarity = (1.0 + (count - having + 0.5) / (having + 0.5)).ln();
        for (at, frequency) in frequencies.into_iter().enumerate() {
            if frequency > 0.0 {
                let length = 1.0 - B + B * index[at].length / average;
                scores[at] += rarity * frequency * (K1 + 1.0) / (frequency + K1 * length);
                matched[at] += 1;
            }
        }
    }
    let mut out: Vec<(f64, usize)> = Vec::new();
    for (at, score) in scores.into_iter().enumerate() {
        if matched[at] == 0 {
            continue;
        }
        // A section that has every word says more than one that has one
        // of them often; words that stand together as they were asked say
        // more still.
        let mut score = score * (0.25 + 0.75 * matched[at] as f64 / terms.len() as f64);
        for pair in terms.windows(2) {
            if adjacent(&index[at].heading, &pair[0], &pair[1])
                || adjacent(&index[at].body, &pair[0], &pair[1])
            {
                score *= 1.3;
            }
        }
        out.push((score * standing(&index[at].section.uri), at));
    }
    out.sort_by(|a, b| {
        b.0.total_cmp(&a.0).then_with(|| {
            let (a, b) = (&index[a.1].section, &index[b.1].section);
            (&a.uri, a.line).cmp(&(&b.uri, b.line))
        })
    });
    out
}

/// Hosts that serve Qt's source, or another toolkit's this project must
/// not read either.
const SOURCE_HOSTS: &[&str] = &[
    "code.qt.io",
    "codereview.qt-project.org",
    "github.com/qt/",
    "github.com/qtproject/",
    "invent.kde.org/qt/",
    "code.woboq.org",
    "codebrowser.dev/qt",
    "github.com/slint-ui/",
];

/// The links in `text`, each without the punctuation of the sentence
/// around it.
fn links(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("http") {
        let tail = &rest[at..];
        let end = tail
            .find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '`' | '\'' | '|'))
            .unwrap_or(tail.len());
        let mut link = &tail[..end];
        // A closing bracket belongs to the Markdown around the link unless
        // the link opened one.
        loop {
            let trimmed = link.trim_end_matches(['.', ',', ';', ':', '!', '?']);
            let trimmed = match trimmed.strip_suffix(')') {
                Some(inner) if !inner.contains('(') => inner,
                _ => trimmed,
            };
            let trimmed = trimmed.strip_suffix(']').unwrap_or(trimmed);
            if trimmed.len() == link.len() {
                break;
            }
            link = trimmed;
        }
        if link.starts_with("http://") || link.starts_with("https://") {
            out.push(link);
        }
        rest = &tail[link.len().max(4)..];
    }
    out
}

fn host_path(link: &str) -> &str {
    link.split_once("://").map_or(link, |(_, rest)| rest)
}

fn is_source(link: &str) -> bool {
    let rest = host_path(link);
    SOURCE_HOSTS.iter().any(|host| rest.starts_with(host))
}

fn qt_docs(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for link in links(text) {
        if host_path(link).starts_with("doc.qt.io/") && !out.iter().any(|l| l == link) {
            out.push(link.to_owned());
        }
    }
    out.truncate(5);
    out
}

/// The lines of `body` that say the most about `terms`, as one line of at
/// most [`SNIPPET_CHARS`] characters.
fn snippet(body: &str, terms: &[String]) -> String {
    let lines: Vec<&str> = body.lines().collect();
    let found: Vec<Vec<bool>> = lines
        .iter()
        .map(|line| {
            let words = words(line);
            terms
                .iter()
                .map(|term| words.iter().any(|word| answers(term, word) > 0.0))
                .collect()
        })
        .collect();
    // A line that has one of the words, and the most of them on it and the
    // few lines after it: the words of a question are seldom on one line of
    // a list or a wrapped paragraph, and the snippet is those lines.
    const NEAR: usize = 4;
    let mut best = lines.iter().position(|l| !l.trim().is_empty()).unwrap_or(0);
    let mut most = 0;
    for at in 0..lines.len() {
        if !found[at].iter().any(|hit| *hit) {
            continue;
        }
        let near = &found[at..lines.len().min(at + NEAR)];
        let count = (0..terms.len())
            .filter(|term| near.iter().any(|line| line[*term]))
            .count();
        if count > most {
            (most, best) = (count, at);
        }
    }
    // From the start of the sentence's paragraph when that is near, so the
    // snippet does not begin in the middle of a thought. An item of a list
    // or a row of a table is where its own thought begins.
    let begins = |line: &str| {
        let line = line.trim_start();
        line.starts_with(['-', '*', '|', '#', '>'])
            || line
                .trim_start_matches(|c: char| c.is_ascii_digit())
                .starts_with(". ")
    };
    let mut start = best;
    while start > 0
        && best - start < 2
        && !begins(lines[start])
        && !lines[start - 1].trim().is_empty()
    {
        start -= 1;
    }
    let mut text = String::new();
    for line in &lines[start..] {
        let line = line.trim();
        if line.is_empty() && text.chars().count() >= SNIPPET_CHARS / 2 {
            break;
        }
        if line.is_empty() || fence(line) {
            continue;
        }
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(line);
        if text.chars().count() >= SNIPPET_CHARS {
            break;
        }
    }
    for link in links(&text.clone()) {
        if is_source(link) {
            text = text.replace(link, "(a link to source code, not passed on)");
        }
    }
    if text.chars().count() > SNIPPET_CHARS {
        let cut: String = text.chars().take(SNIPPET_CHARS).collect();
        let cut = cut.rfind(' ').map_or(cut.as_str(), |at| &cut[..at]);
        text = format!("{cut} …");
    }
    text
}

/// Everything that is searched, as sections.
fn corpus(project: &Project) -> Vec<Section> {
    let mut out = Vec::new();
    let mut files = Vec::new();
    tools::collect(&project.docs_dir(), "md", &mut files);
    files.sort();
    for path in files {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let below = path.strip_prefix(project.docs_dir()).unwrap_or(&path);
        let below = below.to_string_lossy().replace('\\', "/");
        out.extend(sections(&format!("ljos://docs/{below}"), &below, &text));
    }
    if let Ok(text) = std::fs::read_to_string(project.features_readme()) {
        out.extend(sections(
            "ljos://features/README.md",
            "features/README.md",
            &text,
        ));
    }
    if let Ok(ledger) = Ledger::load(&project.ledger_path()) {
        for row in ledger.rows() {
            let Some(reason) = row.reason.as_deref().filter(|r| !r.trim().is_empty()) else {
                continue;
            };
            out.push(Section {
                uri: format!("ljos://ledger/{}/{}", row.module, row.type_name),
                addressable: false,
                title: format!("{} ({})", row.key(), row.status.as_str()),
                line: None,
                heading: row.key(),
                level: 1,
                body: format!("{reason}\n"),
                outline: false,
            });
        }
    }
    out
}

fn search(project: &Project, args: &SearchArgs) -> Result<SearchOut, SurfaceError> {
    let terms = terms(&args.query);
    if terms.is_empty() {
        return Err(SurfaceError::invalid_argument(
            "`query` has no words in it; ask in a few words, for example `binding tiers`",
        ));
    }
    let limit = args.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let index: Vec<Indexed> = corpus(project).into_iter().map(indexed).collect();
    let ranked = ranked(&index, &terms);
    let total = ranked.len();
    let hits: Vec<Hit> = ranked
        .into_iter()
        .map(|(score, at)| {
            let section = &index[at].section;
            let cited = format!("{}\n{}", section.heading, section.body);
            Hit {
                uri: section.uri.clone(),
                section: section
                    .line
                    .filter(|_| section.addressable)
                    .map(|line| format!("{}#L{line}", section.uri)),
                title: section.title.clone(),
                line: section.line,
                score: (score * 100.0).round() / 100.0,
                snippet: snippet(&section.body, &terms),
                qt_docs: qt_docs(&cited),
            }
        })
        .collect();
    let (results, next_cursor) = tools::page(hits, args.cursor.as_deref(), limit, tools::BUDGET)?;
    Ok(SearchOut {
        query: args.query.trim().to_owned(),
        results,
        total,
        next_cursor,
    })
}

pub(crate) const DESCRIPTION: &str = "Search the project's docs/, features/README.md and the reasons in the API ledger. Answers the sections that say the most, best first: title, line, a snippet, the `ljos://` resource to read in full and the Qt documentation pages the section cites.";

pub(crate) fn handler(project: &Arc<Project>) -> ToolHandler {
    let project = project.clone();
    typed(move |_: &CallCtx, args: SearchArgs| search(&project, &args))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "intro\n\n# Top\n\ntext\n\n## One\n\n```sh\n# not a heading\n```\n\n### Deep\n\ndeep text\n\n## Two\n\ntwo text\n";

    #[test]
    fn a_document_is_split_at_its_headings() {
        let found = sections("ljos://docs/a.md", "a.md", DOC);
        let titles: Vec<&str> = found.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(
            titles,
            ["a.md", "Top", "Top > One", "Top > One > Deep", "Top > Two"]
        );
        let lines: Vec<Option<u32>> = found.iter().map(|s| s.line).collect();
        assert_eq!(lines, [Some(1), Some(3), Some(7), Some(13), Some(17)]);
        assert!(found[2].body.contains("# not a heading"));
    }

    #[test]
    fn a_heading_over_headings_says_what_they_are() {
        let found = sections("u", "a.md", "# Top\n\n## One\n\ntext\n\n## Two\n\ntext\n");
        assert_eq!(found[0].body, "Sections: One; Two.\n");
    }

    #[test]
    fn a_section_is_read_down_to_its_next_peer() {
        assert_eq!(
            section_at(DOC, 7).unwrap(),
            "## One\n\n```sh\n# not a heading\n```\n\n### Deep\n\ndeep text\n\n"
        );
        assert_eq!(section_at(DOC, 13).unwrap(), "### Deep\n\ndeep text\n\n");
        assert_eq!(section_at(DOC, 17).unwrap(), "## Two\n\ntwo text\n");
        assert!(section_at(DOC, 99).is_none());
        assert!(section_at(DOC, 0).is_none());
    }

    #[test]
    fn words_are_folded_and_names_are_kept_whole() {
        assert_eq!(words("Ljós runs QML."), ["ljo", "run", "qml"]);
        assert_eq!(
            words("call `qml_check` twice"),
            ["call", "qml", "check", "qml_check", "twice"]
        );
        assert_eq!(
            words("see https://doc.qt.io/qt-6/x.html now"),
            ["see", "now"]
        );
        assert_eq!(terms("what is the Tier of tiers"), ["tier"]);
        assert_eq!(terms("what is it"), ["what", "is", "it"]);
    }

    #[test]
    fn a_link_ends_where_the_sentence_goes_on() {
        assert_eq!(
            links("at https://doc.qt.io/qt-6/a.html#b-prop, and (https://code.qt.io/x)."),
            [
                "https://doc.qt.io/qt-6/a.html#b-prop",
                "https://code.qt.io/x"
            ]
        );
        assert_eq!(
            links("[x](https://doc.qt.io/qt-6/a.html)"),
            ["https://doc.qt.io/qt-6/a.html"]
        );
        assert!(is_source("https://code.qt.io/cgit/qt/qtdeclarative.git"));
        assert!(!is_source("https://doc.qt.io/qt-6/qml-qtquick-text.html"));
    }

    #[test]
    fn a_snippet_is_short_and_starts_where_the_words_are() {
        let body = format!(
            "\nNothing here.\n\nThe tiers of a binding are three.\n{}\n",
            "More. ".repeat(100)
        );
        let text = snippet(&body, &["tier".to_owned(), "binding".to_owned()]);
        assert!(text.starts_with("The tiers of a binding"), "{text}");
        assert!(text.chars().count() <= SNIPPET_CHARS + 2);
        assert!(text.ends_with('…'));
    }

    #[test]
    fn a_snippet_starts_at_the_item_of_a_list_the_words_are_in() {
        let body = "Intro.\n\n- **Sizes.** Nothing of it.\n- **Popups.**\n  - Content goes to the overlay.\n  \
                    - `dim` follows `modal`.\n- **Last.** No.\n";
        let text = snippet(body, &["popup".to_owned(), "modal".to_owned()]);
        assert!(text.starts_with("- **Popups.**"), "{text}");
        assert!(text.contains("`dim` follows `modal`"), "{text}");
        // A wrapped paragraph is read from where it begins.
        let body = "Other.\n\nA popup that is\nmodal dims what is\nbehind it.\n";
        let text = snippet(body, &["modal".to_owned()]);
        assert!(text.starts_with("A popup that is modal"), "{text}");
    }

    #[test]
    fn a_heading_outranks_a_mention_and_all_words_outrank_one() {
        let index: Vec<Indexed> = sections(
            "u",
            "a.md",
            "# Doc\n\n## Hot reload\n\nFiles are watched.\n\n## Other\n\nA reload is hot when nothing restarts.\n\n## Third\n\nreload reload reload reload\n",
        )
        .into_iter()
        .map(indexed)
        .collect();
        let order: Vec<&str> = ranked(&index, &terms("hot reload"))
            .into_iter()
            .map(|(_, at)| index[at].section.heading.as_str())
            .collect();
        assert_eq!(order, ["Hot reload", "Other", "Third"]);
    }
}
