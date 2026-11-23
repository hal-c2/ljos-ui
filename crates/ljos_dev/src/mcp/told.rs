//! The warnings the inspector has told, as it told them: masked.
//!
//! A later change may bring a warning back after every book forgot the
//! secret it quoted. Masking the compiler's text anew would then tell
//! that secret, so a warning told before is told again from what was
//! told, never from the compiler's text. The compiler's text is not kept:
//! a warning is found again by a fingerprint of it.

use std::collections::{HashMap, VecDeque};
use std::hash::{BuildHasher, RandomState};

use ljos_compiler::CompileFault;

use super::redact;

/// Warnings kept. The oldest are forgotten after this many, or once their
/// text holds more than [`MAX_TOLD_BYTES`]: a session that runs for days
/// must not grow the inspector without end. One forgotten is told again
/// as a warning never told is (`Inspector::set_warnings`).
const MAX_TOLD: usize = 1024;
const MAX_TOLD_BYTES: usize = 1 << 20;

#[derive(Debug, Default)]
pub(crate) struct ToldWarnings {
    /// Keyed so that nobody can make two warnings' fingerprints alike on
    /// purpose; by chance, one in 2^64.
    keys: RandomState,
    told: HashMap<u64, CompileFault>,
    order: VecDeque<u64>,
    bytes: usize,
}

impl ToldWarnings {
    /// Where the compiler puts a warning may move while its text stays:
    /// only the text is the warning's.
    fn fingerprint(&self, raw: &CompileFault) -> u64 {
        self.keys.hash_one((&raw.file, &raw.message))
    }

    /// The warning `raw` as it was last told, at `raw`'s place.
    pub(crate) fn get(&self, raw: &CompileFault) -> Option<CompileFault> {
        self.told.get(&self.fingerprint(raw)).map(|m| CompileFault {
            file: m.file.clone(),
            message: m.message.clone(),
            ..raw.clone()
        })
    }

    /// Keeps `masked` as what `raw` was told as.
    pub(crate) fn keep(&mut self, raw: &CompileFault, masked: &CompileFault) {
        let key = self.fingerprint(raw);
        self.bytes += size(masked);
        match self.told.insert(key, masked.clone()) {
            // Told before: it keeps its place among the oldest.
            Some(old) => self.bytes -= size(&old),
            None => self.order.push_back(key),
        }
        while self.order.len() > MAX_TOLD || self.bytes > MAX_TOLD_BYTES {
            let Some(old) = self.order.pop_front() else {
                break;
            };
            if let Some(old) = self.told.remove(&old) {
                self.bytes -= size(&old);
            }
        }
    }

    /// Masks `secrets` in what was told before they were known.
    pub(crate) fn mask(&mut self, secrets: &[String]) {
        for fault in self.told.values_mut() {
            let before = size(fault);
            redact::mask_text(&mut fault.file, secrets);
            redact::mask_text(&mut fault.message, secrets);
            self.bytes = self.bytes - before + size(fault);
        }
    }

    pub(crate) fn faults(&self) -> impl Iterator<Item = &CompileFault> {
        self.told.values()
    }
}

fn size(fault: &CompileFault) -> usize {
    fault.file.len() + fault.message.len()
}
