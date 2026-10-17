//! The local time zone's names, from Rust. This file has one test, so the
//! zone it sets is the only one the process reads.

use ljos::locale::{DateTime, FormatType, LocalZone, Locale};

// 14:05 in Los Angeles on 1 January and 1 July 2026.
const WINTER: i64 = 1_767_305_100_000;
const SUMMER: i64 = 1_782_939_900_000;

#[test]
fn rust_names_the_local_zone_at_an_instant() {
    // SAFETY: the one test of this process sets it before anything reads
    // the environment or the local zone.
    unsafe { std::env::set_var("TZ", "America/Los_Angeles") };
    let en = Locale::named("en_US");
    let winter = en.zone_names(WINTER);
    assert_eq!(winter.abbreviation, "PST");
    assert_eq!(winter.long_name, "Pacific Standard Time");
    let summer = en.zone_names(SUMMER);
    assert_eq!(summer.abbreviation, "PDT");
    assert_eq!(summer.long_name, "Pacific Daylight Time");

    let de = Locale::named("de_DE").zone_names(WINTER);
    assert_eq!(de.abbreviation, "GMT-8");
    assert_eq!(de.long_name, "Nordamerikanische Westküsten-Normalzeit");

    let zone = LocalZone::at(SUMMER);
    assert_eq!(zone.id.as_deref(), Some("America/Los_Angeles"));
    assert_eq!(zone.offset, -7 * 3600);
    let date = DateTime::from_unix_ms(SUMMER, zone.offset);
    assert_eq!(en.format_pattern(&date, "h:mm AP t"), "2:05 PM PDT");
    assert_eq!(
        Locale::named("C").format_time(&date, FormatType::Long),
        "14:05:00 PDT"
    );
}
