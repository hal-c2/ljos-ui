//! The text a `url` property makes of a string, as Qt's tolerant parsing
//! writes it for a person to read (`QUrl::toString`), learned from qml6 as
//! a black box: what no address can be is empty, what an address cannot
//! hold is in percent codes, and a percent code that needs none is decoded.

use std::borrow::Cow;

/// What `text` reads as once a url holds it; `None` when it is no address
/// (a scheme that starts with no letter, a host with a space, a port past
/// 65535), which reads as empty. Text that needs nothing is borrowed.
pub(crate) fn url_text(text: &str) -> Option<Cow<'_, str>> {
    let first_end = text.find(['/', '?', '#']).unwrap_or(text.len());
    let (scheme, rest) = match text[..first_end].find(':') {
        Some(at) => {
            let scheme = &text[..at];
            if !is_scheme(scheme) {
                return None;
            }
            (Some(scheme), &text[at + 1..])
        }
        None => (None, text),
    };
    let (authority, tail) = match rest.strip_prefix("//") {
        Some(r) => {
            let end = r.find(['/', '?', '#']).unwrap_or(r.len());
            (Some(&r[..end]), &r[end..])
        }
        None => (None, rest),
    };
    let authority = match authority {
        Some(a) => Some(authority_text(a)?),
        None => None,
    };
    let tail_out = escaped(tail);
    let unchanged = scheme.is_none_or(|s| !s.bytes().any(|b| b.is_ascii_uppercase()))
        && authority
            .as_ref()
            .is_none_or(|a| matches!(a, Cow::Borrowed(_)))
        && matches!(tail_out, Cow::Borrowed(_));
    if unchanged {
        return Some(Cow::Borrowed(text));
    }
    let mut out = String::with_capacity(text.len() + 8);
    if let Some(s) = scheme {
        out.push_str(&s.to_ascii_lowercase());
        out.push(':');
    }
    if let Some(a) = authority {
        out.push_str("//");
        out.push_str(&a);
    }
    out.push_str(&tail_out);
    Some(Cow::Owned(out))
}

fn is_scheme(s: &str) -> bool {
    let mut b = s.bytes();
    b.next().is_some_and(|c| c.is_ascii_alphabetic())
        && b.all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'-' | b'.'))
}

/// `user@host:port`: the host lower case, its percent codes decoded, the
/// port without leading zeros and gone when empty.
fn authority_text(a: &str) -> Option<Cow<'_, str>> {
    let (user, hostport) = match a.rfind('@') {
        Some(at) => (Some(&a[..at]), &a[at + 1..]),
        None => (None, a),
    };
    let (host, port) = if hostport.starts_with('[') {
        let close = hostport.find(']')?;
        let v6 = &hostport[1..close];
        if v6.is_empty()
            || !v6
                .bytes()
                .all(|b| b.is_ascii_hexdigit() || b == b':' || b == b'.')
        {
            return None;
        }
        match &hostport[close + 1..] {
            "" => (&hostport[..=close], None),
            after => (&hostport[..=close], Some(after.strip_prefix(':')?)),
        }
    } else {
        match hostport.find(':') {
            Some(at) => (&hostport[..at], Some(&hostport[at + 1..])),
            None => (hostport, None),
        }
    };
    let port = match port {
        None => None,
        Some(p) => {
            let n = if p.is_empty() {
                None
            } else if p.bytes().all(|b| b.is_ascii_digit()) {
                Some(p.parse::<u32>().ok().filter(|n| *n <= 65535)?)
            } else {
                return None;
            };
            // Written again unless it is written as Qt writes it.
            Some((n, p.is_empty() || (p.starts_with('0') && p != "0")))
        }
    };
    let host_out: Cow<str> = if host.starts_with('[') {
        Cow::Borrowed(host)
    } else {
        let decoded = decoded_host(host)?;
        if decoded.chars().any(|c| {
            c.is_ascii() && !(c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~'))
        }) {
            return None;
        }
        if decoded.chars().any(char::is_uppercase) {
            Cow::Owned(decoded.to_lowercase())
        } else {
            decoded
        }
    };
    let user_out = user.map(escaped);
    if matches!(host_out, Cow::Borrowed(_))
        && user_out
            .as_ref()
            .is_none_or(|u| matches!(u, Cow::Borrowed(_)))
        && port.is_none_or(|(_, rewritten)| !rewritten)
    {
        return Some(Cow::Borrowed(a));
    }
    let mut out = String::new();
    if let Some(u) = user_out {
        out.push_str(&u);
        out.push('@');
    }
    out.push_str(&host_out);
    if let Some((Some(n), _)) = port {
        out.push(':');
        out.push_str(&n.to_string());
    }
    Some(Cow::Owned(out))
}

fn decoded_host(host: &str) -> Option<Cow<'_, str>> {
    if !host.contains('%') {
        return Some(Cow::Borrowed(host));
    }
    let mut bytes = Vec::with_capacity(host.len());
    let b = host.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            bytes.push(hex_pair(b, i)?);
            i += 3;
        } else {
            bytes.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(bytes).ok().map(Cow::Owned)
}

fn hex_pair(b: &[u8], at: usize) -> Option<u8> {
    let d = |i: usize| b.get(i).and_then(|&c| char::from(c).to_digit(16));
    Some((d(at + 1)? * 16 + d(at + 2)?) as u8)
}

/// A path, query or fragment: what no url holds (a quote, angle brackets,
/// a backslash, caret, backquote, braces, bar or a control) in percent
/// codes, a lone `%` as `%25`, and a code that stands for a letter, digit,
/// `-._~`, a space or a whole character outside ASCII decoded.
fn escaped(s: &str) -> Cow<'_, str> {
    let needs = s.bytes().any(|b| {
        matches!(
            b,
            b'"' | b'<' | b'>' | b'\\' | b'^' | b'`' | b'{' | b'|' | b'}' | b'%'
        ) || b < 0x20
            || b == 0x7f
    });
    if !needs {
        return Cow::Borrowed(s);
    }
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(s.len() + 8);
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'%' {
            match hex_pair(b, i) {
                None => out.extend_from_slice(b"%25"),
                Some(d)
                    if d.is_ascii_alphanumeric()
                        || matches!(d, b'-' | b'.' | b'_' | b'~' | b' ') =>
                {
                    out.push(d);
                    i += 3;
                    continue;
                }
                Some(d) if d >= 0x80 => {
                    // As many codes as make one character, or none.
                    let mut run = Vec::new();
                    let mut j = i;
                    while let Some(x) = (b.get(j) == Some(&b'%')).then(|| hex_pair(b, j)).flatten()
                    {
                        if x < 0x80 {
                            break;
                        }
                        run.push(x);
                        j += 3;
                    }
                    match std::str::from_utf8(&run) {
                        Ok(text) => {
                            out.extend_from_slice(text.as_bytes());
                            i = j;
                        }
                        Err(e) => {
                            let good = e.valid_up_to();
                            out.extend_from_slice(&run[..good]);
                            i += good * 3;
                            push_code(&mut out, b[i + 1], b[i + 2]);
                            i += 3;
                        }
                    }
                    continue;
                }
                Some(_) => {
                    push_code(&mut out, b[i + 1], b[i + 2]);
                    i += 3;
                    continue;
                }
            }
            i += 1;
            continue;
        }
        if matches!(
            c,
            b'"' | b'<' | b'>' | b'\\' | b'^' | b'`' | b'{' | b'|' | b'}'
        ) || c < 0x20
            || c == 0x7f
        {
            out.extend_from_slice(format!("%{c:02X}").as_bytes());
        } else {
            out.push(c);
        }
        i += 1;
    }
    // Only ASCII was put in or taken out whole, so the bytes are UTF-8.
    Cow::Owned(String::from_utf8(out).unwrap_or_default())
}

fn push_code(out: &mut Vec<u8>, h: u8, l: u8) {
    out.push(b'%');
    out.push(h.to_ascii_uppercase());
    out.push(l.to_ascii_uppercase());
}

#[cfg(test)]
mod tests {
    use super::url_text;

    /// Each as qml6 writes it (Qt 6.11.2); `""` where it is no address.
    #[test]
    fn text_is_written_as_qt_writes_a_url() {
        let cases = [
            ("0:0", ""),
            ("a b:c", ""),
            ("a:b", "a:b"),
            ("a1+.-:x", "a1+.-:x"),
            ("1a:x", ""),
            (":x", ""),
            ("x:", "x:"),
            ("/a:b", "/a:b"),
            ("a/b:c", "a/b:c"),
            ("?a:b", "?a:b"),
            ("[\"\"]", "[%22%22]"),
            ("{}.png", "%7B%7D.png"),
            ("a<b>c", "a%3Cb%3Ec"),
            ("a\\b", "a%5Cb"),
            ("a|b^c`d", "a%7Cb%5Ec%60d"),
            ("a%b", "a%25b"),
            ("a%2", "a%252"),
            ("a%41", "aA"),
            ("a\tb", "a%09b"),
            ("a b", "a b"),
            ("a[b]", "a[b]"),
            ("a%2fb", "a%2Fb"),
            ("a%20b", "a b"),
            ("a%c3%a9", "aé"),
            ("a%e9", "a%E9"),
            ("a%7e%2D%5F%2E%30%61", "a~-_.0a"),
            ("?q=%41&x=\"", "?q=A&x=%22"),
            ("http://ex ample.org/", ""),
            ("http://[::1]/", "http://[::1]/"),
            ("http://[x]/", ""),
            ("http://x:99999/", ""),
            ("http://x:ab/", ""),
            ("http://x:/", "http://x/"),
            ("http://x:0080/", "http://x:80/"),
            ("http://a!b/", ""),
            ("http://a%41b/", "http://aab/"),
            ("http://user:pw@x/", "http://user:pw@x/"),
            ("x://a:b:c/", ""),
            ("HTTP://X.ORG/A", "http://x.org/A"),
            ("http://ÉX.org/", "http://éx.org/"),
            ("C:/dir/f", "c:/dir/f"),
            ("http://x/é{", "http://x/é%7B"),
            ("", ""),
        ];
        for (given, seen) in cases {
            assert_eq!(url_text(given).as_deref().unwrap_or(""), seen, "{given:?}");
        }
    }
}
