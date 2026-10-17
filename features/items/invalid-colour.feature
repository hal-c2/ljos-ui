# Sources:
#   https://doc.qt.io/qt-6/qml-color.html
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#color-prop
#   https://doc.qt.io/qt-6/qml-qtquick-rectangle.html#color-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: An item given a name that is not a colour
  A colour written in the file must be a colour, but a binding can give a
  colour property any text. Text that names no colour makes an invalid
  colour, quietly: nothing is logged. Most items keep it as it is, invalid;
  a Text keeps opaque black instead. Either way what is painted is black,
  the channels an invalid colour has.

  Scenario Outline: <item>.<property> given "bogus" reads <colour>, valid <valid>
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "o"
        property string bad: "bogus"
        <item> { id: it; <property>: "red" }
        Component.onCompleted: it.<property> = bad
        property string seen: String(it.<property>)
        property bool valid: it.<property>.valid
      }
      """
    Then the property "seen" of "o" is "<colour>"
    And the property "valid" of "o" is <valid>
    And the log has no warnings

    Examples:
      | item      | property          | colour  | valid |
      | Rectangle | color             | #000000 | false |
      | Rectangle | border.color      | #000000 | false |
      | TextInput | color             | #000000 | false |
      | TextInput | selectionColor    | #000000 | false |
      | TextInput | selectedTextColor | #000000 | false |
      | TextEdit  | color             | #000000 | false |
      | TextEdit  | selectionColor    | #000000 | false |
      | TextEdit  | selectedTextColor | #000000 | false |

  Scenario Outline: Text.<property> given "<given>" is black
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "o"
        property string given: "<given>"
        Text { id: it; <property>: "red" }
        Component.onCompleted: it.<property> = given
        property string seen: String(it.<property>)
      }
      """
    Then the property "seen" of "o" is "#000000"
    And the log has no warnings

    Examples:
      | property   | given |
      | color      | bogus |
      | linkColor  | bogus |
      | styleColor | bogus |
      | color      |       |

  # Ljós keeps what was written, so the colour is black but invalid: a
  # native property has no way yet to change a value as it is written.
  @backlog
  Scenario: The black a Text keeps is a valid colour
    Given the QML:
      """qml
      import QtQuick
      Text {
        objectName: "t"; color: "red"
        Component.onCompleted: color = "bogus"
        property bool valid: color.valid
      }
      """
    Then the property "valid" of "t" is true

  Scenario: A Rectangle with an invalid colour paints black
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 40; height: 40; color: "#336699"
        property string bad: "bogus"
        Rectangle { anchors.fill: parent; color: parent.bad }
      }
      """
    Then the pixel at 20, 20 of the window is "#000000"

  Scenario: A Text with an invalid colour paints its letters black
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 60; height: 60; color: "white"
        property string bad: "bogus"
        Text {
          anchors.fill: parent; text: "█"; color: parent.bad
          font.pixelSize: 60; horizontalAlignment: Text.AlignHCenter
        }
      }
      """
    Then the pixel at 30, 30 of the window is "#000000"

  Scenario Outline: <written>, written in the file, does not load
    Given the file "Bad.qml":
      """qml
      import QtQuick
      Rectangle { <written> }
      """
    Then loading "Bad.qml" is rejected

    Examples:
      | written                   |
      | color: "bogus"            |
      | color: ""                 |
      | color: "#ff000"           |
      | color: "#8f00"            |
      | color: " #f00"            |
      | color: `bogus`            |
      | property color c: "bogus" |
      | border.color: "bogus"     |

  Scenario Outline: <written> is <colour>, valid <valid>
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "o"; <written>
        property string seen: String(color)
        property bool valid: color.valid
      }
      """
    Then the property "seen" of "o" is "<colour>"
    And the property "valid" of "o" is <valid>

    Examples:
      | written            | colour    | valid |
      | color: ("bogus")   | #000000   | false |
      | color: "  red "    | #ff0000   | true  |
      | color: "Steel Blue" | #4682b4  | true  |
      | color: "#123456789" | #124578  | true  |
      | color: "#ABCDEF123456" | #abee34 | true |
