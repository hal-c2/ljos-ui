//! The `# block` comments of a `qmldir`:
//!
//! ```text
//! module App.Media
//! Player 1.0 Player.qml
//! # block Player 1.0 Player.qml Media "Plays one source"
//! ```
//!
//! `# block <Name> <major.minor> <File.qml> [<Category>] ["<summary>"]`.
//! The category is one word; the summary is in double quotes, `\"` and
//! `\\` escaped, and may come without a category. A comment, because Qt
//! fails an import on a first word it does not know and reads past a
//! comment.

/// One `# block` comment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockDecl {
    pub name: String,
    pub version: crate::Version,
    /// Relative to the `qmldir`'s directory.
    pub file: String,
    pub category: Option<String>,
    pub summary: Option<String>,
}

/// A `# block` comment that does not say a block.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("line {line}: {message}")]
pub struct DeclError {
    /// 1-based.
    pub line: u32,
    pub message: String,
}

/// What a `qmldir` says of blocks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QmldirBlocks {
    /// The `module` line's URI; `None` for a directory imported by path.
    pub module: Option<String>,
    /// Each with its 1-based line.
    pub blocks: Vec<(u32, BlockDecl)>,
    pub errors: Vec<DeclError>,
}

/// Reads the module and the `# block` comments of a `qmldir`'s `text`.
pub fn read_qmldir(text: &str) -> QmldirBlocks {
    let mut out = QmldirBlocks::default();
    for (i, line) in text.lines().enumerate() {
        let number = u32::try_from(i + 1).unwrap_or(u32::MAX);
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix('#') {
            match block(rest.trim_start()) {
                None => {}
                Some(Ok(decl)) => out.blocks.push((number, decl)),
                Some(Err(message)) => out.errors.push(DeclError {
                    line: number,
                    message,
                }),
            }
            continue;
        }
        let code = trimmed.split('#').next().unwrap_or(trimmed);
        if let ["module", uri] = code.split_whitespace().collect::<Vec<_>>().as_slice() {
            out.module = Some((*uri).to_string());
        }
    }
    out
}

/// `None` for a comment that is not a `block` one.
fn block(comment: &str) -> Option<Result<BlockDecl, String>> {
    let rest = comment.strip_prefix("block")?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    Some(decl(rest.trim()))
}

fn decl(text: &str) -> Result<BlockDecl, String> {
    const GRAMMAR: &str =
        "a block is `# block <Name> <major.minor> <File.qml> [<Category>] [\"<summary>\"]`";
    let (head, summary) = match text.find('"') {
        Some(at) => (&text[..at], Some(quoted(&text[at..])?)),
        None => (text, None),
    };
    let words: Vec<&str> = head.split_whitespace().collect();
    let (name, version, file, category) = match words.as_slice() {
        [name, version, file] => (*name, *version, *file, None),
        [name, version, file, category] => (*name, *version, *file, Some(*category)),
        _ => return Err(GRAMMAR.to_string()),
    };
    if !name.starts_with(|c: char| c.is_ascii_uppercase())
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(format!("`{name}` is not a type name; {GRAMMAR}"));
    }
    let version = crate::Version::parse(version)
        .filter(|_| version.contains('.'))
        .ok_or_else(|| format!("`{version}` is not a version like 1.0; {GRAMMAR}"))?;
    if !file.ends_with(".qml") {
        return Err(format!("`{file}` is not a .qml file; {GRAMMAR}"));
    }
    Ok(BlockDecl {
        name: name.to_string(),
        version,
        file: file.to_string(),
        category: category.map(str::to_string),
        summary,
    })
}

/// The string `text` starts with, which must also end the line.
fn quoted(text: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut chars = text[1..].char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some((_, c @ ('"' | '\\'))) => out.push(c),
                _ => return Err("a summary escapes only `\\\"` and `\\\\`".to_string()),
            },
            '"' => {
                return if text[1 + i + 1..].trim().is_empty() {
                    Ok(out)
                } else {
                    Err("nothing may follow a block's summary".to_string())
                };
            }
            c => out.push(c),
        }
    }
    Err("a block's summary is not closed with `\"`".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Version;

    fn one(line: &str) -> Result<BlockDecl, String> {
        let read = read_qmldir(line);
        match (read.blocks.as_slice(), read.errors.as_slice()) {
            ([(_, decl)], []) => Ok(decl.clone()),
            ([], [e]) => Err(e.message.clone()),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_block_with_all_its_parts() {
        let text = "module App.Media\n\
            Player 1.0 Player.qml\n\
            # block Player 1.0 Player.qml Media \"Plays one \\\"source\\\"\"\n";
        let read = read_qmldir(text);
        assert_eq!(read.module.as_deref(), Some("App.Media"));
        assert_eq!(
            read.blocks,
            [(
                3,
                BlockDecl {
                    name: "Player".into(),
                    version: Version { major: 1, minor: 0 },
                    file: "Player.qml".into(),
                    category: Some("Media".into()),
                    summary: Some("Plays one \"source\"".into()),
                }
            )]
        );
    }

    #[test]
    fn category_and_summary_are_optional() {
        let d = one("#block Clock 2.3 time/Clock.qml").unwrap();
        assert_eq!(
            (d.version, d.file.as_str()),
            (Version { major: 2, minor: 3 }, "time/Clock.qml")
        );
        assert_eq!((d.category, d.summary), (None, None));
        let d = one("# block Clock 1.0 Clock.qml \"Tells the time\"").unwrap();
        assert_eq!(
            (d.category, d.summary.as_deref()),
            (None, Some("Tells the time"))
        );
    }

    #[test]
    fn other_comments_are_not_blocks() {
        for text in [
            "# blocks Player 1.0 Player.qml",
            "# the block below",
            "Player 1.0 Player.qml",
        ] {
            let read = read_qmldir(text);
            assert!(read.blocks.is_empty() && read.errors.is_empty(), "{text}");
        }
    }

    #[test]
    fn a_malformed_block_says_what_is_wrong() {
        assert!(
            one("# block player 1.0 Player.qml")
                .unwrap_err()
                .contains("`player` is not a type name")
        );
        assert!(
            one("# block Player one Player.qml")
                .unwrap_err()
                .contains("`one` is not a version")
        );
        assert!(
            one("# block Player 1.0 Player.js")
                .unwrap_err()
                .contains("not a .qml file")
        );
        assert!(
            one("# block Player 1.0")
                .unwrap_err()
                .starts_with("a block is")
        );
        assert!(
            one("# block Player 1.0 P.qml \"open")
                .unwrap_err()
                .contains("not closed")
        );
        assert!(
            one("# block Player 1.0 P.qml \"a\" b")
                .unwrap_err()
                .contains("nothing may follow")
        );
        assert!(
            one("# block Player 1.0 P.qml A B")
                .unwrap_err()
                .starts_with("a block is")
        );
    }
}
