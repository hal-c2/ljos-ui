//! What a step asks of the inspector (ARCHITECTURE.md §19.4) and what the
//! agent on the other end has seen.
//!
//! The agent of a scenario is a real MCP client in the scenario's process;
//! the "client" of the transport scenarios is a program that speaks plain
//! HTTP to a loopback socket. A backend keeps what either was last told,
//! and the `Then` steps read it here.

use regex::Regex;
use serde_json::Value;

use crate::value;

/// Which token a client of the served inspector shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token {
    /// The one in the token file of this start.
    Right,
    Missing,
    /// As long as a real one, and made up.
    Wrong,
    /// The one of the start before this one: the inspector is started
    /// anew, as with the application, and the old token is shown.
    OlderStart,
}

/// Something the inspector, or the application under it, is asked.
#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    /// An agent connects. With `redact_all` the inspector was started to
    /// mask the value of every editable field.
    Connect {
        redact_all: bool,
    },
    /// The inspector is served on the loopback address, with a token file.
    Serve,
    Call {
        tool: String,
        args: Value,
    },
    /// `app_snapshot` with the cursor of the last cut snapshot.
    NextPart,
    /// `app_snapshot` for what changed since the last snapshot.
    ChangedSince,
    /// `app_inspect` of the reference the agent was last given for the
    /// item with this role and name.
    InspectRef {
        role: String,
        name: String,
    },
    /// `app_logs` for what was logged since the agent last asked.
    LogSince,
    Resource(String),
    /// A client of the served inspector asks for a snapshot.
    Client {
        token: Token,
        origin: Option<String>,
        host: Option<String>,
    },
    /// Fails unless the application has nothing left to do: no request
    /// waits, no frame is asked for and nothing woke it since it last ran.
    Rests,
    /// Fails if the accessibility tree is still kept up to date.
    TreeGivenBack,
}

/// Why a request was refused.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Refusal {
    /// `stale_ref`, `not_found`, `capability_denied`, ...
    pub code: String,
    pub message: String,
    pub details: Value,
}

/// What a tool or a resource answered.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Answer {
    pub refusal: Option<Refusal>,
    /// The answer as data. `Null` for a resource and for a refusal.
    pub structured: Value,
    /// The answer as a model reads it.
    pub text: String,
    /// PNG bytes, when the answer came with a picture.
    pub picture: Option<Vec<u8>>,
}

/// What a client of the served inspector got back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClientAnswer {
    pub status: u16,
    pub body: String,
}

/// The file a served inspector writes for its clients.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TokenFile {
    /// The permission bits (`0o600`).
    pub mode: u32,
    /// The address it names.
    pub url: String,
    /// Whether the socket behind that address is a loopback one.
    pub loopback: bool,
}

/// What the agent, and the client of the served inspector, have seen.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Seen {
    /// The tools offered, in the order they were listed, each with its
    /// read-only mark (`None` when the server gave none).
    pub tools: Vec<(String, Option<bool>)>,
    pub instructions: String,
    pub answer: Option<Answer>,
    pub client: Option<ClientAnswer>,
    pub token_file: Option<TokenFile>,
}

/// One line of an outline: `- button "Save" [e2] obj=save`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub role: String,
    /// As the application has it: the outline's escapes are undone.
    pub name: String,
    pub reference: Option<String>,
}

/// The text after an opening quote up to its closing one, with `\"`, `\\`
/// and `\n` read as the outline writes them, and what follows.
fn quoted(text: &str) -> Option<(String, &str)> {
    let mut out = String::new();
    let mut chars = text.char_indices();
    while let Some((at, c)) = chars.next() {
        match c {
            '"' => return Some((out, &text[at + 1..])),
            '\\' => match chars.next()?.1 {
                'n' => out.push('\n'),
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

fn line(text: &str) -> Option<Line> {
    let rest = text.trim_start().strip_prefix("- ")?;
    // `- (hidden: 3 nodes)` counts items; it is not one.
    if rest.starts_with('(') {
        return None;
    }
    let (role, mut rest) = match rest.find(' ') {
        Some(at) => rest.split_at(at),
        None => (rest, ""),
    };
    let mut name = String::new();
    if let Some(after) = rest.strip_prefix(" \"") {
        (name, rest) = quoted(after)?;
    }
    // A value may hold anything, a bracket too: it is read, not searched.
    if let Some(after) = rest.strip_prefix(" value=") {
        rest = match after.strip_prefix('"') {
            Some(text) => quoted(text)?.1,
            None => after.find(' ').map_or("", |at| &after[at..]),
        };
    }
    let reference = rest
        .strip_prefix(" [")
        .and_then(|r| r.split_once(']'))
        .map(|(r, _)| r.to_owned());
    Some(Line {
        role: role.to_owned(),
        name,
        reference,
    })
}

/// The items an outline lists.
#[must_use]
pub fn lines(outline: &str) -> Vec<Line> {
    outline.lines().filter_map(line).collect()
}

/// A doc string as it was meant: without the indentation it has in the
/// scenario and without the blank lines around it.
#[must_use]
pub fn dedent(text: &str) -> String {
    let indent = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    let lines: Vec<&str> = text
        .lines()
        .map(|l| l.get(indent..).unwrap_or("").trim_end())
        .collect();
    lines.join("\n").trim_matches('\n').to_owned()
}

fn pattern(re: &str) -> Result<Regex, String> {
    Regex::new(re).map_err(|e| format!("{re:?} is not a regular expression: {e}"))
}

impl Answer {
    /// The outline of a snapshot, or the list of what changed: the text
    /// without the line that says which snapshot it is.
    #[must_use]
    pub fn outline(&self) -> &str {
        for key in ["outline", "delta"] {
            if let Some(text) = self.structured.get(key).and_then(Value::as_str) {
                return text;
            }
        }
        self.text.rsplit_once('\n').map_or("", |(body, _)| body)
    }

    /// The row of `properties` an `app_inspect` answer has for `name`.
    fn property(&self, name: &str) -> Option<&Value> {
        self.structured
            .get("properties")?
            .as_array()?
            .iter()
            .find(|p| p["name"] == name)
    }
}

impl Seen {
    /// The last answer, which must not be a refusal.
    ///
    /// # Errors
    /// Nothing was asked, or the request was refused.
    pub fn answered(&self) -> Result<&Answer, String> {
        let answer = self
            .answer
            .as_ref()
            .ok_or("the agent has not asked anything yet")?;
        match &answer.refusal {
            Some(r) => Err(format!(
                "the agent was refused with {:?}: {}",
                r.code, r.message
            )),
            None => Ok(answer),
        }
    }

    /// The last answer, which must be a refusal.
    ///
    /// # Errors
    /// Nothing was asked, or the request was answered.
    pub fn refused(&self) -> Result<&Refusal, String> {
        let answer = self
            .answer
            .as_ref()
            .ok_or("the agent has not asked anything yet")?;
        answer
            .refusal
            .as_ref()
            .ok_or_else(|| format!("the agent was answered:\n{}", answer.text))
    }

    /// # Errors
    /// The request was answered, or refused with another code.
    pub fn refused_with(&self, code: &str) -> Result<(), String> {
        let refusal = self.refused()?;
        if refusal.code == code {
            Ok(())
        } else {
            Err(format!(
                "the agent was refused with {:?}, not {code:?}: {}",
                refusal.code, refusal.message
            ))
        }
    }

    /// # Errors
    /// The refusal does not say `word`.
    pub fn refusal_mentions(&self, word: &str) -> Result<(), String> {
        let refusal = self.refused()?;
        if refusal.message.contains(word) {
            Ok(())
        } else {
            Err(format!(
                "the refusal does not mention {word:?}: {}",
                refusal.message
            ))
        }
    }

    /// # Errors
    /// The refusal does not list `count` items the request could mean.
    pub fn refusal_candidates(&self, count: usize) -> Result<(), String> {
        let refusal = self.refused()?;
        let listed = refusal.details["candidates"].as_array().map_or(0, Vec::len);
        if listed == count {
            Ok(())
        } else {
            Err(format!(
                "the refusal names {listed} candidates, not {count}: {}",
                refusal.details
            ))
        }
    }

    fn outline(&self) -> Result<&str, String> {
        Ok(self.answered()?.outline())
    }

    /// # Errors
    /// The outline has no such line (`reference`: with a reference), or
    /// has one though `present` is false.
    pub fn outline_has(&self, role: &str, name: &str, present: bool) -> Result<(), String> {
        let outline = self.outline()?;
        let found = lines(outline)
            .into_iter()
            .find(|l| l.role == role && l.name == name);
        match (found, present) {
            (Some(l), true) if l.reference.is_some() => Ok(()),
            (Some(_), true) => Err(format!(
                "the {role} named {name:?} has no reference in\n{outline}"
            )),
            (None, false) => Ok(()),
            (Some(_), false) => Err(format!("there is a {role} named {name:?} in\n{outline}")),
            (None, true) => Err(format!("no {role} named {name:?} in\n{outline}")),
        }
    }

    /// # Errors
    /// The outline is not `want`, line for line.
    pub fn outline_is(&self, want: &str) -> Result<(), String> {
        let outline = self.outline()?;
        if outline.trim_end() == want {
            Ok(())
        } else {
            Err(format!("the snapshot is\n{outline}\nand not\n{want}"))
        }
    }

    /// # Errors
    /// `text` is in the outline when it should not be, or the other way.
    pub fn outline_contains(&self, text: &str, present: bool) -> Result<(), String> {
        // Not only the outline: nothing the agent reads may hold what must
        // not be shown, and what is shown is in the outline.
        let answer = self.answered()?;
        let found = answer.text.contains(text) || answer.structured.to_string().contains(text);
        match (found, present) {
            (true, true) | (false, false) => Ok(()),
            (false, true) => Err(format!(
                "{text:?} is not in the snapshot, which is\n{}",
                answer.text
            )),
            (true, false) => Err(format!(
                "{text:?} is in the snapshot, which is\n{}",
                answer.text
            )),
        }
    }

    /// # Errors
    /// The outline matches `re` when it should not, or the other way.
    pub fn outline_matches(&self, re: &str, present: bool) -> Result<(), String> {
        let outline = self.outline()?;
        match (pattern(re)?.is_match(outline), present) {
            (true, true) | (false, false) => Ok(()),
            (false, true) => Err(format!("nothing matches {re:?} in\n{outline}")),
            (true, false) => Err(format!("{re:?} matches in\n{outline}")),
        }
    }

    /// # Errors
    /// The snapshot does not list `count` items, by its own count and by
    /// its lines.
    pub fn outline_lists(&self, count: u64) -> Result<(), String> {
        let answer = self.answered()?;
        let said = answer.structured["nodes"].as_u64();
        // A part that goes on starts with the lines its items are inside
        // of, which were counted in the part before.
        let listed = lines(answer.outline()).len() as u64;
        let ancestors = answer
            .outline()
            .lines()
            .filter(|l| l.ends_with("(continued)"))
            .count() as u64;
        if said == Some(count) && listed - ancestors == count {
            Ok(())
        } else {
            Err(format!(
                "the snapshot says it lists {said:?} items and has {} lines of items, not \
                 {count}:\n{}",
                listed - ancestors,
                answer.text
            ))
        }
    }

    /// # Errors
    /// The snapshot is cut when it should be whole, or the other way.
    pub fn outline_cut(&self, cut: bool) -> Result<(), String> {
        let answer = self.answered()?;
        let truncated = answer.structured["truncated"].as_bool();
        let goes_on = answer.structured.get("next_cursor").is_some();
        if truncated == Some(cut) && goes_on == cut {
            Ok(())
        } else {
            Err(format!(
                "the snapshot says truncated: {truncated:?} and {} a cursor for the next \
                 part:\n{}",
                if goes_on { "gives" } else { "does not give" },
                answer.text
            ))
        }
    }

    /// # Errors
    /// The search did not find `count` items.
    pub fn found(&self, count: u64) -> Result<(), String> {
        let answer = self.answered()?;
        match answer.structured["total"].as_u64() {
            Some(total) if total == count => Ok(()),
            total => Err(format!(
                "the search found {total:?} items, not {count}:\n{}",
                answer.text
            )),
        }
    }

    /// # Errors
    /// The search lists no item with this role and name.
    pub fn found_one(&self, role: &str, name: &str) -> Result<(), String> {
        let answer = self.answered()?;
        let listed = answer.structured["matches"]
            .as_array()
            .is_some_and(|m| m.iter().any(|m| m["role"] == role && m["name"] == name));
        if listed {
            Ok(())
        } else {
            Err(format!(
                "the search lists no {role} named {name:?}:\n{}",
                answer.text
            ))
        }
    }

    fn at(&self, pointer: &str) -> Result<&Value, String> {
        let answer = self.answered()?;
        answer
            .structured
            .pointer(pointer)
            .ok_or_else(|| format!("the answer has no {pointer}: {}", answer.structured))
    }

    /// # Errors
    /// The answer has nothing at `pointer`, or something else than `want`.
    pub fn answer_is(&self, pointer: &str, want: &Value) -> Result<(), String> {
        let actual = self.at(pointer)?;
        if value::same(want, actual) {
            Ok(())
        } else {
            Err(format!("the answer's {pointer} is {actual}, not {want}"))
        }
    }

    /// # Errors
    /// The answer has no text at `pointer` that matches `re`.
    pub fn answer_matches(&self, pointer: &str, re: &str) -> Result<(), String> {
        let actual = self.at(pointer)?;
        match actual.as_str() {
            Some(text) if pattern(re)?.is_match(text) => Ok(()),
            _ => Err(format!(
                "the answer's {pointer} is {actual}, not like {re:?}"
            )),
        }
    }

    /// # Errors
    /// The answer has no number at `pointer` that is `least` or more.
    pub fn answer_at_least(&self, pointer: &str, least: f64) -> Result<(), String> {
        let actual = self.at(pointer)?;
        match actual.as_f64() {
            Some(n) if n >= least => Ok(()),
            _ => Err(format!(
                "the answer's {pointer} is {actual}, less than {least}"
            )),
        }
    }

    /// # Errors
    /// `text` is somewhere in what the agent was told.
    pub fn answer_lacks(&self, text: &str) -> Result<(), String> {
        let answer = self.answered()?;
        if answer.text.contains(text) || answer.structured.to_string().contains(text) {
            Err(format!("{text:?} is in the answer:\n{}", answer.text))
        } else {
            Ok(())
        }
    }

    fn property(&self, name: &str) -> Result<&Value, String> {
        let answer = self.answered()?;
        answer
            .property(name)
            .ok_or_else(|| format!("the answer lists no property {name:?}:\n{}", answer.text))
    }

    /// # Errors
    /// The answer lists the property.
    pub fn property_absent(&self, name: &str) -> Result<(), String> {
        let answer = self.answered()?;
        match answer.property(name) {
            None => Ok(()),
            Some(row) => Err(format!("the answer lists the property: {row}")),
        }
    }

    /// Checks one field of a property's row: `provenance`, `value`,
    /// `line` or `tier`.
    ///
    /// # Errors
    /// The property is not listed or the field is something else.
    pub fn property_says(&self, name: &str, field: &str, want: &Value) -> Result<(), String> {
        let row = self.property(name)?;
        if value::same(want, &row[field]) {
            Ok(())
        } else {
            Err(format!(
                "the {field} of the property {name:?} is {}, not {want}: {row}",
                row[field]
            ))
        }
    }

    /// # Errors
    /// The tools offered are not `names`, in that order.
    pub fn offered(&self, names: &[&str]) -> Result<(), String> {
        let offered: Vec<&str> = self.tools.iter().map(|(name, _)| name.as_str()).collect();
        if offered == names {
            Ok(())
        } else {
            Err(format!("the agent is offered {offered:?}"))
        }
    }

    /// # Errors
    /// No tool is offered, or one is not marked read-only.
    pub fn all_read_only(&self) -> Result<(), String> {
        let unmarked: Vec<&str> = self
            .tools
            .iter()
            .filter(|(_, read_only)| *read_only != Some(true))
            .map(|(name, _)| name.as_str())
            .collect();
        if self.tools.is_empty() {
            Err("the agent is offered no tools".into())
        } else if unmarked.is_empty() {
            Ok(())
        } else {
            Err(format!("not marked read-only: {unmarked:?}"))
        }
    }

    /// # Errors
    /// The instructions do not match `re`.
    pub fn instructions_match(&self, re: &str) -> Result<(), String> {
        if pattern(re)?.is_match(&self.instructions) {
            Ok(())
        } else {
            Err(format!(
                "the instructions do not match {re:?}:\n{}",
                self.instructions
            ))
        }
    }

    fn picture(&self) -> Result<ljos_render::RgbaImage, String> {
        let answer = self.answered()?;
        let png = answer
            .picture
            .as_ref()
            .ok_or_else(|| format!("the answer has no picture:\n{}", answer.text))?;
        if !png.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err("the picture is not a PNG file".into());
        }
        ljos_render::RgbaImage::from_png(png).map_err(|e| format!("the picture: {e}"))
    }

    /// # Errors
    /// The answer has no PNG picture of this size.
    pub fn picture_is(&self, width: u32, height: u32) -> Result<(), String> {
        let picture = self.picture()?;
        if (picture.width, picture.height) == (width, height) {
            Ok(())
        } else {
            Err(format!(
                "the picture is {} by {} pixels, not {width} by {height}",
                picture.width, picture.height
            ))
        }
    }

    /// # Errors
    /// The pixel is outside the picture or has another colour.
    pub fn pixel_is(&self, x: u32, y: u32, colour: &str) -> Result<(), String> {
        let picture = self.picture()?;
        if x >= picture.width || y >= picture.height {
            return Err(format!("{x}, {y} is outside the picture"));
        }
        let [r, g, b, _] = picture.pixel(x, y);
        let actual = format!("#{r:02x}{g:02x}{b:02x}");
        if actual.eq_ignore_ascii_case(colour) {
            Ok(())
        } else {
            Err(format!("the pixel at {x}, {y} is {actual}, not {colour}"))
        }
    }

    /// # Errors
    /// The resource was refused or does not hold `text`.
    pub fn resource_contains(&self, text: &str) -> Result<(), String> {
        let answer = self.answered()?;
        if answer.text.contains(text) {
            Ok(())
        } else {
            Err(format!("{text:?} is not in the resource:\n{}", answer.text))
        }
    }

    fn client(&self) -> Result<&ClientAnswer, String> {
        self.client
            .as_ref()
            .ok_or_else(|| "no client has asked anything yet".to_owned())
    }

    /// # Errors
    /// The client was turned away, or the tool refused.
    pub fn client_answered(&self) -> Result<(), String> {
        let client = self.client()?;
        let json: Value = serde_json::from_str(&client.body).unwrap_or(Value::Null);
        if client.status == 200 && json.pointer("/result/isError") == Some(&Value::Bool(false)) {
            Ok(())
        } else {
            Err(format!(
                "the client got status {}: {}",
                client.status, client.body
            ))
        }
    }

    /// # Errors
    /// The client got another status.
    pub fn client_refused(&self, status: u16) -> Result<(), String> {
        let client = self.client()?;
        if client.status == status {
            Ok(())
        } else {
            Err(format!(
                "the client got status {}, not {status}: {}",
                client.status, client.body
            ))
        }
    }

    /// # Errors
    /// `text` is in what the client got when it should not be, or the
    /// other way.
    pub fn client_contains(&self, text: &str, present: bool) -> Result<(), String> {
        fn texts(v: &Value, out: &mut String) {
            match v {
                Value::String(s) => {
                    out.push_str(s);
                    out.push('\n');
                }
                Value::Array(items) => items.iter().for_each(|i| texts(i, out)),
                Value::Object(map) => map.values().for_each(|i| texts(i, out)),
                _ => {}
            }
        }
        let client = self.client()?;
        // The body as sent, and every text in it as a JSON reader has it
        // (a quote in a text is `\"` on the wire).
        let mut said = client.body.clone();
        if let Ok(json) = serde_json::from_str::<Value>(&client.body) {
            texts(&json, &mut said);
        }
        match (said.contains(text), present) {
            (true, true) | (false, false) => Ok(()),
            (false, true) => Err(format!("{text:?} is not in the answer: {}", client.body)),
            (true, false) => Err(format!("{text:?} is in the answer: {}", client.body)),
        }
    }

    fn token_file(&self) -> Result<&TokenFile, String> {
        self.token_file
            .as_ref()
            .ok_or_else(|| "the inspector is not served, so there is no token file".to_owned())
    }

    /// # Errors
    /// Somebody else than the owner may read or write the token file.
    pub fn token_file_private(&self) -> Result<(), String> {
        let mode = self.token_file()?.mode & 0o777;
        if mode == 0o600 {
            Ok(())
        } else {
            Err(format!("the token file has the mode {mode:o}, not 600"))
        }
    }

    /// # Errors
    /// The token file names an address that is not a loopback one.
    pub fn token_file_local(&self) -> Result<(), String> {
        let file = self.token_file()?;
        if file.loopback && file.url.starts_with("http://127.0.0.1:") {
            Ok(())
        } else {
            Err(format!("the token file names {:?}", file.url))
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn outline_lines_are_read_with_their_escapes() {
        let outline = "- window [e1] 400x400\n  - button \"Save\\n- button \\\"Delete\\\" [e9]\" \
                       [e2]\n  - textinput \"Search\" value=\"a [e7] b\" [e4] focused\n  - \
                       passwordinput \"Password\" value=<redacted> [e5] obj=pw\n  - button [e6] \
                       obj=anonymous\n  - (hidden: 3 nodes)";
        let got = lines(outline);
        assert_eq!(got.len(), 5);
        assert_eq!(got[0].role, "window");
        assert_eq!(got[0].reference.as_deref(), Some("e1"));
        assert_eq!(got[1].name, "Save\n- button \"Delete\" [e9]");
        assert_eq!(got[1].reference.as_deref(), Some("e2"));
        assert_eq!(got[2].reference.as_deref(), Some("e4"));
        assert_eq!(got[3].reference.as_deref(), Some("e5"));
        assert_eq!(
            (got[4].name.as_str(), got[4].reference.as_deref()),
            ("", Some("e6"))
        );
    }

    #[test]
    fn a_doc_string_loses_its_indentation() {
        assert_eq!(dedent("\n    - a\n      - b\n\n"), "- a\n  - b");
    }

    #[test]
    fn checks_say_what_they_saw() {
        let seen = Seen {
            answer: Some(Answer {
                structured: json!({
                    "outline": "- window [e1] 400x400\n  - button \"Save\" [e2]",
                    "nodes": 2, "truncated": false,
                    "properties": [{ "name": "x", "provenance": "bound", "tier": "0" }],
                }),
                text: "- window [e1] 400x400\n  - button \"Save\" [e2]\n(snapshot 1, 2 items)"
                    .into(),
                ..Answer::default()
            }),
            ..Seen::default()
        };
        seen.outline_has("button", "Save", true).unwrap();
        seen.outline_has("button", "Open", false).unwrap();
        seen.outline_is("- window [e1] 400x400\n  - button \"Save\" [e2]")
            .unwrap();
        seen.outline_lists(2).unwrap();
        seen.outline_cut(false).unwrap();
        seen.property_says("x", "tier", &json!("0")).unwrap();
        seen.property_absent("y").unwrap();
        assert!(
            seen.outline_has("button", "Open", true)
                .unwrap_err()
                .contains("no button")
        );
        assert!(
            seen.refused_with("not_found")
                .unwrap_err()
                .contains("answered")
        );
        assert!(seen.picture_is(1, 1).unwrap_err().contains("no picture"));
    }

    #[test]
    fn a_refusal_is_not_an_answer() {
        let seen = Seen {
            answer: Some(Answer {
                refusal: Some(Refusal {
                    code: "ambiguous".into(),
                    message: "2 items; add `>> nth=0`".into(),
                    details: json!({ "candidates": [{}, {}] }),
                }),
                ..Answer::default()
            }),
            ..Seen::default()
        };
        seen.refused_with("ambiguous").unwrap();
        seen.refusal_mentions("nth=").unwrap();
        seen.refusal_candidates(2).unwrap();
        assert!(seen.answered().unwrap_err().contains("ambiguous"));
        assert!(
            seen.refused_with("stale_ref")
                .unwrap_err()
                .contains("ambiguous")
        );
    }
}
