# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#formatDateTime-method
#   https://doc.qt.io/qt-6/qdatetime.html#toString (the `t` letters)
#   https://www.unicode.org/reports/tr35/tr35-dates.html#Time_Zone_Names
#   tzfile(5)
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).
# Each run is a program of its own started in the zone the scenario names (`TZ`), with
# LANG=en_US.UTF-8, so a date made in local time is in that zone.

@qt-parity @headless
Feature: A date writes the name of its time zone
  In a date's format, `t` is the zone's abbreviation and `tttt` its long name, at that date: a zone
  that keeps daylight saving time has one name in winter and another in summer. The abbreviation
  is the one the system's zone database gives, in the application's locale and in English; a
  locale of another language writes the one its own language has, or the offset from GMT. The
  long name is in the locale's language. A zone without a name is written as its offset.

  @api-QtQml.Qt.formatDateTime
  Scenario Outline: A date in <zone> names its zone
    Given the file "tst_zone.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Zone"
        function test_name() {
          const day = new Date(<date>)
          compare(Qt.formatDateTime(day, "h:mm AP t"), "<short>")
          compare(Qt.formatDateTime(day, "tttt"), "<long>")
        }
      }
      """
    When the tests are run in the time zone "<zone>"
    Then the test report is:
      """
      PASS   : Zone::initTestCase()
      PASS   : Zone::test_name()
      PASS   : Zone::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

    Examples:
      | zone                | date              | short        | long                             |
      | America/Los_Angeles | 2026, 0, 1, 14, 5 | 2:05 PM PST  | Pacific Standard Time            |
      | America/Los_Angeles | 2026, 6, 1, 14, 5 | 2:05 PM PDT  | Pacific Daylight Time            |
      | Europe/Berlin       | 2026, 0, 1, 14, 5 | 2:05 PM CET  | Central European Standard Time   |
      | Europe/Berlin       | 2026, 6, 1, 14, 5 | 2:05 PM CEST | Central European Summer Time     |
      | Australia/Sydney    | 2026, 0, 1, 14, 5 | 2:05 PM AEDT | Australian Eastern Daylight Time |
      | Asia/Tokyo          | 2026, 6, 1, 14, 5 | 2:05 PM JST  | Japan Standard Time              |
      | UTC                 | 2026, 6, 1, 14, 5 | 2:05 PM UTC  | Coordinated Universal Time       |

  # tzfile names some zones by their offset alone, and CLDR has no name for a zone that is only an
  # offset from GMT.
  @api-QtQml.Qt.formatDateTime
  Scenario Outline: A zone the database names by its offset in <zone>
    Given the file "tst_zone.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Zone"
        function test_name() {
          const day = new Date(2026, 0, 1, 14, 5)
          compare(Qt.formatDateTime(day, "t"), "<short>")
          compare(Qt.formatDateTime(day, "tttt"), "<long>")
        }
      }
      """
    When the tests are run in the time zone "<zone>"
    Then the test report is:
      """
      PASS   : Zone::initTestCase()
      PASS   : Zone::test_name()
      PASS   : Zone::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

    Examples:
      | zone              | short | long                   |
      | Asia/Kathmandu    | +0545 | Nepal Time             |
      | America/Sao_Paulo | -03   | Brasilia Standard Time |
      | Etc/GMT+5         | -05   | GMT-05:00              |

  @api-QtQml.Date.toLocaleString
  Scenario Outline: A locale names the zone in its own language
    Given the file "tst_zone.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Zone"
        function test_name() {
          const winter = new Date(2026, 0, 1, 14, 5)
          const summer = new Date(2026, 6, 1, 14, 5)
          compare(winter.toLocaleString(Qt.locale("<locale>"), "t|tttt"), "<winter>")
          compare(summer.toLocaleString(Qt.locale("<locale>"), "t|tttt"), "<summer>")
        }
      }
      """
    When the tests are run in the time zone "<zone>"
    Then the test report is:
      """
      PASS   : Zone::initTestCase()
      PASS   : Zone::test_name()
      PASS   : Zone::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

    Examples:
      | zone                | locale | winter                                         | summer                                       |
      | Europe/Berlin       | de_DE  | MEZ\|Mitteleuropäische Normalzeit              | MESZ\|Mitteleuropäische Sommerzeit           |
      | Europe/Berlin       | en_GB  | CET\|Central European Standard Time            | CEST\|Central European Summer Time           |
      | Europe/Berlin       | C      | CET\|Central European Standard Time            | CEST\|Central European Summer Time           |
      | America/Los_Angeles | de_DE  | GMT-8\|Nordamerikanische Westküsten-Normalzeit | GMT-7\|Nordamerikanische Westküsten-Sommerzeit |
      | UTC                 | de_DE  | UTC\|Koordinierte Weltzeit                     | UTC\|Koordinierte Weltzeit                   |

  # Qt's runs are made in UTC, the zone the ledger's own are made in.
  @api-QtQml.Date.toLocaleString
  Scenario: The application names the zone in its locale's language
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        readonly property var day: new Date(2026, 0, 1, 14, 5)
        property string de: day.toLocaleString(Qt.locale("de_DE"), "t tttt")
        property string fr: day.toLocaleString(Qt.locale("fr_FR"), "tttt")
      }
      """
    Then the property "de" of "root" is "UTC Koordinierte Weltzeit"
    And the property "fr" of "root" is "temps universel coordonné"
