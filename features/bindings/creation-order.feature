# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (bindings), bucket value:int:literal.

@qt-parity
Feature: A change handler run while the object is made
  The bindings of an object are made one after another. When one is made its change handler
  runs at once, before the bindings that come after it: what the handler writes to one of those
  replaces it before it is made.

  @backlog
  Scenario: A handler that writes a property whose binding is not made yet replaces the binding
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string label: (0).toPrecision(3)
        height: Text.AlignRight
        property int runs: 0
        onLabelChanged: { runs++; root.height = root.height }
      }
      """
    Then the property "runs" of "root" is 1
    And the property "height" of "root" is 0

  Scenario: A property given a plain value runs no handler while the object is made
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string label: "a"
        height: Text.AlignRight
        property int runs: 0
        onLabelChanged: { runs++; root.height = root.height }
      }
      """
    Then the property "runs" of "root" is 0
    And the property "height" of "root" is 2
