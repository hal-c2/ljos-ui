# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-date.html
#   https://tc39.es/ecma262/#sec-date.parse
#   https://tc39.es/ecma262/#sec-date.prototype.toutcstring
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (builtins), buckets value:load:text-nan-qt, value:load:number,
# result:none/none:call:nan-qt, value:step:text and result:none/none:call:text.

@qt-parity
Feature: Dates read from and written as text
  Date.parse reads an ISO date or the forms Date itself writes, and anything else is NaN: a
  number alone is no date. toUTCString writes the weekday, the month, the day, the time and the
  year, in that order.

  Scenario Outline: Date.parse of "<text>" is NaN
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string seen: String(Date.parse("<text>"))
      }
      """
    Then the property "seen" of "root" is "NaN"

    Examples:
      | text    |
      | 0       |
      | 10      |
      | 3.5     |
      | 1,2,3   |
      | 0.00000 |
      | 02024   |
      | +2024   |
      | -1      |

  Scenario Outline: Date.parse of "<text>" is a date
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string seen: String(Date.parse("<text>"))
      }
      """
    Then the property "seen" of "root" is "<seen>"

    Examples:
      | text                          | seen          |
      | 2024                          | 1704067200000 |
      | 2024-01-02                    | 1704153600000 |
      | Thu, 01 Jan 1970 00:00:00 GMT | 0             |
      | Thu Jan 1 00:00:00 1970 GMT   | 0             |

  Scenario: toUTCString puts the year last
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string seen: new Date(0).toUTCString() + "|" + new Date(Date.UTC(2024, 1, 5, 3, 4, 5)).toUTCString()
      }
      """
    Then the property "seen" of "root" is "Thu Jan 1 00:00:00 1970 GMT|Mon Feb 5 03:04:05 2024 GMT"

  Scenario: A date is written with its day unpadded and its year last
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property var d: new Date(2024, 1, 5, 3, 4, 5)
        property string seen: [d.toString().startsWith("Mon Feb 5 03:04:05 2024 GMT"), d.toDateString(),
                               d.toTimeString(), String(d) === d.toString()].join("|")
      }
      """
    Then the property "seen" of "root" is "true|Mon Feb 5 2024|03:04:05|true"

  Scenario: A year before 1000 is padded, and a year before 1 counts from -1
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        function at(y) { const d = new Date(0); d.setUTCFullYear(y); return d.toUTCString() }
        property string seen: [at(99), at(-50), at(12345)].join("|")
      }
      """
    Then the property "seen" of "root" is "Thu Jan 1 00:00:00 0099 GMT|Sun Jan 1 00:00:00 -051 GMT|Mon Jan 1 00:00:00 12345 GMT"

  Scenario: An invalid date is written as such
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property var d: new Date(NaN)
        property string seen: [d.toString(), d.toUTCString(), d.toDateString(), d.toTimeString(),
                               Date.prototype.toGMTString === Date.prototype.toUTCString].join("|")
      }
      """
    Then the property "seen" of "root" is "Invalid Date|Invalid Date|||true"
