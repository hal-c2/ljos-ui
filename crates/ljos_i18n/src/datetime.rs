//! Dates and times as a locale writes them, in Qt's format strings
//! (<https://doc.qt.io/qt-6/qml-qtqml-locale.html>,
//! <https://doc.qt.io/qt-6/qml-qtqml-date.html>): the formats a locale has
//! (`dateFormat(Locale.LongFormat)`), writing a date with a format string,
//! and reading one back.
//!
//! A locale's formats come from CLDR through ICU4X, written in Qt's letters:
//! CLDR's full date pattern is Qt's long format and its short one Qt's short
//! one, as a running Qt 6.11 reports them. The `C` locale has Qt's own.

use icu_calendar::{Gregorian, week::WeekInformation};
use icu_datetime::FixedCalendarDateTimeFormatter;
use icu_datetime::fieldsets::{T, YMD, YMDE};
use icu_datetime::input::{Date, DateTime, Time};

use crate::calendar::NameWidth;
use crate::locale::LocaleInfo;

/// Which half of a date and time a format string is read for: a date
/// writes only the letters of a date, a time only those of a time, and the
/// other letters are text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Date,
    Time,
    DateTime,
}

impl Part {
    fn date(self) -> bool {
        self != Self::Time
    }

    fn time(self) -> bool {
        self != Self::Date
    }
}

/// A date and time in local time, as a script's `Date` has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Fields {
    pub year: i32,
    /// 1 is January.
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub millisecond: u32,
    /// Seconds east of UTC.
    pub offset: i32,
}

impl Fields {
    /// The day of the week, as `Qt::DayOfWeek` numbers it: 1 is Monday.
    #[must_use]
    pub fn weekday(&self) -> u8 {
        let days = crate::calendar::days_from_civil(self.year, self.month, self.day);
        // 1970-01-01 was a Thursday.
        (days + 3).rem_euclid(7) as u8 + 1
    }
}

/// What reading a text gave: the fields it named, the rest left at Qt's
/// defaults (1 January 1900, midnight). `offset` is set only when the text
/// named a time zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parsed {
    pub fields: Fields,
    pub offset: Option<i32>,
}

impl LocaleInfo {
    /// `Locale.dateFormat`: the long format for `long`, else the short one.
    #[must_use]
    pub fn date_format(&self, long: bool) -> String {
        let Some(locale) = self.icu() else {
            return c_date_format(long);
        };
        let date = Date::try_new_gregorian(2024, 1, 5).ok();
        let pattern = date.and_then(|date| {
            if long {
                FixedCalendarDateTimeFormatter::<Gregorian, _>::try_new(locale.into(), YMDE::long())
                    .ok()
                    .map(|f| f.format(&date).pattern().to_string())
            } else {
                FixedCalendarDateTimeFormatter::<Gregorian, _>::try_new(locale.into(), YMD::short())
                    .ok()
                    .map(|f| f.format(&date).pattern().to_string())
            }
        });
        pattern.map_or_else(|| c_date_format(long), |p| from_cldr(&p))
    }

    /// `Locale.timeFormat`: with seconds and the zone's name for `long`,
    /// hours and minutes else.
    #[must_use]
    pub fn time_format(&self, long: bool) -> String {
        let Some(locale) = self.icu() else {
            return if long { "HH:mm:ss t" } else { "HH:mm" }.to_owned();
        };
        let fields = if long { T::hms() } else { T::hm() };
        let time = Time::try_new(14, 7, 9, 0).ok();
        let pattern = time.and_then(|time| {
            FixedCalendarDateTimeFormatter::<Gregorian, _>::try_new(locale.into(), fields)
                .ok()
                .map(|f| f.format(&time).pattern().to_string())
        });
        let Some(pattern) = pattern else {
            return if long { "HH:mm:ss t" } else { "HH:mm" }.to_owned();
        };
        let time = from_cldr(&pattern);
        if long { format!("{time} tttt") } else { time }
    }

    /// `Locale.dateTimeFormat`: the date's format, a space, the time's.
    #[must_use]
    pub fn date_time_format(&self, long: bool) -> String {
        format!("{} {}", self.date_format(long), self.time_format(long))
    }

    /// `Locale.amText` and `Locale.pmText`.
    #[must_use]
    pub fn am_pm(&self) -> (String, String) {
        let english = || ("AM".to_owned(), "PM".to_owned());
        let Some(locale) = self.icu() else {
            return english();
        };
        let Ok(pattern) = "a".parse::<icu_datetime::pattern::DateTimePattern>() else {
            return english();
        };
        let Ok(mut names) =
            icu_datetime::pattern::FixedCalendarDateTimeNames::<Gregorian>::try_new(locale.into())
        else {
            return english();
        };
        let Ok(formatter) = names.include_for_pattern(&pattern) else {
            return english();
        };
        let at = |hour| {
            let date = Date::try_new_gregorian(2024, 1, 1).ok()?;
            let time = Time::try_new(hour, 0, 0, 0).ok()?;
            use writeable::TryWriteable as _;
            Some(
                formatter
                    .format(&DateTime { date, time })
                    .try_write_to_string()
                    .map_or_else(|(_, s)| s.into_owned(), |s| s.into_owned()),
            )
        };
        match (at(9), at(15)) {
            (Some(am), Some(pm)) => (am, pm),
            _ => english(),
        }
    }

    /// `Locale.measurementSystem`: 0 metric, 1 US, 2 UK.
    #[must_use]
    pub fn measurement_system(&self) -> u8 {
        let region = self
            .icu()
            .and_then(crate::locale::maximized)
            .and_then(|id| id.region);
        match region.as_ref().map(|r| r.as_str()) {
            Some("US" | "LR" | "MM") => 1,
            Some("GB") => 2,
            _ => 0,
        }
    }

    /// `Locale.weekDays`: the days that are not the weekend, Monday first,
    /// as `Locale.DayOfWeek` numbers them (Sunday is 0).
    #[must_use]
    pub fn week_days(&self) -> Vec<u8> {
        let weekend: Vec<u8> = self
            .icu()
            .and_then(|l| WeekInformation::try_new(l.into()).ok())
            .map_or_else(
                || vec![6, 7],
                |w| w.weekend().map(crate::calendar::weekday_number).collect(),
            );
        (1..=7u8)
            .filter(|d| !weekend.contains(d))
            .map(|d| d % 7)
            .collect()
    }

    /// Writes `fields` with a Qt format string.
    #[must_use]
    pub fn format_date_time(&self, pattern: &str, fields: &Fields, part: Part) -> String {
        let tokens = tokenize(pattern, part);
        let twelve = tokens.iter().any(|t| matches!(t, Token::AmPm(_)));
        let mut out = String::new();
        for token in tokens {
            match token {
                Token::Text(t) => out.push_str(&t),
                Token::Field(letter, n) => out.push_str(&self.field(letter, n, fields, twelve)),
                Token::AmPm(case) => {
                    let (am, pm) = self.am_pm();
                    let text = if fields.hour < 12 { am } else { pm };
                    out.push_str(&match case {
                        Case::Upper => text.to_uppercase(),
                        Case::Lower => text.to_lowercase(),
                        Case::Locale => text,
                    });
                }
            }
        }
        out
    }

    fn digits(&self, ascii: String) -> String {
        let zero = self.zero_digit();
        let mut z = zero.chars();
        match (z.next(), z.next()) {
            (Some(zero), None) if zero != '0' => ascii
                .chars()
                .map(|c| match c.to_digit(10) {
                    Some(d) => char::from_u32(zero as u32 + d).unwrap_or(c),
                    None => c,
                })
                .collect(),
            _ => ascii,
        }
    }

    fn field(&self, letter: char, n: usize, f: &Fields, twelve: bool) -> String {
        let num = |v: i64, width: usize| self.digits(format!("{v:0width$}"));
        let hour12 = || match f.hour % 12 {
            0 => 12,
            h => h,
        };
        match (letter, n) {
            ('y', 2) => num(i64::from(f.year).rem_euclid(100), 2),
            ('y', _) => num(i64::from(f.year), 4),
            ('M', 1 | 2) => num(i64::from(f.month), n),
            ('M', 3) => self.month_name(month_index(f.month), NameWidth::Short),
            ('M', _) => self.month_name(month_index(f.month), NameWidth::Long),
            ('d', 1 | 2) => num(i64::from(f.day), n),
            ('d', 3) => self.day_name(f.weekday(), NameWidth::Short),
            ('d', _) => self.day_name(f.weekday(), NameWidth::Long),
            ('h', _) if twelve => num(i64::from(hour12()), n),
            ('h' | 'H', _) => num(i64::from(f.hour), n),
            ('m', _) => num(i64::from(f.minute), n),
            ('s', _) => num(i64::from(f.second), n),
            ('z', 3) => num(i64::from(f.millisecond), 3),
            ('z', _) => {
                // The fraction of the second without its trailing zeros.
                let ms = format!("{:03}", f.millisecond);
                let trimmed = ms.trim_end_matches('0');
                self.digits(if trimmed.is_empty() { "0" } else { trimmed }.to_owned())
            }
            ('t', _) => zone(f.offset, n),
            _ => String::new(),
        }
    }

    /// Reads `text` written with the Qt format string `pattern`. `None`
    /// when the text does not fit the pattern, or names a date or time that
    /// does not exist.
    #[must_use]
    pub fn parse_date_time(&self, pattern: &str, text: &str, part: Part) -> Option<Parsed> {
        let tokens = tokenize(pattern, part);
        let twelve = tokens.iter().any(|t| matches!(t, Token::AmPm(_)));
        let mut f = Fields {
            year: 1900,
            month: 1,
            day: 1,
            ..Fields::default()
        };
        let mut weekday = None;
        let mut pm = None;
        let mut offset = None;
        let mut rest = text;
        for token in &tokens {
            match token {
                Token::Text(t) => {
                    for c in t.chars() {
                        let mut chars = rest.chars();
                        let got = chars.next()?;
                        let same = got == c
                            || (got.is_whitespace() && c.is_whitespace())
                            || got.to_lowercase().eq(c.to_lowercase());
                        if !same {
                            return None;
                        }
                        rest = chars.as_str();
                    }
                }
                Token::AmPm(_) => {
                    let (am, pmt) = self.am_pm();
                    // The English markers too: Qt reads "PM" in any locale.
                    let read = |name: &str, english| {
                        strip_name(rest, name).or_else(|| strip_name(rest, english))
                    };
                    let (is_pm, r) = match read(&am, "AM") {
                        Some(r) => (false, r),
                        None => (true, read(&pmt, "PM")?),
                    };
                    pm = Some(is_pm);
                    rest = r;
                }
                Token::Field(letter, n) => {
                    let n = *n;
                    match (letter, n) {
                        ('M', 3 | 4) => {
                            let (m, r) = self.read_name(
                                rest,
                                n,
                                |i, w| self.month_name(u8::try_from(i).ok()?, w).into(),
                                12,
                            )?;
                            f.month = m + 1;
                            rest = r;
                        }
                        ('d', 3 | 4) => {
                            let (d, r) = self.read_name(
                                rest,
                                n,
                                |i, w| self.day_name(u8::try_from(i + 1).ok()?, w).into(),
                                7,
                            )?;
                            weekday = Some(d + 1);
                            rest = r;
                        }
                        ('t', _) => {
                            let (o, r) = read_zone(rest)?;
                            offset = Some(o);
                            rest = r;
                        }
                        _ => {
                            let (min, max) = match (letter, n) {
                                ('y', 2) => (2, 2),
                                ('y', _) => (4, 4),
                                ('z', 3) => (3, 3),
                                ('z', _) => (1, 3),
                                (_, 1) => (1, 2),
                                _ => (2, 2),
                            };
                            let (v, digits, r) = self.read_number(rest, min, max)?;
                            rest = r;
                            match letter {
                                'y' if n == 2 => f.year = 1900 + v as i32,
                                'y' => f.year = v as i32,
                                'M' => f.month = v,
                                'd' => f.day = v,
                                'h' | 'H' => f.hour = v,
                                'm' => f.minute = v,
                                's' => f.second = v,
                                'z' if n == 3 => f.millisecond = v,
                                // A fraction: "5" is half a second.
                                'z' => f.millisecond = v * 10u32.pow(3 - digits),
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
        if !rest.is_empty() {
            return None;
        }
        if let Some(pm) = pm {
            if !(1..=12).contains(&f.hour) || !twelve {
                return None;
            }
            f.hour = f.hour % 12 + if pm { 12 } else { 0 };
        }
        let days_in_month = match f.month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if f.year % 4 == 0 && (f.year % 100 != 0 || f.year % 400 == 0) => 29,
            2 => 28,
            _ => return None,
        };
        if f.day == 0 || f.day > days_in_month || f.hour > 23 || f.minute > 59 || f.second > 59 {
            return None;
        }
        if weekday.is_some_and(|w| u8::try_from(w).ok() != Some(f.weekday())) {
            return None;
        }
        Some(Parsed { fields: f, offset })
    }

    /// A name of one of `count` things (months, days) at the start of
    /// `text`, in either width, the longest that fits.
    fn read_name<'t>(
        &self,
        text: &'t str,
        letters: usize,
        name: impl Fn(u32, NameWidth) -> Option<String>,
        count: u32,
    ) -> Option<(u32, &'t str)> {
        let widths = if letters == 4 {
            [NameWidth::Long, NameWidth::Short]
        } else {
            [NameWidth::Short, NameWidth::Long]
        };
        widths.iter().find_map(|&w| {
            (0..count)
                .filter_map(|i| {
                    let n = name(i, w)?;
                    let rest = strip_name(text, &n)?;
                    Some((i, rest, n.chars().count()))
                })
                .max_by_key(|(_, _, len)| *len)
                .map(|(i, rest, _)| (i, rest))
        })
    }

    /// `min` to `max` digits, in ASCII or the locale's own.
    fn read_number<'t>(
        &self,
        text: &'t str,
        min: usize,
        max: usize,
    ) -> Option<(u32, u32, &'t str)> {
        let zero = self.zero_digit().chars().next().unwrap_or('0');
        let mut value = 0u32;
        let mut count = 0;
        let mut end = 0;
        for (i, c) in text.char_indices() {
            if count == max {
                break;
            }
            let d = c
                .to_digit(10)
                .or_else(|| (c as u32).checked_sub(zero as u32).filter(|d| *d < 10));
            let Some(d) = d else { break };
            value = value * 10 + d;
            count += 1;
            end = i + c.len_utf8();
        }
        (count >= min).then(|| (value, count as u32, &text[end..]))
    }
}

fn month_index(month: u32) -> u8 {
    u8::try_from(month.saturating_sub(1)).unwrap_or(u8::MAX)
}

fn c_date_format(long: bool) -> String {
    if long {
        "dddd, d MMMM yyyy"
    } else {
        "d MM yyyy"
    }
    .to_owned()
}

/// The name of the zone `offset` seconds east of UTC: `t` its
/// abbreviation, `tt` `+0000`, `ttt` `+00:00`, `tttt` its long name.
fn zone(offset: i32, letters: usize) -> String {
    let sign = if offset < 0 { '-' } else { '+' };
    let minutes = offset.unsigned_abs() / 60;
    let (h, m) = (minutes / 60, minutes % 60);
    match letters {
        2 => format!("{sign}{h:02}{m:02}"),
        3 => format!("{sign}{h:02}:{m:02}"),
        _ if offset == 0 && letters >= 4 => "Coordinated Universal Time".to_owned(),
        _ if offset == 0 => "UTC".to_owned(),
        _ => format!("UTC{sign}{h:02}:{m:02}"),
    }
}

/// A zone at the start of `text`, as [`zone`] writes one.
fn read_zone(text: &str) -> Option<(i32, &str)> {
    for name in ["Coordinated Universal Time", "UTC", "GMT", "Z"] {
        if let Some(rest) = strip_name(text, name) {
            if let Some((o, r)) = read_offset(rest) {
                return Some((o, r));
            }
            return Some((0, rest));
        }
    }
    read_offset(text)
}

fn read_offset(text: &str) -> Option<(i32, &str)> {
    let sign = match text.chars().next()? {
        '+' => 1,
        '-' => -1,
        _ => return None,
    };
    let b = &text.as_bytes()[1..];
    let digit = |i: usize| {
        b.get(i)
            .filter(|c| c.is_ascii_digit())
            .map(|c| i32::from(c - b'0'))
    };
    let h = digit(0)? * 10 + digit(1)?;
    let (m, used) = if b.get(2) == Some(&b':') {
        (digit(3)? * 10 + digit(4)?, 5)
    } else {
        (digit(2)? * 10 + digit(3)?, 4)
    };
    Some((sign * (h * 3600 + m * 60), &text[1 + used..]))
}

/// `text` after `name`, compared without case.
fn strip_name<'t>(text: &'t str, name: &str) -> Option<&'t str> {
    if name.is_empty() {
        return None;
    }
    let mut rest = text.chars();
    for n in name.chars() {
        let t = rest.next()?;
        if !(t == n
            || t.to_lowercase().eq(n.to_lowercase())
            || (t.is_whitespace() && n.is_whitespace()))
        {
            return None;
        }
    }
    Some(rest.as_str())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Case {
    Upper,
    Lower,
    /// As the locale writes it.
    Locale,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Text(String),
    /// A letter and how many times it is repeated.
    Field(char, usize),
    AmPm(Case),
}

/// The tokens of a Qt format string. A run of a letter longer than its
/// longest field is that field and then the rest of the run (`yyy` is `yy`
/// and a `y`); a run shorter than its shortest is text.
fn tokenize(pattern: &str, part: Part) -> Vec<Token> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out: Vec<Token> = Vec::new();
    let text = |out: &mut Vec<Token>, s: &str| match out.last_mut() {
        Some(Token::Text(t)) => t.push_str(s),
        _ => out.push(Token::Text(s.to_owned())),
    };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            // `''` is a quote; anything else up to the next quote is text.
            if chars.get(i + 1) == Some(&'\'') {
                text(&mut out, "'");
                i += 2;
                continue;
            }
            let mut j = i + 1;
            let mut lit = String::new();
            while j < chars.len() {
                if chars[j] == '\'' {
                    if chars.get(j + 1) == Some(&'\'') {
                        lit.push('\'');
                        j += 2;
                        continue;
                    }
                    break;
                }
                lit.push(chars[j]);
                j += 1;
            }
            text(&mut out, &lit);
            i = j + 1;
            continue;
        }
        if part.time() && (c == 'a' || c == 'A') {
            let next = chars.get(i + 1).copied();
            let (case, len) = match (c, next) {
                ('A', Some('P')) => (Case::Upper, 2),
                ('a', Some('p')) => (Case::Lower, 2),
                ('A', Some('p')) | ('a', Some('P')) => (Case::Locale, 2),
                ('A', _) => (Case::Upper, 1),
                _ => (Case::Lower, 1),
            };
            out.push(Token::AmPm(case));
            i += len;
            continue;
        }
        let max = match c {
            'd' | 'M' | 'y' if part.date() => 4,
            'h' | 'H' | 'm' | 's' if part.time() => 2,
            'z' if part.time() => 3,
            't' if part.time() => 4,
            _ => 0,
        };
        if max == 0 {
            text(&mut out, &c.to_string());
            i += 1;
            continue;
        }
        let mut run = 1;
        while chars.get(i + run) == Some(&c) {
            run += 1;
        }
        let mut left = run;
        while left > 0 {
            let take = left.min(max);
            // A year is two or four letters; a lone `y` is text.
            let take = if c == 'y' {
                match take {
                    4 => 4,
                    2 | 3 => 2,
                    _ => 0,
                }
            } else {
                take
            };
            if take == 0 {
                text(&mut out, &c.to_string());
                left -= 1;
            } else {
                out.push(Token::Field(c, take));
                left -= take;
            }
        }
        i += run;
    }
    out
}

/// A CLDR date pattern (`EEEE, MMMM d, y`) in Qt's letters
/// (`dddd, MMMM d, yyyy`).
#[must_use]
pub fn from_cldr(pattern: &str) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            // Quoted text means the same in both.
            let mut j = i + 1;
            while j < chars.len() {
                if chars[j] == '\'' {
                    if chars.get(j + 1) == Some(&'\'') {
                        j += 2;
                        continue;
                    }
                    break;
                }
                j += 1;
            }
            out.extend(&chars[i..(j + 1).min(chars.len())]);
            i = j + 1;
            continue;
        }
        if !c.is_ascii_alphabetic() {
            out.push(c);
            i += 1;
            continue;
        }
        let mut n = 1;
        while chars.get(i + n) == Some(&c) {
            n += 1;
        }
        i += n;
        let qt = match c {
            'y' | 'u' | 'Y' if n == 2 => "yy",
            'y' | 'u' | 'Y' => "yyyy",
            'M' | 'L' => match n {
                1 => "M",
                2 => "MM",
                3 | 5 => "MMM",
                _ => "MMMM",
            },
            'd' if n == 1 => "d",
            'd' => "dd",
            'E' | 'c' | 'e' if n == 4 => "dddd",
            'E' | 'c' | 'e' => "ddd",
            'a' | 'b' | 'B' => "Ap",
            'h' | 'K' if n == 1 => "h",
            'h' | 'K' => "hh",
            'H' | 'k' if n == 1 => "H",
            'H' | 'k' => "HH",
            'm' if n == 1 => "m",
            'm' => "mm",
            's' if n == 1 => "s",
            's' => "ss",
            'S' => "zzz",
            'z' | 'v' | 'V' | 'O' | 'Z' | 'x' | 'X' if n >= 4 => "tttt",
            'z' | 'v' | 'V' | 'O' | 'Z' | 'x' | 'X' => "t",
            // An era or a quarter: Qt's formats have none.
            _ => "",
        };
        out.push_str(qt);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day() -> Fields {
        Fields {
            year: 2024,
            month: 1,
            day: 5,
            hour: 14,
            minute: 7,
            second: 9,
            millisecond: 0,
            offset: 0,
        }
    }

    #[test]
    fn cldr_patterns_in_qt_letters() {
        assert_eq!(from_cldr("EEEE, MMMM d, y"), "dddd, MMMM d, yyyy");
        assert_eq!(from_cldr("M/d/yy"), "M/d/yy");
        assert_eq!(from_cldr("h:mm\u{202f}a"), "h:mm\u{202f}Ap");
        assert_eq!(from_cldr("EEEE, d MMMM y 'г'."), "dddd, d MMMM yyyy 'г'.");
    }

    #[test]
    fn formats_of_locales() {
        let en = LocaleInfo::new("en_US");
        assert_eq!(en.date_format(true), "dddd, MMMM d, yyyy");
        assert_eq!(en.date_format(false), "M/d/yy");
        assert_eq!(en.time_format(false), "h:mm\u{202f}Ap");
        assert_eq!(en.time_format(true), "h:mm:ss\u{202f}Ap tttt");
        let de = LocaleInfo::new("de_DE");
        assert_eq!(de.date_format(true), "dddd, d. MMMM yyyy");
        assert_eq!(de.date_format(false), "dd.MM.yy");
        assert_eq!(de.time_format(false), "HH:mm");
        assert_eq!(LocaleInfo::c().date_format(false), "d MM yyyy");
    }

    #[test]
    fn writes_with_a_format_string() {
        let en = LocaleInfo::new("en_US");
        let d = day();
        let w = |p, part| en.format_date_time(p, &d, part);
        assert_eq!(w("dd MMMM yyyy", Part::DateTime), "05 January 2024");
        assert_eq!(w("h:mm AP", Part::DateTime), "2:07 PM");
        assert_eq!(w("y yy yyy yyyy", Part::Date), "y 24 24y 2024");
        assert_eq!(
            w("'Day' d 'of' MMMM, ''yy''", Part::Date),
            "Day 5 of January, '24'"
        );
        assert_eq!(
            w("dd MMMM yyyy h:mm t", Part::Time),
            "dd MMMM yyyy 14:07 UTC"
        );
        assert_eq!(
            w("dd MMM h:mm:ss ap zzz t", Part::Date),
            "05 Jan h:mm:ss ap zzz t"
        );
        assert_eq!(w("t tt ttt", Part::Time), "UTC +0000 +00:00");
        assert_eq!(
            LocaleInfo::new("de_DE").format_date_time("dddd ddd MMMM MMM", &d, Part::Date),
            "Freitag Fr. Januar Jan."
        );
    }

    #[test]
    fn reads_what_it_writes() {
        let en = LocaleInfo::new("en_US");
        let r = |t, p, part| en.parse_date_time(p, t, part).map(|p| p.fields);
        let got = r("Friday, January 5, 2024", "dddd, MMMM d, yyyy", Part::Date).unwrap();
        assert_eq!((got.year, got.month, got.day), (2024, 1, 5));
        assert_eq!(r("1/5/24", "M/d/yy", Part::Date).unwrap().year, 1924);
        assert_eq!(r("2:07 pm", "h:mm AP", Part::Time).unwrap().hour, 14);
        assert_eq!(r("12:07 AM", "h:mm AP", Part::Time).unwrap().hour, 0);
        assert!(r("13:07 PM", "h:mm AP", Part::Time).is_none());
        assert!(r("Monday, January 5, 2024", "dddd, MMMM d, yyyy", Part::Date).is_none());
        assert!(r("31/2/2024", "d/M/yyyy", Part::Date).is_none());
        assert!(r("1/5/2024", "M/d/yy", Part::Date).is_none());
        assert!(r("1/5/2024", "MM/dd/yyyy", Part::Date).is_none());
        assert!(r("05 January 2024 x", "dd MMMM yyyy", Part::Date).is_none());
        assert_eq!(
            r("14:07:09.250", "HH:mm:ss.zzz", Part::Time)
                .unwrap()
                .millisecond,
            250
        );
    }

    #[test]
    fn what_a_locale_knows_of_days_and_measures() {
        assert_eq!(LocaleInfo::new("en_US").week_days(), [1, 2, 3, 4, 5]);
        assert_eq!(LocaleInfo::new("ar_EG").week_days(), [1, 2, 3, 4, 0]);
        assert_eq!(LocaleInfo::new("en_GB").measurement_system(), 2);
        assert_eq!(LocaleInfo::new("en_US").measurement_system(), 1);
        assert_eq!(LocaleInfo::new("is_IS").am_pm().0, "f.h.");
    }
}
