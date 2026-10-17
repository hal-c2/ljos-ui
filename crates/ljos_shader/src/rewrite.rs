//! Where naga's GLSL front end and the Vulkan GLSL that `qsb` takes part:
//!
//! - a combined sampler (`uniform sampler2D source;`) is a texture and a
//!   sampler in naga, so the declaration becomes both and each use
//!   `sampler2D(source_ljos_t, source_ljos_s)`;
//! - `out gl_PerVertex { … };` is a redeclaration naga refuses; it says
//!   nothing the shader needs.
//!
//! Comments are blanked first, keeping their line breaks, so naga's line
//! numbers stay the source's.

/// The texture half of a rewritten sampler.
pub(crate) const TEXTURE_SUFFIX: &str = "_ljos_t";
const SAMPLER_SUFFIX: &str = "_ljos_s";
/// Samplers are bound after every texture, so the two never share a slot.
const SAMPLER_BINDING_BASE: u32 = 1000;

const SAMPLER_TYPES: [(&str, &str); 4] = [
    ("sampler2D", "texture2D"),
    ("sampler2DArray", "texture2DArray"),
    ("sampler3D", "texture3D"),
    ("samplerCube", "textureCube"),
];

pub(crate) fn rewrite(source: &str) -> String {
    let text = drop_per_vertex(&blank_comments(source));
    let mut out = String::with_capacity(text.len() + 256);
    let mut combined: Vec<(String, &str)> = Vec::new();
    let mut rest = text.as_str();
    let mut next_binding = 0u32;
    while let Some((start, end, decl)) = find_sampler_decl(rest) {
        out.push_str(&rest[..start]);
        let binding = decl.binding.unwrap_or(next_binding);
        next_binding = next_binding.max(binding + 1);
        out.push_str(&format!(
            "layout(binding = {binding}) uniform {tex} {name}{TEXTURE_SUFFIX}; \
             layout(binding = {sb}) uniform sampler {name}{SAMPLER_SUFFIX};",
            tex = decl.texture,
            name = decl.name,
            sb = SAMPLER_BINDING_BASE + binding,
        ));
        combined.push((decl.name.to_owned(), decl.ty));
        rest = &rest[end..];
    }
    out.push_str(rest);
    for (name, ty) in combined {
        out = replace_word(
            &out,
            &name,
            &format!("{ty}({name}{TEXTURE_SUFFIX}, {name}{SAMPLER_SUFFIX})"),
        );
    }
    out
}

struct Decl<'a> {
    name: &'a str,
    ty: &'static str,
    texture: &'static str,
    binding: Option<u32>,
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The next `[layout(…)] uniform samplerX name;`: its span and parts.
fn find_sampler_decl(text: &str) -> Option<(usize, usize, Decl<'_>)> {
    let mut from = 0;
    while let Some(at) = find_word(&text[from..], "uniform").map(|i| i + from) {
        from = at + "uniform".len();
        let after = text[from..].trim_start();
        let word_len = after.find(|c: char| !is_ident(c)).unwrap_or(after.len());
        let word = &after[..word_len];
        let Some(&(ty, texture)) = SAMPLER_TYPES.iter().find(|(t, _)| *t == word) else {
            continue;
        };
        let after = after[word_len..].trim_start();
        let name_len = after.find(|c: char| !is_ident(c)).unwrap_or(after.len());
        let name = &after[..name_len];
        let tail = after[name_len..].trim_start();
        if name.is_empty() || !tail.starts_with(';') {
            continue;
        }
        let end = text.len() - tail.len() + 1;
        let (start, binding) = layout_before(text, at);
        return Some((
            start,
            end,
            Decl {
                name,
                ty,
                texture,
                binding,
            },
        ));
    }
    None
}

/// A `layout(…)` just before `at`: where it starts and its binding.
fn layout_before(text: &str, at: usize) -> (usize, Option<u32>) {
    let before = text[..at].trim_end();
    if !before.ends_with(')') {
        return (at, None);
    }
    let Some(open) = before.rfind('(') else {
        return (at, None);
    };
    let head = before[..open].trim_end();
    if !head.ends_with("layout") {
        return (at, None);
    }
    let start = head.len() - "layout".len();
    let binding = before[open + 1..before.len() - 1].split(',').find_map(|q| {
        let (k, v) = q.split_once('=')?;
        (k.trim() == "binding").then(|| v.trim().parse().ok())?
    });
    (start, binding)
}

fn find_word(text: &str, word: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = text[from..].find(word).map(|i| i + from) {
        let before = text[..i].chars().next_back();
        let after = text[i + word.len()..].chars().next();
        if !before.is_some_and(is_ident) && !after.is_some_and(is_ident) {
            return Some(i);
        }
        from = i + word.len();
    }
    None
}

fn replace_word(text: &str, word: &str, with: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = find_word(rest, word) {
        out.push_str(&rest[..i]);
        out.push_str(with);
        rest = &rest[i + word.len()..];
    }
    out.push_str(rest);
    out
}

fn blank_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, chars.peek()) {
            ('/', Some('/')) => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut star = false;
                for c in chars.by_ref() {
                    if c == '\n' {
                        out.push('\n');
                    }
                    if star && c == '/' {
                        break;
                    }
                    star = c == '*';
                }
                out.push(' ');
            }
            _ => out.push(c),
        }
    }
    out
}

/// Blanks `out gl_PerVertex { … };`, keeping its line breaks.
fn drop_per_vertex(text: &str) -> String {
    let Some(at) = find_word(text, "gl_PerVertex") else {
        return text.to_owned();
    };
    let start = text[..at]
        .trim_end()
        .strip_suffix("out")
        .map_or(at, |s| s.len());
    let Some(end) = text[at..].find("};").map(|i| at + i + 2) else {
        return text.to_owned();
    };
    let gap: String = text[start..end].chars().filter(|c| *c == '\n').collect();
    format!("{}{gap}{}", &text[..start], &text[end..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_combined_sampler_becomes_a_texture_and_a_sampler() {
        let src = "layout(binding = 1) uniform sampler2D source; // the picture\n\
                   void main() { c = texture(source, uv); }\n";
        let out = rewrite(src);
        assert!(out.contains("layout(binding = 1) uniform texture2D source_ljos_t;"));
        assert!(out.contains("layout(binding = 1001) uniform sampler source_ljos_s;"));
        assert!(out.contains("texture(sampler2D(source_ljos_t, source_ljos_s), uv)"));
        assert_eq!(out.lines().count(), src.lines().count());
    }

    #[test]
    fn gl_per_vertex_goes_and_the_lines_stay() {
        let src = "a;\nout gl_PerVertex {\n vec4 gl_Position;\n};\nb;\n";
        let out = rewrite(src);
        assert!(!out.contains("gl_PerVertex"));
        assert_eq!(out.lines().count(), src.lines().count());
    }
}
