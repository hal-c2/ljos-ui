# Sources:
#   https://doc.qt.io/qt-6/qtqml-javascript-hostenvironment.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html
#   docs/ARCHITECTURE.md (the engine's clock)

# The wall clock is a host setting of Ljós: a test, a replay or a picture
# of an example can say what time it is. Qt reads the machine's clock only,
# so the oracle has no such setting.
@ljos-only
Feature: A pinned wall clock
  A host can say what time `new Date()` and `Date.now()` see. The time then
  moves with application time, so a script that reads it twice sees exactly
  what passed in between.

  Background:
    Given the system clock reads "2026-10-11T17:57:09Z"
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string iso: new Date().toISOString()
        property string stamp: ""
        property double ms: 0
        property string fixed: new Date(2020, 0, 2, 3, 4, 5).toISOString()
        property bool isDate: new Date() instanceof Date && new Date().constructor === Date
        property string later: new (class Later extends Date {})().toISOString()
        property bool called: Date() === new Date().toString()
        function sample() {
          stamp = new Date().toISOString()
          ms = Date.now()
        }
      }
      """

  Scenario: A new Date without arguments is the pinned time
    Then the property "iso" of "root" is "2026-10-11T17:57:09.000Z"

  Scenario: Date.now is the pinned time in milliseconds
    When the method "sample" of "root" is called
    Then the property "ms" of "root" is 1791741429000

  Scenario: Time moves with application time
    When 90 seconds pass
    And the method "sample" of "root" is called
    Then the property "stamp" of "root" is "2026-10-11T17:58:39.000Z"
    And the property "ms" of "root" is 1791741519000

  Scenario: A Date made from arguments is not the clock's business
    Then the property "fixed" of "root" is not "2026-10-11T17:57:09.000Z"

  Scenario: A Date of the pinned time is a Date
    Then the property "isDate" of "root" is true

  Scenario: A subclass of Date made without arguments is the pinned time
    Then the property "later" of "root" is "2026-10-11T17:57:09.000Z"

  Scenario: Date called as a function is the pinned time as text
    Then the property "called" of "root" is true
