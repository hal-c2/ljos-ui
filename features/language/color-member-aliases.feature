# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#property-aliases
#   https://doc.qt.io/qt-6/qml-color.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (bindings), buckets value:color:step-set(literal),
# warning:other(ljos-only):alias, warning:TypeError(ljos-only):Qt.size+alias+string-times-one
# and throws:none/Error:step-set(ref).

@qt-parity
Feature: An alias of a channel of a colour property
  An alias can name one channel of a colour property, `root.tint.g` or `root.tint.a`, as it names
  a member of any other value. Reading it reads the channel; giving it a value changes that
  channel of the colour and keeps the others.

  @backlog
  Scenario: Giving the alias a value changes that channel
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property color tint: Qt.rgba(0, 0.75, 0, 0)
        property alias green: root.tint.g
        function clear() { green = 0 }
      }
      """
    When the method "clear" of "root" is called
    Then the property "tint" of "root" is "#00000000"

  @backlog
  Scenario: An alias of the alpha channel reads it
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property color tint: "#80ff0000"
        property alias alpha: root.tint.a
        property string text: alpha.toFixed(2)
      }
      """
    Then the property "text" of "root" is "0.50"
    And the log has no warnings

  @backlog
  Scenario: What an alias of a channel reads can be given to another property
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property color tint: "red"
        property alias red: root.tint.r
        function move() { root.x = root.red }
      }
      """
    When the method "move" of "root" is called
    Then the property "x" of "root" is 1
