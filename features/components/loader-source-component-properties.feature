# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#setSource-method
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#sourceComponent-prop
# Verified on Qt 6.11.2 and 6.12.0 (qml6, offscreen).

@qt-parity
Feature: A sourceComponent is loaded without setSource's properties
  The properties setSource gives are for the source it loads. A Loader
  given an empty source with them, and then a sourceComponent, makes the
  component's item without them, at once or asynchronously: what the item
  has keeps its own value, and what it lacks is not looked for.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property alias asynchronous: loader.asynchronous
        Component { id: comp; Item { objectName: "page"; property int k: 0 } }
        Loader { id: loader; objectName: "loader" }
        function load() {
          loader.setSource("", { nope: 1, k: 2 })
          loader.sourceComponent = comp
        }
      }
      """

  Scenario Outline: The component's item has none of them, <how>
    When the property "asynchronous" of "root" is set to <asynchronous>
    And the method "load" of "root" is called
    And 50 milliseconds pass
    Then the property "status" of "loader" is 1
    And the property "k" of "loader/page" is 0
    And the log has no warnings

    Examples:
      | how            | asynchronous |
      | at once        | false        |
      | asynchronously | true         |
