use std::{fmt, str::FromStr};

use serde::{Serialize, Serializer};

/// One right a client can be granted (§19.7). Enforcement is in the server;
/// tool annotations are only hints.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Cap {
    Inspect,
    Input,
    Mutate,
    Eval,
    Fs,
    Run,
}

impl Cap {
    pub const ALL: [Cap; 6] = [
        Cap::Inspect,
        Cap::Input,
        Cap::Mutate,
        Cap::Eval,
        Cap::Fs,
        Cap::Run,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Cap::Inspect => "inspect",
            Cap::Input => "input",
            Cap::Mutate => "mutate",
            Cap::Eval => "eval",
            Cap::Fs => "fs",
            Cap::Run => "run",
        }
    }

    fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

impl fmt::Display for Cap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for Cap {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

#[derive(Debug, thiserror::Error)]
#[error("unknown capability `{0}` (expected one of inspect, input, mutate, eval, fs, run)")]
pub struct ParseCapError(String);

impl FromStr for Cap {
    type Err = ParseCapError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Cap::ALL
            .into_iter()
            .find(|c| c.as_str() == s)
            .ok_or_else(|| ParseCapError(s.to_owned()))
    }
}

/// A set of granted capabilities. `Default` is empty; servers choose their
/// own baseline (`inspect` for the inspector, nothing extra for authoring).
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct CapSet(u8);

impl CapSet {
    pub const EMPTY: CapSet = CapSet(0);

    pub fn of(caps: impl IntoIterator<Item = Cap>) -> Self {
        caps.into_iter().fold(Self::EMPTY, |s, c| s.with(c))
    }

    #[must_use]
    pub fn with(self, cap: Cap) -> Self {
        CapSet(self.0 | cap.bit())
    }

    pub fn contains(self, cap: Cap) -> bool {
        self.0 & cap.bit() != 0
    }

    /// Granted capabilities in canonical order.
    pub fn iter(self) -> impl Iterator<Item = Cap> {
        Cap::ALL.into_iter().filter(move |c| self.contains(*c))
    }

    /// Parses `inspect,input` (comma separated, whitespace tolerated).
    pub fn parse_list(list: &str) -> Result<Self, ParseCapError> {
        list.split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::parse)
            .collect::<Result<Vec<Cap>, _>>()
            .map(Self::of)
    }
}

impl fmt::Debug for CapSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.iter()).finish()
    }
}

impl FromIterator<Cap> for CapSet {
    fn from_iter<T: IntoIterator<Item = Cap>>(iter: T) -> Self {
        Self::of(iter)
    }
}

impl Serialize for CapSet {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_iterate_in_canonical_order() {
        let set = CapSet::parse_list("run, inspect").unwrap();
        assert_eq!(set.iter().collect::<Vec<_>>(), [Cap::Inspect, Cap::Run]);
        assert!(!set.contains(Cap::Fs));
        assert!(CapSet::parse_list("").unwrap().iter().next().is_none());
        assert!(CapSet::parse_list("root").is_err());
    }
}
