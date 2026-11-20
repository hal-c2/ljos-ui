# Sources:
#   https://doc.qt.io/qt-6/qml-singleton.html
#   docs/ARCHITECTURE.md §9.3 (faults: JsInterrupted, JsOutOfMemory)
#   docs/ARCHITECTURE.md §9.4 (Supervisor: content a resource fault took is made again)

# Qt has no time limit on JavaScript: a script that never returns hangs the application.
@ljos-only
Feature: A singleton whose script was interrupted is made again at a later lookup
  A singleton a script was interrupted in while it was made, or that a full
  script heap kept from being made, is not available at that lookup. It is
  not given up on: the next lookup makes it again, at once after the first
  such fault and after a wait of 1 second, doubling up to a minute, after
  each one soon after. A singleton with a code bug stays unavailable, as in Qt.

  Background:
    Given JavaScript is timed by a clock on which native work a script asks for takes 0 seconds
    And the file "look/qmldir":
      """
      singleton Tints Tints.qml
      """

  Scenario: A singleton whose script is interrupted once is there at the next lookup
    Given the file "look/Tints.qml":
      """qml
      pragma Singleton
      import QtQuick
      QtObject {
        property int gap: 6
        Component.onCompleted: {
          Tries.count += 1
          if (Tries.count === 1) for (;;) { }
        }
      }
      """
    And the file "look/Tries.qml":
      """qml
      pragma Singleton
      import QtQuick
      QtObject { property int count: 0 }
      """
    And the file "look/qmldir":
      """
      singleton Tints Tints.qml
      singleton Tries Tries.qml
      """
    And the QML:
      """qml
      import QtQuick
      import "look"
      Item {
        objectName: "root"
        property int gap: 0
        function look() { try { gap = Tints.gap } catch (e) { gap = -1 } }
      }
      """
    When the method "look" of "root" is called
    Then the log has a warning matching "the singleton Tints is not available: .*interrupted.*made again at the first lookup from now on"
    And the application has no fault
    When the method "look" of "root" is called
    Then the property "gap" of "root" is 6

  Scenario: A singleton whose script is interrupted each time is tried again after a wait
    Given the file "look/Tints.qml":
      """qml
      pragma Singleton
      import QtQuick
      QtObject {
        property int gap: 6
        Component.onCompleted: { for (;;) { } }
      }
      """
    And the QML:
      """qml
      import QtQuick
      import "look"
      Item {
        objectName: "root"
        property int gap: 0
        function look() { try { gap = Tints.gap } catch (e) { gap = -1 } }
      }
      """
    When the method "look" of "root" is called
    And the method "look" of "root" is called
    Then the log has a warning matching "the singleton Tints is not available: .*made again at the first lookup in 1 s"
    And the application has no fault
