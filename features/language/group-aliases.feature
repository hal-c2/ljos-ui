# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#property-aliases
#   https://doc.qt.io/qt-6/qml-qtquick-rectangle.html#border-prop
#   https://doc.qt.io/qt-6/qml-font.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Writing the members of a group through an alias
  An alias can name a whole group, `box.border`, or a whole value with
  members, `label.font`. Whoever uses the component then writes one member
  through it, `frame.color: "red"` or `labelFont.bold: true`, as they
  would on the item itself; the other members keep their values.

  Background:
    Given the file "Tag.qml":
      """qml
      import QtQuick
      Item {
        property alias frame: box.border
        property alias labelFont: label.font
        Rectangle { id: box; objectName: "box"; border.color: "green"; border.width: 1 }
        Text { id: label; objectName: "label"; text: "t"; font.pixelSize: 10; font.italic: true }
      }
      """

  @api-QtQuick.Rectangle.border
  Scenario: The members of a group are written through its alias
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"; property string seen: ""
        Tag { id: tag; frame.color: "#9d9faa"; frame.width: 3 }
        function look() { seen = tag.frame.color + " " + tag.frame.width }
      }
      """
    Then the property "border.color" of "box" is "#9d9faa"
    And the property "border.width" of "box" is 3
    When the method "look" of "root" is called
    Then the property "seen" of "root" is "#9d9faa 3"

  @api-QtQuick.Text.font
  Scenario: The members of a font are written through its alias, and the others kept
    Given the QML:
      """qml
      import QtQuick
      Item { Tag { objectName: "tag"; labelFont.bold: true; labelFont.pixelSize: 20 } }
      """
    Then the property "font.bold" of "label" is true
    And the property "font.pixelSize" of "label" is 20
    And the property "font.italic" of "label" is true
    And the property "labelFont.pixelSize" of "tag" is 20

  Scenario: A member bound through the alias follows its binding
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; property int w: 2
        Tag { frame.width: root.w * 2 }
      }
      """
    Then the property "border.width" of "box" is 4
    When the property "w" of "root" is set to 5
    Then the property "border.width" of "box" is 10
