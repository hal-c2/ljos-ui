//! The mask the inspector puts over what a hidden field holds wherever the
//! application repeats it: in an act's answer, a line it logged, a fault
//! it threw.
//!
//! A line or a fault outlives the field's value: the field may be cleared
//! or changed, or go, before somebody asks for the log. So the secrets
//! every look at the scene found are kept ([`SecretBook`]), and what is
//! told later is masked with all of them, not only the ones there now.

use std::collections::{HashSet, VecDeque};

use ljos_mcp_core::SurfaceError;
use serde_json::Value;

use super::capture::REDACTED;

/// Secrets kept. The oldest are forgotten after this many, or once they
/// hold more than [`MAX_SECRET_BYTES`] between them: a field typed into
/// for hours must not grow the inspector without end.
const MAX_SECRETS: usize = 1024;
const MAX_SECRET_BYTES: usize = 1 << 20;

/// Replaces every secret that is in `v`, inside strings too, by the mask.
/// `secrets` is longest first so that a secret that contains another is
/// masked whole. The keys are the answer's own and are left as they are;
/// what the application made is masked with [`mask_data`].
pub(crate) fn mask_json(v: &mut Value, secrets: &[String]) {
    match v {
        Value::String(s) => mask_text(s, secrets),
        Value::Array(items) => items.iter_mut().for_each(|i| mask_json(i, secrets)),
        Value::Object(map) => map.values_mut().for_each(|i| mask_json(i, secrets)),
        _ => {}
    }
}

/// [`mask_json`] for a value the application made (what a script or a
/// method answered), whose keys are its too: a script may build
/// `{[pw.text]: 1}`.
pub(crate) fn mask_data(v: &mut Value, secrets: &[String]) {
    match v {
        Value::String(s) => mask_text(s, secrets),
        Value::Array(items) => items.iter_mut().for_each(|i| mask_data(i, secrets)),
        Value::Object(map) => {
            map.values_mut().for_each(|i| mask_data(i, secrets));
            mask_keys(map, secrets);
        }
        _ => {}
    }
}

/// Masks the keys of `map`. Two keys that mask alike must not become one,
/// which would drop a value and tell that the secrets were the same, so a
/// masked key that is taken is numbered (`<redacted>#2`). The keys that
/// keep their text are placed first: a masked key never takes the place
/// of one of them.
fn mask_keys(map: &mut serde_json::Map<String, Value>, secrets: &[String]) {
    let masked = |k: &str| {
        let mut m = k.to_string();
        mask_text(&mut m, secrets);
        m
    };
    if !map.keys().any(|k| masked(k) != *k) {
        return;
    }
    let old = std::mem::take(map);
    let mut hidden = Vec::new();
    for (k, v) in old {
        let m = masked(&k);
        if m == k {
            map.insert(k, v);
        } else {
            hidden.push((m, v));
        }
    }
    for (m, v) in hidden {
        let mut key = m.clone();
        let mut n = 2usize;
        while map.contains_key(&key) {
            key = format!("{m}#{n}");
            n += 1;
        }
        map.insert(key, v);
    }
}

pub(crate) fn mask_text(s: &mut String, secrets: &[String]) {
    if secrets.iter().any(|x| x == s) {
        *s = REDACTED.to_string();
        return;
    }
    for secret in secrets {
        if s.contains(secret.as_str()) {
            *s = s.replace(secret.as_str(), REDACTED);
        }
    }
}

/// A refusal is an answer too: what a method threw or a script said may
/// hold what a field hides.
pub(crate) fn mask_error(e: &mut SurfaceError, secrets: &[String]) {
    mask_text(&mut e.message, secrets);
    if let Some(details) = e.details.as_mut() {
        mask_json(details, secrets);
    }
}

/// Every secret the inspector has seen a field hold, the oldest first.
#[derive(Debug, Default)]
pub(crate) struct SecretBook {
    order: VecDeque<String>,
    known: HashSet<String>,
    bytes: usize,
    /// `known`, longest first, as the masks want it.
    longest_first: Vec<String>,
}

impl SecretBook {
    /// Adds what `secrets` holds that is not known yet, and returns those,
    /// longest first: what was written down before must be masked with
    /// them too.
    pub(crate) fn learn<'a>(
        &mut self,
        secrets: impl IntoIterator<Item = &'a String>,
    ) -> Vec<String> {
        let mut new: Vec<String> = secrets
            .into_iter()
            .filter(|s| !s.is_empty() && !self.known.contains(*s))
            .cloned()
            .collect();
        if new.is_empty() {
            return new;
        }
        new.sort_unstable();
        new.dedup();
        for s in &new {
            self.bytes += s.len();
            self.known.insert(s.clone());
            self.order.push_back(s.clone());
        }
        while self.order.len() > MAX_SECRETS
            || (self.bytes > MAX_SECRET_BYTES && self.order.len() > 1)
        {
            if let Some(old) = self.order.pop_front() {
                self.bytes -= old.len();
                self.known.remove(&old);
            }
        }
        self.longest_first = self.order.iter().cloned().collect();
        self.longest_first
            .sort_by_key(|s| std::cmp::Reverse(s.len()));
        new.sort_by_key(|s| std::cmp::Reverse(s.len()));
        new
    }

    pub(crate) fn secrets(&self) -> &[String] {
        &self.longest_first
    }

    pub(crate) fn mask_text(&self, s: &mut String) {
        mask_text(s, &self.longest_first);
    }

    pub(crate) fn mask_json(&self, v: &mut Value) {
        mask_json(v, &self.longest_first);
    }

    pub(crate) fn mask_error(&self, e: &mut SurfaceError) {
        mask_error(e, &self.longest_first);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_stays_known_after_its_field_lets_it_go() {
        let mut book = SecretBook::default();
        let new = book.learn(&["hunter2".to_string()]);
        assert_eq!(new, ["hunter2"]);
        // The field now holds another secret; the first one is kept.
        assert_eq!(book.learn(&["swordfish".to_string()]), ["swordfish"]);
        assert!(book.learn(&["hunter2".to_string()]).is_empty());
        let mut s = "was hunter2, is swordfish".to_string();
        book.mask_text(&mut s);
        assert_eq!(s, "was <redacted>, is <redacted>");
    }

    #[test]
    fn keys_are_masked_without_two_becoming_one() {
        let secrets = vec!["swordfish".to_string(), "hunter2".to_string()];
        let mut v = serde_json::json!({
            "hunter2": 1,
            "swordfish": 2,
            "<redacted>": 3,
            "is hunter2": { "swordfish": ["hunter2"] },
            "plain": "kept",
        });
        mask_data(&mut v, &secrets);
        let text = v.to_string();
        assert!(
            !text.contains("hunter2") && !text.contains("swordfish"),
            "{text}"
        );
        let map = v.as_object().unwrap();
        assert_eq!(map.len(), 5, "{text}");
        assert_eq!(
            map["<redacted>"], 3,
            "the application's own key keeps its place"
        );
        assert_eq!(map["plain"], "kept");
        let mut numbers: Vec<_> = ["<redacted>#2", "<redacted>#3"]
            .iter()
            .map(|k| map[*k].as_i64().unwrap())
            .collect();
        numbers.sort_unstable();
        assert_eq!(numbers, [1, 2]);
        assert_eq!(
            map["is <redacted>"],
            serde_json::json!({ "<redacted>": ["<redacted>"] })
        );
    }

    #[test]
    fn the_book_forgets_the_oldest_when_full() {
        let mut book = SecretBook::default();
        let many: Vec<String> = (0..MAX_SECRETS + 5).map(|i| format!("s{i:05}")).collect();
        book.learn(&many);
        assert_eq!(book.secrets().len(), MAX_SECRETS);
        let mut s = "s00000 s01028".to_string();
        book.mask_text(&mut s);
        assert_eq!(s, "s00000 <redacted>");
    }
}
