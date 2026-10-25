# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-date.html (fromLocaleString; no time zone argument, no Intl)
#   https://doc.qt.io/qt-6/qdatetime.html#fromString (the `t` letters it reads)
#   tzfile(5), RFC 9636
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).
# Each run is a program of its own started in the zone the scenario names (`TZ`), with
# LANG=en_US.UTF-8.

@qt-parity @headless
Feature: A date is read in a time zone named by its IANA name
  A script has no `Intl` and no time zone option: the way to a zone other than the local one is
  to read a date written in it. In the format `Date.fromLocaleString` reads, `t` and `tttt` read
  a zone by its name in the system's zone database (`America/New_York`), as well as an offset
  (`+05:30`, `UTC+05:30`, `Z`) and the local zone's abbreviation. The date is the instant the
  zone's clocks showed it, by the zone's history: a time the clocks skipped is read at the offset
  before they went forward, and a time they showed twice is the first of the two. A date without
  a zone is read the same way in the local zone.

  @api-QtQml.Date.fromLocaleString
  Scenario Outline: <text> is an instant in its zone
    Given the file "tst_zone.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Zone"
        function test_read() {
          const day = Date.fromLocaleString(Qt.locale("en_US"), "<text>", "<format>")
          compare(day.toISOString(), "<instant>")
        }
      }
      """
    When the tests are run in the time zone "Europe/Berlin"
    Then the test report is:
      """
      PASS   : Zone::initTestCase()
      PASS   : Zone::test_read()
      PASS   : Zone::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

    Examples:
      | text                                 | format                | instant                  |
      | 2026-07-01 12:00 Asia/Tokyo          | yyyy-MM-dd HH:mm tttt | 2026-07-01T03:00:00.000Z |
      | 2026-07-01 12:00 America/New_York    | yyyy-MM-dd HH:mm t    | 2026-07-01T16:00:00.000Z |
      | 2026-03-08 02:30 America/New_York    | yyyy-MM-dd HH:mm tttt | 2026-03-08T07:30:00.000Z |
      | 2026-11-01 01:30 America/New_York    | yyyy-MM-dd HH:mm tttt | 2026-11-01T05:30:00.000Z |
      | 2026-10-04 02:15 Australia/Lord_Howe | yyyy-MM-dd HH:mm tttt | 2026-10-03T15:45:00.000Z |
      | 2026-04-05 01:45 Australia/Lord_Howe | yyyy-MM-dd HH:mm tttt | 2026-04-04T14:45:00.000Z |
      | 1900-01-01 12:00 Asia/Kolkata        | yyyy-MM-dd HH:mm tttt | 1900-01-01T06:38:50.000Z |
      | 2026-07-01 12:00 Etc/GMT+5           | yyyy-MM-dd HH:mm tttt | 2026-07-01T17:00:00.000Z |
      | 2026-07-01 12:00 EST5EDT             | yyyy-MM-dd HH:mm tttt | 2026-07-01T16:00:00.000Z |
      | 2026-07-01 12:00 UTC                 | yyyy-MM-dd HH:mm tttt | 2026-07-01T12:00:00.000Z |
      | Europe/Berlin 2026-07-01 12:00       | tttt yyyy-MM-dd HH:mm | 2026-07-01T10:00:00.000Z |
      | 2026-07-01 12:00 Z                   | yyyy-MM-dd HH:mm t    | 2026-07-01T12:00:00.000Z |
      | 2026-07-01 12:00 UTC+05:30           | yyyy-MM-dd HH:mm t    | 2026-07-01T06:30:00.000Z |
      | 2026-07-01 12:00 +05:30              | yyyy-MM-dd HH:mm t    | 2026-07-01T06:30:00.000Z |
      | 2026-07-01 12:00 CEST                | yyyy-MM-dd HH:mm t    | 2026-07-01T10:00:00.000Z |

  # Berlin's clocks went from 02:00 to 03:00 on 29 March 2026 and from 03:00 back to 02:00 on
  # 25 October.
  @api-QtQml.Date.fromLocaleString
  Scenario Outline: A local time the clocks skipped or repeated, <text>, is one instant
    Given the file "tst_zone.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Zone"
        function test_read() {
          const day = Date.fromLocaleString(Qt.locale("en_US"), "<text>", "yyyy-MM-dd HH:mm")
          compare(day.toISOString(), "<instant>")
        }
      }
      """
    When the tests are run in the time zone "Europe/Berlin"
    Then the test report is:
      """
      PASS   : Zone::initTestCase()
      PASS   : Zone::test_read()
      PASS   : Zone::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

    Examples:
      | text             | instant                  |
      | 2026-03-29 02:00 | 2026-03-29T01:00:00.000Z |
      | 2026-03-29 02:30 | 2026-03-29T01:30:00.000Z |
      | 2026-10-25 02:30 | 2026-10-25T00:30:00.000Z |

  # The database's names are spelled as it spells them, and an abbreviation is only the local
  # zone's: Tokyo's is not read in Berlin.
  @api-QtQml.Date.fromLocaleString
  Scenario Outline: <text> names no zone
    Given the file "tst_zone.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Zone"
        function test_read() {
          const day = Date.fromLocaleString(Qt.locale("en_US"), "<text>", "<format>")
          verify(isNaN(day.getTime()))
        }
      }
      """
    When the tests are run in the time zone "Europe/Berlin"
    Then the test report is:
      """
      PASS   : Zone::initTestCase()
      PASS   : Zone::test_read()
      PASS   : Zone::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

    Examples:
      | text                                 | format                |
      | 2026-07-01 12:00 Foo/Bar             | yyyy-MM-dd HH:mm tttt |
      | 2026-07-01 12:00 europe/berlin       | yyyy-MM-dd HH:mm tttt |
      | 2026-07-01 12:00 Japan Standard Time | yyyy-MM-dd HH:mm tttt |
      | 2026-07-01 12:00 JST                 | yyyy-MM-dd HH:mm t    |
      | 2026-07-01 12:00 GMT+2               | yyyy-MM-dd HH:mm t    |
