//! The local time zone at an instant, and its names there: its
//! abbreviation (`PST`, `CEST`) and its long name (`Pacific Standard
//! Time`), daylight saving time included.
//!
//! Which zone it is, its offset and its abbreviation are the platform's:
//!
//! - Unix: the C library's `localtime_r`, which reads `TZ` and the zone
//!   database (tzfile(5)) as a script's `Date` does. The zone's IANA name
//!   is the one `TZ` gives, else the one `/etc/localtime` links to, else
//!   `/etc/timezone`'s.
//! - Windows: the zone the system is set to
//!   (`GetDynamicTimeZoneInformation`), by its Windows name. Windows has
//!   no abbreviations.
//! - Elsewhere nothing is known, and a zone is named by its offset.
//!
//! Long names, and the abbreviations a language other than English uses,
//! are CLDR's, through ICU4X.

use icu_datetime::NoCalendarFormatter;
use icu_datetime::fieldsets::zone::{SpecificLong, SpecificShort};
use icu_datetime::input::{TimeZone, TimeZoneInfo, UtcOffset};
use icu_locale::Locale;
use icu_time::zone::{ZoneNameTimestamp, models::AtTime};

/// What the platform says of the local zone at an instant.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalZone {
    /// Its IANA name (`Europe/Berlin`), or on Windows its Windows name
    /// (`W. Europe Standard Time`). `None` when the platform does not say.
    pub id: Option<String>,
    /// Seconds east of UTC.
    pub offset: i32,
    /// The zone database's abbreviation (`CET`; `+0545` where it has none
    /// but the offset). Windows has none.
    pub abbreviation: Option<String>,
}

impl LocalZone {
    /// The local zone at `unix_ms`, milliseconds after the UNIX epoch.
    #[must_use]
    pub fn at(unix_ms: i64) -> Self {
        platform::at(unix_ms)
    }
}

/// What a zone is called at an instant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneNames {
    /// `t`: `PST`, `MEZ`, or the offset (`GMT-8`, `UTC+01:00`) where the
    /// locale has no abbreviation.
    pub abbreviation: String,
    /// `tttt`: `Pacific Standard Time`.
    pub long_name: String,
}

/// The names of a zone at `unix_ms`, in `locale` (`None` is English, as
/// the `C` locale writes them). `platform_abbreviation` writes the zone
/// database's abbreviation where it has one, as Qt does for English and
/// the application's locale.
pub(crate) fn names(
    zone: Option<&LocalZone>,
    offset: i32,
    unix_ms: i64,
    locale: Option<&Locale>,
    platform_abbreviation: bool,
) -> ZoneNames {
    // A zone at another offset is not the local one: it is named by its
    // offset alone.
    let zone = zone.filter(|z| z.offset == offset);
    let info = zone.and_then(|z| cldr_zone(z.id.as_deref()?, offset, unix_ms));
    let en = icu_locale::locale!("en");
    let locale = locale.unwrap_or(&en);
    // A zone CLDR does not know has the zone database's name alone.
    let abbreviation = zone
        .filter(|_| platform_abbreviation || info.is_none())
        .and_then(|z| z.abbreviation.clone())
        .or_else(|| {
            let f = NoCalendarFormatter::try_new(locale.into(), SpecificShort).ok()?;
            Some(f.format(&info?).to_string())
        })
        .unwrap_or_else(|| offset_name(offset, false));
    let long_name = info
        .and_then(|info| {
            let f = NoCalendarFormatter::try_new(locale.into(), SpecificLong).ok()?;
            Some(f.format(&info).to_string())
        })
        .or_else(|| {
            let id = zone?.id.as_deref()?;
            is_posix_rule(id).then(|| id.to_owned())
        })
        .unwrap_or_else(|| offset_name(offset, true));
    ZoneNames {
        abbreviation,
        long_name,
    }
}

/// The zone CLDR knows by `id`, at an instant; `None` for one it does not
/// (a POSIX rule such as `<+0330>-3:30`).
fn cldr_zone(id: &str, offset: i32, unix_ms: i64) -> Option<TimeZoneInfo<AtTime>> {
    let zone = TimeZone::from_system_id(id, None);
    if zone.is_unknown() {
        return None;
    }
    let offset = UtcOffset::try_from_seconds(offset).ok()?;
    Some(zone.with_offset(Some(offset)).with_zone_name_timestamp(
        ZoneNameTimestamp::from_epoch_seconds(unix_ms.div_euclid(1000)),
    ))
}

/// Whether `TZ` is a POSIX rule (`ABC-2`, `<+0330>-3:30`,
/// `XYZ3ABC,M3.2.0,M11.1.0`): a name of three letters or more, or one
/// between angle brackets, and the offset from it. Qt names such a zone by
/// the rule.
fn is_posix_rule(tz: &str) -> bool {
    let rest = match tz.strip_prefix('<') {
        Some(quoted) => match quoted.split_once('>') {
            Some((name, rest)) if !name.is_empty() => rest,
            _ => return false,
        },
        None => {
            let letters = tz.bytes().take_while(u8::is_ascii_alphabetic).count();
            if letters < 3 {
                return false;
            }
            &tz[letters..]
        }
    };
    let rest = rest.strip_prefix(['+', '-']).unwrap_or(rest);
    !tz.contains('/') && rest.starts_with(|c: char| c.is_ascii_digit())
}

/// A zone without a name: `UTC`, `UTC-08:00`, and `Coordinated Universal
/// Time` for the long name of UTC.
pub(crate) fn offset_name(offset: i32, long: bool) -> String {
    let sign = if offset < 0 { '-' } else { '+' };
    let minutes = offset.unsigned_abs() / 60;
    let (h, m) = (minutes / 60, minutes % 60);
    match offset {
        0 if long => "Coordinated Universal Time".to_owned(),
        0 => "UTC".to_owned(),
        _ => format!("UTC{sign}{h:02}:{m:02}"),
    }
}

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_vendor = "apple",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly",
))]
mod platform {
    use std::ffi::CStr;
    use std::path::Path;

    use super::LocalZone;

    pub(super) fn at(unix_ms: i64) -> LocalZone {
        // A 32-bit time_t ends in 2038.
        let Some(time) = libc::time_t::try_from(unix_ms.div_euclid(1000)).ok() else {
            return LocalZone::default();
        };
        // SAFETY: `tm` is plain data, for which all zeros is a value.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        // SAFETY: both pointers are to locals that outlive the call.
        if unsafe { libc::localtime_r(&raw const time, &raw mut tm) }.is_null() {
            return LocalZone::default();
        }
        let abbreviation = (!tm.tm_zone.is_null())
            // SAFETY: the C library points `tm_zone` at a NUL-terminated
            // name it keeps until the zone is read again; it is copied now.
            .then(|| unsafe { CStr::from_ptr(tm.tm_zone) })
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty());
        LocalZone {
            id: iana_name(),
            offset: i32::try_from(tm.tm_gmtoff).unwrap_or(0),
            abbreviation,
        }
    }

    /// The IANA name of the zone the C library reads.
    fn iana_name() -> Option<String> {
        match std::env::var("TZ") {
            // An empty TZ is UTC.
            Ok(tz) if tz.is_empty() => Some("UTC".to_owned()),
            Ok(tz) => {
                let tz = tz.strip_prefix(':').unwrap_or(&tz);
                if tz.starts_with('/') {
                    in_zoneinfo(Path::new(tz))
                } else {
                    // A name (`Europe/Berlin`, `EST5EDT`) or a POSIX rule,
                    // which no zone has as its name.
                    Some(tz.to_owned())
                }
            }
            Err(_) => std::fs::read_link("/etc/localtime")
                .ok()
                .and_then(|link| in_zoneinfo(&link))
                .or_else(|| {
                    let name = std::fs::read_to_string("/etc/timezone").ok()?;
                    let name = name.trim();
                    (!name.is_empty()).then(|| name.to_owned())
                }),
        }
    }

    /// `Europe/Berlin` of `/usr/share/zoneinfo/Europe/Berlin` (and of the
    /// `posix/` and `right/` trees beside it).
    fn in_zoneinfo(path: &Path) -> Option<String> {
        let path = path.to_str()?;
        let name = &path[path.rfind("zoneinfo/")? + "zoneinfo/".len()..];
        let name = name
            .strip_prefix("posix/")
            .or_else(|| name.strip_prefix("right/"))
            .unwrap_or(name);
        (!name.is_empty()).then(|| name.to_owned())
    }

    #[cfg(test)]
    mod tests {
        use std::path::Path;

        #[test]
        fn a_zone_file_is_named_by_its_place_in_the_database() {
            for (path, name) in [
                ("/usr/share/zoneinfo/Europe/Berlin", "Europe/Berlin"),
                (
                    "../usr/share/zoneinfo/America/Los_Angeles",
                    "America/Los_Angeles",
                ),
                ("/var/db/timezone/zoneinfo/Asia/Tokyo", "Asia/Tokyo"),
                ("/usr/share/zoneinfo/posix/UTC", "UTC"),
            ] {
                assert_eq!(super::in_zoneinfo(Path::new(path)).as_deref(), Some(name));
            }
            assert_eq!(super::in_zoneinfo(Path::new("/etc/zone")), None);
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::Time::{
        DYNAMIC_TIME_ZONE_INFORMATION, GetDynamicTimeZoneInformation,
        SystemTimeToTzSpecificLocalTimeEx,
    };

    use super::LocalZone;

    const TIME_ZONE_ID_INVALID: u32 = u32::MAX;

    pub(super) fn at(unix_ms: i64) -> LocalZone {
        // SAFETY: the structure is plain data, for which all zeros is a
        // value, and the system fills it.
        let mut zone: DYNAMIC_TIME_ZONE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetDynamicTimeZoneInformation(&mut zone) } == TIME_ZONE_ID_INVALID {
            return LocalZone::default();
        }
        let utc = system_time(unix_ms);
        // SAFETY: as above.
        let mut local: SYSTEMTIME = unsafe { std::mem::zeroed() };
        // SAFETY: the pointers are to locals that outlive the call.
        let offset = if unsafe { SystemTimeToTzSpecificLocalTimeEx(&zone, &utc, &mut local) } != 0 {
            i32::try_from((unix_ms_of(&local) - unix_ms.div_euclid(60_000) * 60_000) / 1000)
                .unwrap_or(0)
        } else {
            -zone.Bias * 60
        };
        let key = &zone.TimeZoneKeyName;
        let len = key.iter().position(|&c| c == 0).unwrap_or(key.len());
        let id = String::from_utf16_lossy(&key[..len]);
        LocalZone {
            id: (!id.is_empty()).then_some(id),
            offset,
            abbreviation: None,
        }
    }

    /// The minute `unix_ms` is in, in UTC.
    fn system_time(unix_ms: i64) -> SYSTEMTIME {
        let days = unix_ms.div_euclid(86_400_000);
        let in_day = unix_ms.rem_euclid(86_400_000);
        let (year, month, day) = crate::calendar::civil_from_days(days);
        SYSTEMTIME {
            wYear: u16::try_from(year).unwrap_or(1970),
            wMonth: month as u16,
            wDayOfWeek: 0,
            wDay: day as u16,
            wHour: (in_day / 3_600_000) as u16,
            wMinute: (in_day / 60_000 % 60) as u16,
            wSecond: 0,
            wMilliseconds: 0,
        }
    }

    fn unix_ms_of(t: &SYSTEMTIME) -> i64 {
        let days = crate::calendar::days_from_civil(
            i32::from(t.wYear),
            u32::from(t.wMonth),
            u32::from(t.wDay),
        );
        days * 86_400_000 + i64::from(t.wHour) * 3_600_000 + i64::from(t.wMinute) * 60_000
    }
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_vendor = "apple",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly",
    target_os = "windows",
)))]
mod platform {
    use super::LocalZone;

    pub(super) fn at(_unix_ms: i64) -> LocalZone {
        LocalZone::default()
    }
}

#[cfg(test)]
mod tests {
    use super::{LocalZone, names};

    fn zone(id: &str, offset: i32, abbreviation: &str) -> LocalZone {
        LocalZone {
            id: Some(id.to_owned()),
            offset,
            abbreviation: Some(abbreviation.to_owned()),
        }
    }

    // 1 January and 1 July 2026, at noon UTC.
    const WINTER: i64 = 1_767_268_800_000;
    const SUMMER: i64 = 1_782_907_200_000;

    #[test]
    fn a_zone_is_named_by_its_variant_at_the_instant() {
        let la = zone("America/Los_Angeles", -8 * 3600, "PST");
        let n = names(Some(&la), -8 * 3600, WINTER, None, true);
        assert_eq!(
            (n.abbreviation.as_str(), n.long_name.as_str()),
            ("PST", "Pacific Standard Time")
        );
        let la = zone("America/Los_Angeles", -7 * 3600, "PDT");
        let n = names(Some(&la), -7 * 3600, SUMMER, None, true);
        assert_eq!(
            (n.abbreviation.as_str(), n.long_name.as_str()),
            ("PDT", "Pacific Daylight Time")
        );
    }

    #[test]
    fn another_language_has_its_own_abbreviation_or_the_offset() {
        let de: icu_locale::Locale = "de-DE".parse().unwrap();
        let berlin = zone("Europe/Berlin", 7200, "CEST");
        let n = names(Some(&berlin), 7200, SUMMER, Some(&de), false);
        assert_eq!(n.abbreviation, "MESZ");
        assert_eq!(n.long_name, "Mitteleuropäische Sommerzeit");
        let la = zone("America/Los_Angeles", -8 * 3600, "PST");
        assert_eq!(
            names(Some(&la), -8 * 3600, WINTER, Some(&de), false).abbreviation,
            "GMT-8"
        );
    }

    #[test]
    fn a_zone_without_a_name_is_its_offset_or_its_rule() {
        let rule = zone("<+0330>-3:30", 12_600, "+0330");
        let n = names(Some(&rule), 12_600, WINTER, None, true);
        assert_eq!(
            (n.abbreviation.as_str(), n.long_name.as_str()),
            ("+0330", "<+0330>-3:30")
        );
        let n = names(None, -8 * 3600, WINTER, None, true);
        assert_eq!(
            (n.abbreviation.as_str(), n.long_name.as_str()),
            ("UTC-08:00", "UTC-08:00")
        );
        let n = names(None, 0, WINTER, None, true);
        assert_eq!(
            (n.abbreviation.as_str(), n.long_name.as_str()),
            ("UTC", "Coordinated Universal Time")
        );
    }

    #[test]
    fn a_posix_rule_is_named_by_the_c_library_and_by_itself() {
        let de = icu_locale::locale!("de");
        let rule = zone("XYZ3ABC,M3.2.0,M11.1.0", -3 * 3600, "XYZ");
        let n = names(Some(&rule), -3 * 3600, WINTER, Some(&de), false);
        assert_eq!(
            (n.abbreviation.as_str(), n.long_name.as_str()),
            ("XYZ", "XYZ3ABC,M3.2.0,M11.1.0")
        );
        for rule in [
            "ABC-2",
            "<+0330>-3:30",
            "<-05>5",
            "ABC0",
            "XYZ3ABC,M3.2.0,M11.1.0",
        ] {
            assert!(super::is_posix_rule(rule), "{rule}");
        }
        for name in ["Foo/Bar", "UT1", "<>3", "ABC", "Europe/Berlin", ""] {
            assert!(!super::is_posix_rule(name), "{name}");
        }
    }

    #[test]
    fn a_zone_at_another_offset_is_not_the_local_one() {
        let berlin = zone("Europe/Berlin", 3600, "CET");
        let n = names(Some(&berlin), -8 * 3600, WINTER, None, true);
        assert_eq!(n.abbreviation, "UTC-08:00");
    }
}
