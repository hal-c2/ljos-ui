//! What a calendar view needs of a locale: the first day of the week, the
//! names of the days and months, and the ISO week of a date.
//!
//! The names and the week data come from CLDR through ICU4X, so a name can
//! differ from Qt's where Qt's locale data is of another CLDR version. The
//! `C` locale has English names, Monday first and digits for narrow names,
//! as a running Qt 6.11 reports.

use icu_calendar::{Gregorian, types::Weekday, week::WeekInformation};
use icu_datetime::input::{Date, DateTime, Time};
use icu_datetime::pattern::{DateTimePattern, FixedCalendarDateTimeNames};
use writeable::TryWriteable;

use crate::locale::LocaleInfo;

/// How long a name is, as `Locale.LongFormat`, `ShortFormat` and
/// `NarrowFormat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameWidth {
    Long,
    Short,
    Narrow,
}

const ENGLISH_DAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
const ENGLISH_MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn weekday_number(w: Weekday) -> u8 {
    match w {
        Weekday::Monday => 1,
        Weekday::Tuesday => 2,
        Weekday::Wednesday => 3,
        Weekday::Thursday => 4,
        Weekday::Friday => 5,
        Weekday::Saturday => 6,
        Weekday::Sunday => 7,
    }
}

impl LocaleInfo {
    /// The first day of the week, as `Qt::DayOfWeek` numbers it: 1 is
    /// Monday, 7 is Sunday.
    #[must_use]
    pub fn first_day_of_week(&self) -> u8 {
        let Some(locale) = self.icu() else {
            return 1;
        };
        WeekInformation::try_new(locale.into()).map_or(1, |w| weekday_number(w.first_weekday))
    }

    /// The name of a day of the week (1 is Monday, 7 is Sunday). An empty
    /// string for a day that is not 1 to 7.
    #[must_use]
    pub fn day_name(&self, day: u8, width: NameWidth) -> String {
        if !(1..=7).contains(&day) {
            return String::new();
        }
        let Some(locale) = self.icu() else {
            let full = ENGLISH_DAYS[usize::from(day) - 1];
            return match width {
                NameWidth::Long => full.to_owned(),
                NameWidth::Short => full[..3].to_owned(),
                NameWidth::Narrow => day.to_string(),
            };
        };
        let pattern = match width {
            NameWidth::Long => "EEEE",
            NameWidth::Short => "EEE",
            NameWidth::Narrow => "EEEEE",
        };
        // 1 January 2024 was a Monday.
        format_date(locale, pattern, 2024, 1, i32::from(day)).unwrap_or_default()
    }

    /// The name of a month (0 is January, as in a JavaScript `Date`). An
    /// empty string for a month that is not 0 to 11.
    #[must_use]
    pub fn month_name(&self, month: u8, width: NameWidth) -> String {
        self.month_name_in(month, width, false)
    }

    /// The name of a month on its own, as a title has it: where a
    /// language declines the name inside a date ("марта") this is the
    /// plain one ("март").
    #[must_use]
    pub fn standalone_month_name(&self, month: u8, width: NameWidth) -> String {
        self.month_name_in(month, width, true)
    }

    fn month_name_in(&self, month: u8, width: NameWidth, standalone: bool) -> String {
        if month > 11 {
            return String::new();
        }
        let Some(locale) = self.icu() else {
            let full = ENGLISH_MONTHS[usize::from(month)];
            return match width {
                NameWidth::Long => full.to_owned(),
                NameWidth::Short => full[..3].to_owned(),
                NameWidth::Narrow => (month + 1).to_string(),
            };
        };
        let pattern = match (width, standalone) {
            (NameWidth::Long, false) => "MMMM",
            (NameWidth::Short, false) => "MMM",
            (NameWidth::Narrow, false) => "MMMMM",
            (NameWidth::Long, true) => "LLLL",
            (NameWidth::Short, true) => "LLL",
            (NameWidth::Narrow, true) => "LLLLL",
        };
        format_date(locale, pattern, 2024, i32::from(month) + 1, 1).unwrap_or_default()
    }
}

/// Writes a date with a pattern of one textual field.
fn format_date(
    locale: &icu_locale::Locale,
    pattern: &str,
    year: i32,
    month: i32,
    day: i32,
) -> Option<String> {
    let pattern: DateTimePattern = pattern.parse().ok()?;
    let mut names = FixedCalendarDateTimeNames::<Gregorian>::try_new(locale.into()).ok()?;
    let formatter = names.include_for_pattern(&pattern).ok()?;
    // A day past the end of the month carries into the next, so 1 January
    // plus a weekday number is every weekday once.
    let date =
        Date::try_new_gregorian(year, u8::try_from(month).ok()?, u8::try_from(day).ok()?).ok()?;
    let datetime = DateTime {
        date,
        time: Time::start_of_day(),
    };
    Some(
        formatter
            .format(&datetime)
            .try_write_to_string()
            .map_or_else(|(_, s)| s.into_owned(), |s| s.into_owned()),
    )
}

/// The ISO 8601 week number (1 to 53) of a Gregorian date.
#[must_use]
pub fn iso_week_number(year: i32, month: u32, day: u32) -> u32 {
    let days = days_from_civil(year, month, day);
    // 0 is Monday: 1970-01-01 was a Thursday.
    let weekday = (days + 3).rem_euclid(7);
    // The Thursday of this date's week decides the year the week is in.
    let thursday = days - weekday + 3;
    let (y, _, _) = civil_from_days(thursday);
    let jan1 = days_from_civil(y, 1, 1);
    ((thursday - jan1) / 7 + 1) as u32
}

/// Days since 1970-01-01 of a proleptic Gregorian date.
#[must_use]
pub fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let y = i64::from(year) - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The date `days` after 1970-01-01.
#[must_use]
pub fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y as i32, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn week_numbers_are_iso() {
        assert_eq!(iso_week_number(2024, 1, 29), 5);
        assert_eq!(iso_week_number(2024, 1, 28), 4);
        assert_eq!(iso_week_number(2021, 1, 3), 53);
        assert_eq!(iso_week_number(2024, 12, 30), 1);
    }

    #[test]
    fn civil_round_trip() {
        for d in [-800_000, -1, 0, 19_800, 2_000_000] {
            let (y, m, day) = civil_from_days(d);
            assert_eq!(days_from_civil(y, m, day), d);
        }
    }

    #[test]
    fn names_follow_the_locale() {
        let de = LocaleInfo::new("de_DE");
        assert_eq!(de.first_day_of_week(), 1);
        assert_eq!(de.day_name(1, NameWidth::Long), "Montag");
        assert_eq!(de.day_name(7, NameWidth::Narrow), "S");
        assert_eq!(LocaleInfo::new("en_US").first_day_of_week(), 7);
        let c = LocaleInfo::c();
        assert_eq!(c.day_name(3, NameWidth::Short), "Wed");
        assert_eq!(c.month_name(0, NameWidth::Narrow), "1");
    }
}
