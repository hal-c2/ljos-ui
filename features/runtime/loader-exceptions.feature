# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#status-prop
#   https://doc.qt.io/qt-6/qtquick-debugging.html
#   docs/ARCHITECTURE.md §9.3 (faults)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: An exception in a loaded item does not unload it
  A Loader's status is about loading. An exception thrown later by a
  handler of the loaded item is logged and the item stays; an item that
  could not be loaded at all is `Loader.Error`.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Component {
          id: page
          Item { objectName: "page"; property int n: 0; onNChanged: { throw new Error("page failed") } }
        }
        Loader { objectName: "loader"; sourceComponent: page }
        Loader { objectName: "lost"; source: "Nowhere.qml" }
      }
      """

  Scenario: The item stays loaded after its handler threw
    When the property "n" of "page" is set to 1
    Then the object "loader/page" exists
    And the property "status" of "loader" is 1

  Scenario: The exception is logged
    When the property "n" of "page" is set to 1
    Then the log has a warning matching "Scenario.qml:6: .*page failed"

  Scenario: A file that cannot be loaded is an error
    Then the property "status" of "lost" is 3
    And the property "item" of "lost" is null
