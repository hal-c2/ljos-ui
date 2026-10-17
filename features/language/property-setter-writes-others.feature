# Sources:
#   https://doc.qt.io/qt-6/properties.html (a property's WRITE function and NOTIFY signal)
#   https://doc.qt.io/qt-6/qtqml-cppintegration-exposecppattributes.html
# The fixture type stands for one an application registers in Rust. Its
# setters, as a WRITE function in Qt might:
# - rootPath and zoom clear error on every write, equal ones included;
# - rootPath also counts its writes in visits;
# - visits keeps itself at 3 or below;
# - depth tries to write depth itself, and logs what it was told.

@ljos-only
Feature: A native type's setter writes its object's other properties
  A type registered from Rust can give a property a setter that does more
  than store the value, as a WRITE function does in Qt: it can change the
  object's other properties as well. What it writes is there, and heard,
  before the assignment that ran it returns.

  Scenario: An assignment clears the error before it returns
    Given the QML:
      """qml
      import QtQuick
      import Fixtures
      Browser {
        objectName: "b"
        property string seen: "unset"
        function open() { error = "no such folder"; rootPath = "/home"; seen = error }
      }
      """
    When the method "open" of "b" is called
    Then the property "seen" of "b" is ""
    And the property "rootPath" of "b" is "/home"

  # Change handlers hear only real changes; the setter sees every write.
  Scenario: A write of the value already held still runs the setter
    Given the QML:
      """qml
      import QtQuick
      import Fixtures
      Browser {
        objectName: "b"
        rootPath: "/home"
        property int pathHeard: 0
        property int errorHeard: 0
        onRootPathChanged: pathHeard++
        onErrorChanged: errorHeard++
        function fail() { error = "denied" }
        function again() { rootPath = "/home" }
      }
      """
    When the method "fail" of "b" is called
    And the method "again" of "b" is called
    Then the property "error" of "b" is ""
    And the property "pathHeard" of "b" is 0
    And the property "errorHeard" of "b" is 2

  Scenario: What the setter writes goes through that property's own setter
    Given the QML:
      """qml
      import QtQuick
      import Fixtures
      Browser {
        objectName: "b"
        function openMany() { for (var i = 0; i < 5; i++) rootPath = "/p" + i }
      }
      """
    When the method "openMany" of "b" is called
    Then the property "visits" of "b" is 3
    And the property "rootPath" of "b" is "/p4"

  # The setter reads visits to count; a binding of rootPath does not come
  # to depend on what its setter reads.
  Scenario: A binding's new value runs the setter, which it does not depend on
    Given the QML:
      """qml
      import QtQuick
      import Fixtures
      Browser {
        objectName: "b"
        property string wanted: "/a"
        rootPath: wanted
        function fail() { error = "denied" }
        function follow() { wanted = "/b" }
      }
      """
    When the method "fail" of "b" is called
    And the method "follow" of "b" is called
    Then the property "error" of "b" is ""
    And the property "rootPath" of "b" is "/b"
    And the property "visits" of "b" is 2
    And the log has no warnings

  Scenario: Each step of an animation runs the setter
    Given the QML:
      """qml
      import QtQuick
      import Fixtures
      Browser {
        objectName: "b"
        NumberAnimation on zoom { id: grow; from: 1; to: 2; duration: 200; running: false }
        function fail() { error = "denied" }
        function start() { grow.start() }
      }
      """
    When the method "start" of "b" is called
    And the method "fail" of "b" is called
    And 100 milliseconds pass
    Then the property "error" of "b" is ""

  Scenario: A setter that writes its own property is refused, and does not run again
    Given the QML:
      """qml
      import QtQuick
      import Fixtures
      Browser {
        objectName: "b"
        function deeper() { depth = 4 }
      }
      """
    When the method "deeper" of "b" is called
    Then the property "depth" of "b" is 4
    And the log has a warning matching "depth.*its own setter"
