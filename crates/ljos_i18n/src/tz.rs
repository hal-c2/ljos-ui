//! A time zone by its IANA name (`Europe/Berlin`): its offset from UTC,
//! its abbreviation and whether it keeps daylight saving time at any
//! instant, and the instants a local time is (none where the clocks skip
//! it, two where they repeat it).
//!
//! Its history is the platform's zone database, read as tzfile(5) and RFC
//! 9636 describe it: the transitions the file lists, then the POSIX rule
//! at its end for every year after them. Nothing of the database is built
//! in. Where the platform has no database (Windows) no zone is known by
//! name, and `TZDIR` can point at one.

use std::path::{Path, PathBuf};

/// A zone the database knows, with every transition it has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeZone {
    id: String,
    /// Seconds after the UNIX epoch at which the type of the same index in
    /// `kinds` begins, ascending.
    transitions: Vec<i64>,
    kinds: Vec<u8>,
    types: Vec<Kind>,
    /// The time before the first transition.
    first: Kind,
    /// The years after the last transition.
    rule: Option<Rule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Kind {
    offset: i32,
    dst: bool,
    abbreviation: String,
}

/// The instants a local time is in a zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalTime {
    /// The clocks skipped it: it is in a gap.
    None,
    One(i64),
    /// The clocks went back over it: the earlier instant, then the later.
    Both(i64, i64),
}

impl TimeZone {
    /// The zone the database names `id` (`America/New_York`, `UTC`,
    /// `EST5EDT`), as the name is spelled there.
    #[must_use]
    pub fn named(id: &str) -> Option<Self> {
        if !is_zone_id(id) {
            return None;
        }
        zoneinfo_dirs().find_map(|dir| Self::from_file(id, &dir.join(id)))
    }

    /// The local zone, as the C library chooses it: `TZ` (a name, a file,
    /// or a POSIX rule such as `CET-1CEST,M3.5.0,M10.5.0/3`), else
    /// `/etc/localtime`.
    #[must_use]
    pub fn local() -> Option<Self> {
        match std::env::var("TZ") {
            Ok(tz) if tz.is_empty() => Self::named("UTC").or_else(|| Self::posix("UTC0")),
            Ok(tz) => {
                let tz = tz.strip_prefix(':').unwrap_or(&tz);
                if tz.starts_with('/') {
                    Self::from_file(tz, Path::new(tz))
                } else {
                    Self::named(tz).or_else(|| Self::posix(tz))
                }
            }
            Err(_) => Self::from_file("localtime", Path::new("/etc/localtime")),
        }
    }

    /// A zone a POSIX rule makes, with no history before it.
    #[must_use]
    pub fn posix(rule: &str) -> Option<Self> {
        let rule = Rule::parse(rule)?;
        Some(Self {
            id: rule.text.clone(),
            transitions: Vec::new(),
            kinds: Vec::new(),
            types: Vec::new(),
            first: rule.std.clone(),
            rule: Some(rule),
        })
    }

    /// A zone from tzfile(5) data, named `id`.
    #[must_use]
    pub fn from_tzif(id: &str, data: &[u8]) -> Option<Self> {
        let mut zone = parse_tzif(data)?;
        zone.id = id.to_owned();
        Some(zone)
    }

    fn from_file(id: &str, path: &Path) -> Option<Self> {
        let data = std::fs::read(path).ok()?;
        Self::from_tzif(id, &data)
    }

    /// The name it was found by.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Seconds east of UTC at `unix_ms`, milliseconds after the epoch.
    #[must_use]
    pub fn offset_at(&self, unix_ms: i64) -> i32 {
        self.kind_at(unix_ms.div_euclid(1000)).offset
    }

    /// The database's abbreviation at `unix_ms` (`CEST`; `+0545` where it
    /// has none but the offset).
    #[must_use]
    pub fn abbreviation_at(&self, unix_ms: i64) -> &str {
        &self.kind_at(unix_ms.div_euclid(1000)).abbreviation
    }

    /// Whether the zone keeps daylight saving time at `unix_ms`.
    #[must_use]
    pub fn is_daylight_time_at(&self, unix_ms: i64) -> bool {
        self.kind_at(unix_ms.div_euclid(1000)).dst
    }

    /// The instants, in milliseconds after the epoch, at which the zone's
    /// clocks read `local_ms` (a local time counted as if it were UTC).
    #[must_use]
    pub fn from_local(&self, local_ms: i64) -> LocalTime {
        // A transition moves the clocks by less than a day, so the offsets
        // a day either side are the ones the local time can be at.
        let mut found = [None, None];
        for probe in [local_ms - 86_400_000, local_ms, local_ms + 86_400_000] {
            let offset = i64::from(self.offset_at(probe)) * 1000;
            let utc = local_ms - offset;
            if i64::from(self.offset_at(utc)) * 1000 == offset && !found.contains(&Some(utc)) {
                match found {
                    [None, _] => found[0] = Some(utc),
                    _ => found[1] = Some(utc),
                }
            }
        }
        match found {
            [Some(a), Some(b)] => LocalTime::Both(a.min(b), a.max(b)),
            [Some(a), None] => LocalTime::One(a),
            _ => LocalTime::None,
        }
    }

    /// The one instant Qt takes `local_ms` to be: the offset in force
    /// before a transition decides, so a skipped time is read as the
    /// standard time it would have been, a repeated one as its first.
    #[must_use]
    pub fn resolve_local(&self, local_ms: i64) -> i64 {
        match self.from_local(local_ms) {
            LocalTime::One(t) | LocalTime::Both(t, _) => t,
            LocalTime::None => {
                let before = self.offset_at(local_ms - 86_400_000);
                let after = self.offset_at(local_ms + 86_400_000);
                local_ms - i64::from(before.min(after)) * 1000
            }
        }
    }

    /// Every abbreviation the zone has had, with the offset it had then.
    pub(crate) fn abbreviations(&self) -> impl Iterator<Item = (&str, i32)> {
        let rule = self
            .rule
            .iter()
            .flat_map(|r| std::iter::once(&r.std).chain(r.dst.as_ref().map(|d| &d.0)));
        std::iter::once(&self.first)
            .chain(&self.types)
            .chain(rule)
            .map(|k| (k.abbreviation.as_str(), k.offset))
            .filter(|(name, _)| !name.is_empty())
    }

    fn kind_at(&self, t: i64) -> &Kind {
        let i = self.transitions.partition_point(|&at| at <= t);
        match &self.rule {
            Some(rule) if i == self.transitions.len() => rule.kind_at(t),
            _ if i == 0 => &self.first,
            _ => self
                .types
                .get(usize::from(self.kinds[i - 1]))
                .unwrap_or(&self.first),
        }
    }
}

/// A name the database could have: no way out of its directory.
fn is_zone_id(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('/')
        && id
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/_+-".contains(&b))
}

fn zoneinfo_dirs() -> impl Iterator<Item = PathBuf> {
    let env = std::env::var_os("TZDIR").map(PathBuf::from);
    env.into_iter().chain(
        [
            "/usr/share/zoneinfo",
            "/usr/lib/zoneinfo",
            "/usr/share/lib/zoneinfo",
            "/etc/zoneinfo",
        ]
        .into_iter()
        .map(PathBuf::from),
    )
}

struct Reader<'d> {
    data: &'d [u8],
}

impl<'d> Reader<'d> {
    fn take(&mut self, n: usize) -> Option<&'d [u8]> {
        if self.data.len() < n {
            return None;
        }
        let (head, tail) = self.data.split_at(n);
        self.data = tail;
        Some(head)
    }

    fn u32(&mut self) -> Option<usize> {
        let b = self.take(4)?;
        usize::try_from(u32::from_be_bytes([b[0], b[1], b[2], b[3]])).ok()
    }
}

struct Header {
    version: u8,
    isut: usize,
    isstd: usize,
    leap: usize,
    time: usize,
    types: usize,
    chars: usize,
}

fn header(r: &mut Reader<'_>) -> Option<Header> {
    if r.take(4)? != b"TZif" {
        return None;
    }
    let version = r.take(16)?[0];
    Some(Header {
        version,
        isut: r.u32()?,
        isstd: r.u32()?,
        leap: r.u32()?,
        time: r.u32()?,
        types: r.u32()?,
        chars: r.u32()?,
    })
}

fn parse_tzif(data: &[u8]) -> Option<TimeZone> {
    let mut r = Reader { data };
    let mut h = header(&mut r)?;
    let mut wide = false;
    if h.version >= b'2' {
        // The version 1 block holds 32-bit times only: the 64-bit block
        // after it is the whole history.
        r.take(h.time * 5 + h.types * 6 + h.chars + h.leap * 8 + h.isstd + h.isut)?;
        h = header(&mut r)?;
        wide = true;
    }
    let size = if wide { 8 } else { 4 };
    let times = r.take(h.time * size)?;
    let transitions = times
        .chunks_exact(size)
        .map(|c| match c {
            [a, b, c, d] => i64::from(i32::from_be_bytes([*a, *b, *c, *d])),
            _ => i64::from_be_bytes(c.try_into().unwrap_or([0; 8])),
        })
        .collect();
    let kinds = r.take(h.time)?.to_vec();
    let raw = r.take(h.types * 6)?;
    let chars = r.take(h.chars)?;
    r.take(h.leap * (size + 4) + h.isstd + h.isut)?;
    let types: Vec<Kind> = raw
        .as_chunks::<6>()
        .0
        .iter()
        .map(|t| {
            let at = &chars[usize::from(t[5]).min(chars.len())..];
            let end = at.iter().position(|&c| c == 0).unwrap_or(at.len());
            Kind {
                offset: i32::from_be_bytes([t[0], t[1], t[2], t[3]]),
                dst: t[4] != 0,
                abbreviation: String::from_utf8_lossy(&at[..end]).into_owned(),
            }
        })
        .collect();
    if kinds.iter().any(|&k| usize::from(k) >= types.len()) {
        return None;
    }
    let rule = wide
        .then(|| {
            let footer = r.data.strip_prefix(b"\n")?;
            let end = footer.iter().position(|&c| c == b'\n')?;
            Rule::parse(std::str::from_utf8(&footer[..end]).ok()?)
        })
        .flatten();
    let first = types
        .first()
        .cloned()
        .or_else(|| rule.as_ref().map(|r| r.std.clone()))?;
    Some(TimeZone {
        id: String::new(),
        transitions,
        kinds,
        types,
        first,
        rule,
    })
}

/// A POSIX `TZ` rule: standard time, and daylight saving time between two
/// days of every year.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    text: String,
    std: Kind,
    dst: Option<(Kind, Day, i32, Day, i32)>,
}

/// A day of a year, as a rule names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Day {
    /// `Jn`: 1 to 365, never 29 February.
    Julian(u32),
    /// `n`: 0 to 365, 29 February counted.
    Ordinal(u32),
    /// `Mm.w.d`: day `d` (0 is Sunday) of week `w` (5 is the last) of
    /// month `m`.
    Month(u32, u32, u32),
}

impl Rule {
    fn parse(text: &str) -> Option<Self> {
        let mut s = text;
        let std_name = name(&mut s)?;
        let std_offset = -posix_time(&mut s)?;
        let std = Kind {
            offset: std_offset,
            dst: false,
            abbreviation: std_name,
        };
        if s.is_empty() {
            return Some(Self {
                text: text.to_owned(),
                std,
                dst: None,
            });
        }
        let dst_name = name(&mut s)?;
        let dst_offset = if s.starts_with(',') {
            std_offset + 3600
        } else {
            -posix_time(&mut s)?
        };
        let dst = Kind {
            offset: dst_offset,
            dst: true,
            abbreviation: dst_name,
        };
        // Without dates, the rules of the United States, as POSIX says an
        // implementation may choose.
        let (start, start_at, end, end_at) = if s.is_empty() {
            (Day::Month(3, 2, 0), 7200, Day::Month(11, 1, 0), 7200)
        } else {
            s = s.strip_prefix(',')?;
            let (start, start_at) = transition(&mut s)?;
            s = s.strip_prefix(',')?;
            let (end, end_at) = transition(&mut s)?;
            if !s.is_empty() {
                return None;
            }
            (start, start_at, end, end_at)
        };
        Some(Self {
            text: text.to_owned(),
            std,
            dst: Some((dst, start, start_at, end, end_at)),
        })
    }

    fn kind_at(&self, t: i64) -> &Kind {
        let Some((dst, start, start_at, end, end_at)) = &self.dst else {
            return &self.std;
        };
        let year =
            crate::calendar::civil_from_days((t + i64::from(self.std.offset)).div_euclid(86_400)).0;
        // The start is in standard time, the end in daylight saving time.
        let begins =
            day_of(*start, year) * 86_400 + i64::from(*start_at) - i64::from(self.std.offset);
        let ends = day_of(*end, year) * 86_400 + i64::from(*end_at) - i64::from(dst.offset);
        let in_dst = if begins < ends {
            (begins..ends).contains(&t)
        } else {
            // The southern hemisphere: daylight saving time spans the new
            // year.
            !(ends..begins).contains(&t)
        };
        if in_dst { dst } else { &self.std }
    }
}

/// Days from the epoch to `day` of `year`.
fn day_of(day: Day, year: i32) -> i64 {
    let jan1 = crate::calendar::days_from_civil(year, 1, 1);
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    match day {
        Day::Julian(n) => jan1 + i64::from(n) - 1 + i64::from(leap && n >= 60),
        Day::Ordinal(n) => jan1 + i64::from(n),
        Day::Month(m, w, d) => {
            let first = crate::calendar::days_from_civil(year, m, 1);
            // 1970-01-01 was a Thursday.
            let weekday = (first + 4).rem_euclid(7);
            let mut day = first + (i64::from(d) - weekday).rem_euclid(7) + 7 * (i64::from(w) - 1);
            let next = if m == 12 {
                crate::calendar::days_from_civil(year + 1, 1, 1)
            } else {
                crate::calendar::days_from_civil(year, m + 1, 1)
            };
            while day >= next {
                day -= 7;
            }
            day
        }
    }
}

/// A zone's name: three letters or more, or anything between `<` and `>`.
fn name(s: &mut &str) -> Option<String> {
    if let Some(rest) = s.strip_prefix('<') {
        let (name, rest) = rest.split_once('>')?;
        *s = rest;
        return (!name.is_empty()).then(|| name.to_owned());
    }
    let len = s.bytes().take_while(u8::is_ascii_alphabetic).count();
    if len < 3 {
        return None;
    }
    let (name, rest) = s.split_at(len);
    *s = rest;
    Some(name.to_owned())
}

/// `[+-]hh[:mm[:ss]]`, in seconds.
fn posix_time(s: &mut &str) -> Option<i32> {
    let sign = match s.as_bytes().first() {
        Some(b'-') => -1,
        Some(b'+') => 1,
        _ => 1,
    };
    if sign == -1 || s.starts_with('+') {
        *s = &s[1..];
    }
    let mut total = 0;
    for (i, scale) in [3600, 60, 1].into_iter().enumerate() {
        if i > 0 {
            match s.strip_prefix(':') {
                Some(rest) => *s = rest,
                None => break,
            }
        }
        let len = s.bytes().take_while(u8::is_ascii_digit).count();
        if len == 0 || len > 3 {
            return None;
        }
        total += s[..len].parse::<i32>().ok()? * scale;
        *s = &s[len..];
    }
    Some(sign * total)
}

/// A rule's day and its time of day (02:00 when it has none).
fn transition(s: &mut &str) -> Option<(Day, i32)> {
    let number = |s: &mut &str| {
        let len = s.bytes().take_while(u8::is_ascii_digit).count();
        let n = s.get(..len)?.parse::<u32>().ok()?;
        *s = &s[len..];
        Some(n)
    };
    let day = if let Some(rest) = s.strip_prefix('M') {
        *s = rest;
        let m = number(s)?;
        *s = s.strip_prefix('.')?;
        let w = number(s)?;
        *s = s.strip_prefix('.')?;
        let d = number(s)?;
        if !(1..=12).contains(&m) || !(1..=5).contains(&w) || d > 6 {
            return None;
        }
        Day::Month(m, w, d)
    } else if let Some(rest) = s.strip_prefix('J') {
        *s = rest;
        let n = number(s)?;
        if !(1..=365).contains(&n) {
            return None;
        }
        Day::Julian(n)
    } else {
        let n = number(s)?;
        if n > 365 {
            return None;
        }
        Day::Ordinal(n)
    };
    let at = match s.strip_prefix('/') {
        Some(rest) => {
            *s = rest;
            posix_time(s)?
        }
        None => 7200,
    };
    Some((day, at))
}

#[cfg(test)]
mod tests {
    use super::{LocalTime, TimeZone};

    // 1 January and 1 July 2026, at noon UTC.
    const WINTER: i64 = 1_767_268_800_000;
    const SUMMER: i64 = 1_782_907_200_000;

    fn local(y: i32, m: u32, d: u32, h: i64, min: i64) -> i64 {
        (crate::calendar::days_from_civil(y, m, d) * 86_400 + h * 3600 + min * 60) * 1000
    }

    #[test]
    fn a_rule_alone_keeps_daylight_saving_time_between_its_days() {
        let berlin = TimeZone::posix("CET-1CEST,M3.5.0,M10.5.0/3").unwrap();
        assert_eq!(berlin.offset_at(WINTER), 3600);
        assert_eq!(berlin.abbreviation_at(WINTER), "CET");
        assert_eq!(berlin.offset_at(SUMMER), 7200);
        assert_eq!(berlin.abbreviation_at(SUMMER), "CEST");
        assert!(berlin.is_daylight_time_at(SUMMER));
        // 29 March 2026, 01:00 UTC: the clocks go from 02:00 to 03:00.
        let change = local(2026, 3, 29, 1, 0);
        assert_eq!(berlin.offset_at(change - 1), 3600);
        assert_eq!(berlin.offset_at(change), 7200);
    }

    #[test]
    fn a_skipped_time_is_no_instant_and_a_repeated_one_two() {
        let berlin = TimeZone::posix("CET-1CEST,M3.5.0,M10.5.0/3").unwrap();
        assert_eq!(
            berlin.from_local(local(2026, 3, 29, 2, 30)),
            LocalTime::None
        );
        assert_eq!(
            berlin.from_local(local(2026, 10, 25, 2, 30)),
            LocalTime::Both(local(2026, 10, 25, 0, 30), local(2026, 10, 25, 1, 30))
        );
        assert_eq!(
            berlin.from_local(local(2026, 7, 1, 12, 0)),
            LocalTime::One(local(2026, 7, 1, 10, 0))
        );
        // As Qt reads them: by the offset before the change.
        assert_eq!(
            berlin.resolve_local(local(2026, 3, 29, 2, 30)),
            local(2026, 3, 29, 1, 30)
        );
        assert_eq!(
            berlin.resolve_local(local(2026, 10, 25, 2, 30)),
            local(2026, 10, 25, 0, 30)
        );
    }

    #[test]
    fn the_southern_hemisphere_keeps_it_over_the_new_year() {
        let sydney = TimeZone::posix("AEST-10AEDT,M10.1.0,M4.1.0/3").unwrap();
        assert_eq!(sydney.abbreviation_at(WINTER), "AEDT");
        assert_eq!(sydney.offset_at(SUMMER), 36_000);
        let lord_howe = TimeZone::posix("<+1030>-10:30<+11>-11,M10.1.0,M4.1.0").unwrap();
        assert_eq!(lord_howe.offset_at(WINTER), 39_600);
        assert_eq!(lord_howe.offset_at(SUMMER), 37_800);
    }

    #[test]
    fn a_zone_of_one_offset_has_no_daylight_saving_time() {
        let zone = TimeZone::posix("<+0545>-5:45").unwrap();
        assert_eq!(zone.offset_at(SUMMER), 20_700);
        assert_eq!(zone.abbreviation_at(SUMMER), "+0545");
        for bad in ["", "AB-1", "<>1", "CET-1CEST,M13.5.0,M10.5.0", "CET"] {
            assert!(TimeZone::posix(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn a_name_out_of_the_database_is_none() {
        for bad in [
            "",
            "/etc/passwd",
            "../etc/passwd",
            "Europe/../UTC",
            "Europe//Berlin",
            "a b",
        ] {
            assert!(TimeZone::named(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn the_database_has_each_zones_history() {
        // Not every system has a zone database; where it has, the zone is
        // read as the C library reads it.
        let Some(ny) = TimeZone::named("America/New_York") else {
            return;
        };
        assert_eq!(ny.id(), "America/New_York");
        assert_eq!(ny.offset_at(WINTER), -5 * 3600);
        assert_eq!(ny.abbreviation_at(SUMMER), "EDT");
        // Far past the transitions a file lists: the rule at its end.
        let later = local(2300, 7, 1, 12, 0);
        assert_eq!(ny.offset_at(later), -4 * 3600);
        // Before India had one time, Calcutta kept Madras time.
        if let Some(kolkata) = TimeZone::named("Asia/Kolkata") {
            assert_eq!(kolkata.offset_at(local(1900, 1, 1, 0, 0)), 19_270);
        }
    }
}
