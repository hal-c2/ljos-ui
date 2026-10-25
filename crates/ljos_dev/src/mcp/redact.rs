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
/// masked whole.
pub(crate) fn mask_json(v: &mut Value, secrets: &[String]) {
    match v {
        Value::String(s) => mask_text(s, secrets),
        Value::Array(items) => items.iter_mut().for_each(|i| mask_json(i, secrets)),
        Value::Object(map) => map.values_mut().for_each(|i| mask_json(i, secrets)),
        _ => {}
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
