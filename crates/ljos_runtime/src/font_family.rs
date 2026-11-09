//! How one text names font families: as a style sheet writes a family, a
//! comma-separated list of names, some quoted. A lone `font.family` and a
//! style sheet's family are read this way where text is laid out
//! (`ljos_text`), and a font value (`ljos_engine`) asks the same rule which
//! names it can keep as such a text, so the two never disagree.

/// The names `text` lists, in order, each trimmed and unquoted. A part
/// with nothing in it comes out empty; it names no family.
pub fn family_names(text: &str) -> impl Iterator<Item = &str> {
    text.split(',')
        .map(|part| part.trim().trim_matches(['"', '\'']))
}

/// Whether `name`, read by [`family_names`], names itself and nothing else:
/// no comma, no quote or space at either end, and not empty. A list of such
/// a name alone means what that name as one family text means.
pub fn names_only_itself(name: &str) -> bool {
    let mut names = family_names(name).filter(|n| !n.is_empty());
    names.next() == Some(name) && names.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_lists_its_names_trimmed_and_unquoted() {
        let names: Vec<_> = family_names(" A , \"B C\",'D',").collect();
        assert_eq!(names, ["A", "B C", "D", ""]);
    }

    #[test]
    fn only_a_plain_name_names_itself() {
        assert!(names_only_itself("Liberation Mono"));
        for name in ["", "A, B", "\"A\"", "'A'", " A", "A ", "A,", ","] {
            assert!(!names_only_itself(name), "{name:?}");
        }
    }
}
