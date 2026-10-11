# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-textfield.html
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html
#   https://doc.qt.io/qt-6/qtquickcontrols-customize.html#customizing-textfield
# Verified on Qt 6.11.2 (qml6, offscreen; pointer and key input over its vnc platform).

Feature: TextField
  A text field is a TextInput with a background, a text shown while it is
  empty, and the font and palette of the controls around it. The numbers
  are those of the bundled font, Liberation Sans, at 14 pixels.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 300
        TextField {
          objectName: "f"
          x: 20; y: 20
          font.family: "Liberation Sans"; font.pixelSize: 14
          readonly property Item hint: children[0]
          property string log: ""
          onPressed: (e) => log += "pressed " + e.x + "," + e.y + " "
          onReleased: (e) => log += "released " + e.x + "," + e.y + " "
          onPressAndHold: (e) => log += "held " + e.x + "," + e.y + " "
          onEditingFinished: log += "finished "
        }
        TextField {
          objectName: "other"
          x: 20; y: 100
          font.family: "Liberation Sans"; font.pixelSize: 14
          text: "hello brave"
        }
      }
      """

  @qt-parity
  Rule: The Basic style

    Scenario Outline: An empty field is as large as its background
      Then the property "<property>" of "f" is <value>

      Examples:
        | property                 | value |
        | width                    | 200   |
        | height                   | 40    |
        | implicitWidth            | 200   |
        | implicitHeight           | 40    |
        | implicitBackgroundWidth  | 200   |
        | implicitBackgroundHeight | 40    |
        | padding                  | 6     |
        | leftPadding              | 10    |
        | topPadding               | 6     |
        | rightPadding             | 6     |
        | bottomPadding            | 6     |
        | topInset                 | 0     |
        | leftInset                | 0     |
        | verticalAlignment        | 128   |
        | horizontalAlignment      | 1     |
        | activeFocusOnTab         | true  |
        | selectByMouse            | true  |
        | hoverEnabled             | true  |
        | hovered                  | false |

    Scenario Outline: Its colours are those of the palette
      Then the property "<property>" of "f" is <value>

      Examples:
        | property               | value       |
        | color                  | "#353637"   |
        | selectionColor         | "#0066ff"   |
        | selectedTextColor      | "#090909"   |
        | placeholderTextColor   | "#88353637" |
        | background.color       | "#ffffff"   |

    @api-QtQuick.Controls.TextField.background
    Scenario Outline: The background fills the field and lies behind the text
      Then the property "<property>" of "f" is <value>

      Examples:
        | property                | value     |
        | background.x            | 0         |
        | background.y            | 0         |
        | background.width        | 200       |
        | background.height       | 40        |
        | background.z            | -1        |
        | background.border.color | "#bdbdbd" |
        | background.border.width | 1         |

    Scenario: The field with the keyboard has a thicker frame in the highlight colour
      When the method "forceActiveFocus" of "f" is called
      Then the property "background.border.color" of "f" is "#0066ff"
      And the property "background.border.width" of "f" is 2

    Scenario: A field that is not enabled is drawn in the colours for that
      When the property "enabled" of "f" is set to false
      Then the property "color" of "f" is "#7f353637"
      And the property "background.color" of "f" is "#d6d6d6"
      And the property "selectionColor" of "f" is "#f0f6ff"

    Scenario: The text takes its colour from the palette of the field
      When the property "palette.text" of "f" is set to "#aa0000"
      Then the property "color" of "f" is "#aa0000"

    Scenario: A colour its author gave the text stays when the palette changes
      When the property "color" of "f" is set to "#00aa00"
      And the property "palette.text" of "f" is set to "#aa0000"
      Then the property "color" of "f" is "#00aa00"

  @qt-parity
  Rule: Its size comes from the background, the text and the placeholder

    @api-QtQuick.Controls.TextField.implicitBackgroundWidth
    Scenario: A wider background makes a wider field
      When the property "background.implicitWidth" of "f" is set to 260
      Then the property "implicitBackgroundWidth" of "f" is 260
      And the property "width" of "f" is 260

    Scenario: A taller text makes a taller field
      When the property "font.pixelSize" of "f" is set to 40
      Then the property "implicitHeight" of "f" is 57
      And the property "implicitWidth" of "f" is 200

    Scenario: More padding makes a taller field
      When the property "padding" of "f" is set to 20
      Then the property "implicitHeight" of "f" is 56
      And the property "leftPadding" of "f" is 24

    Scenario: Without a background an empty field is as large as its padding and one line
      When the property "background" of "f" is set to null
      Then the property "implicitWidth" of "f" is 16
      And the property "implicitHeight" of "f" is 28
      And the property "implicitBackgroundWidth" of "f" is 0

    Scenario: Without a background the field is as wide as its text
      When the property "background" of "f" is set to null
      And the property "text" of "f" is set to "hello"
      Then the property "implicitWidth" of "f" is within 0.01 of 45.56

    Scenario: Without a background the field is as wide as its placeholder
      When the property "background" of "f" is set to null
      And the property "placeholderText" of "f" is set to "placeholder"
      Then the property "implicitWidth" of "f" is within 0.01 of 88.34

    @api-QtQuick.Controls.TextField.topInset @api-QtQuick.Controls.TextField.leftInset
    @api-QtQuick.Controls.TextField.rightInset @api-QtQuick.Controls.TextField.bottomInset
    Scenario: Insets move the edges of the background inwards and make the field larger
      When the property "leftInset" of "f" is set to 5
      And the property "rightInset" of "f" is set to 7
      And the property "topInset" of "f" is set to 3
      And the property "bottomInset" of "f" is set to 4
      Then the property "implicitWidth" of "f" is 212
      And the property "implicitHeight" of "f" is 47
      And the property "background.x" of "f" is 5
      And the property "background.y" of "f" is 3
      And the property "background.width" of "f" is 200
      And the property "background.height" of "f" is 40

  @qt-parity
  Rule: The placeholder shows while the field is empty

    @api-QtQuick.Controls.TextField.placeholderText
    Scenario Outline: The placeholder lies where the text would be
      When the property "placeholderText" of "f" is set to "Name"
      Then the property "<property>" of "f" is <value>

      Examples:
        | property               | value       |
        | placeholderText        | "Name"      |
        | hint.text              | "Name"      |
        | hint.visible           | true        |
        | hint.x                 | 10          |
        | hint.y                 | 6           |
        | hint.width             | 184         |
        | hint.height            | 28          |
        | hint.color             | "#88353637" |
        | hint.elide             | 1           |
        | hint.verticalAlignment | 128         |
        | hint.font.family       | "Liberation Sans" |

    Scenario: A text hides the placeholder and an empty field shows it again
      When the property "placeholderText" of "f" is set to "Name"
      And the property "text" of "f" is set to "x"
      Then the property "hint.visible" of "f" is false
      When the method "clear" of "f" is called
      Then the property "hint.visible" of "f" is true

    @api-QtQuick.Controls.TextField.placeholderTextColor
    Scenario: The placeholder has a colour of its own
      When the property "placeholderTextColor" of "f" is set to "#123456"
      Then the property "hint.color" of "f" is "#123456"

    Scenario: The placeholder follows the palette
      When the property "palette.placeholderText" of "f" is set to "#654321"
      Then the property "placeholderTextColor" of "f" is "#654321"
      And the property "hint.color" of "f" is "#654321"

    Scenario: The placeholder stays while the field has the keyboard
      When the method "forceActiveFocus" of "f" is called
      Then the property "hint.visible" of "f" is true

    Scenario: A centred placeholder makes way for the cursor
      When the property "horizontalAlignment" of "f" is set to 4
      Then the property "hint.horizontalAlignment" of "f" is 4
      And the property "hint.visible" of "f" is true
      When the method "forceActiveFocus" of "f" is called
      Then the property "hint.visible" of "f" is false

  @qt-parity
  Rule: The field knows whether the pointer is over it

    @api-QtQuick.Controls.TextField.hovered
    Scenario: The pointer over the field
      When the user moves the pointer to 50, 35
      Then the property "hovered" of "f" is true
      And the property "hovered" of "other" is false
      When the user moves the pointer to 300, 250
      Then the property "hovered" of "f" is false

    @api-QtQuick.Controls.TextField.hoverEnabled
    Scenario: A field that does not want to know is never hovered
      When the property "hoverEnabled" of "f" is set to false
      And the user moves the pointer to 50, 35
      Then the property "hovered" of "f" is false

    Scenario: A field that is not enabled is still hovered
      When the property "enabled" of "f" is set to false
      And the user moves the pointer to 50, 35
      Then the property "hovered" of "f" is true

  @qt-parity
  Rule: A press and a release are told, and a click takes the keyboard

    @api-QtQuick.Controls.TextField.pressed @api-QtQuick.Controls.TextField.released
    Scenario: A click is a press and a release where the pointer is in the field
      When the user touches 50, 35
      Then the property "log" of "f" is "pressed 30,15 released 30,15 "
      And the property "activeFocus" of "f" is true
      And the property "cursorVisible" of "f" is true

    Scenario: The press alone does not take the keyboard
      When the user presses the pointer at 50, 35
      And 100 milliseconds pass
      Then the property "log" of "f" is "pressed 30,15 "
      And the property "activeFocus" of "f" is false

    Scenario: A press that begins to move takes the keyboard
      When the user presses the pointer at 50, 35
      And the user moves the pointer to 60, 35
      Then the property "activeFocus" of "f" is true

    @api-QtQuick.Controls.TextField.pressAndHold
    Scenario: A press held for a while is a press and hold
      When the user presses the pointer at 50, 35
      And 1 second passes
      Then the property "log" of "f" is "pressed 30,15 held 30,15 "

    Scenario: A press and hold does not take the keyboard, nor does its release
      When the user presses the pointer at 50, 35
      And 1 second passes
      And the user releases the pointer at 50, 35
      Then the property "activeFocus" of "f" is false

    Scenario: A long press is not a click in a field nobody asked to tell of holds
      When the user presses the pointer at 50, 115
      And 1 second passes
      And the user releases the pointer at 50, 115
      Then the property "activeFocus" of "other" is false

    Scenario: A press released elsewhere selected on its way
      When the user presses the pointer at 50, 115
      And the user releases the pointer at 250, 150
      Then the property "activeFocus" of "other" is true
      And the property "selectedText" of "other" is "lo brave"
      And the property "cursorPosition" of "other" is 11

    Scenario: A double click selects a word
      When the user double-clicks at 50, 115
      Then the property "selectedText" of "other" is "hello"

    Scenario: A drag selects
      When the user drags from 32, 115 to 80, 115
      Then the property "selectedText" of "other" is "hello bra"

    Scenario: A field that is not enabled tells of nothing
      When the property "enabled" of "f" is set to false
      And the user touches 50, 35
      Then the property "log" of "f" is ""
      And the property "activeFocus" of "f" is false

    Scenario: A read-only field takes the keyboard and shows no cursor
      When the property "readOnly" of "f" is set to true
      And the user touches 50, 35
      Then the property "activeFocus" of "f" is true
      And the property "cursorVisible" of "f" is false
      And the property "log" of "f" is "pressed 30,15 released 30,15 "

    Scenario: Without activeFocusOnPress a click is told and the keyboard stays where it is
      When the property "activeFocusOnPress" of "f" is set to false
      And the user touches 50, 35
      Then the property "log" of "f" is "pressed 30,15 released 30,15 "
      And the property "activeFocus" of "f" is false

  @qt-parity
  Rule: The field says why it last got or lost the keyboard

    @api-QtQuick.Controls.TextField.focusReason
    Scenario: A field that never had the keyboard has no reason to tell
      Then the property "focusReason" of "f" is 7

    Scenario: A click is the pointer's reason
      When the user touches 50, 35
      Then the property "focusReason" of "f" is 0

    Scenario: The Tab key is its own reason
      When the user presses "Tab"
      Then the property "activeFocus" of "f" is true
      And the property "focusReason" of "f" is 1

    Scenario: Shift+Tab is its own reason
      When the user presses "Shift+Tab"
      Then the property "activeFocus" of "other" is true
      And the property "focusReason" of "other" is 2

    Scenario: A script gives no reason
      When the method "forceActiveFocus" of "f" is called
      Then the property "activeFocus" of "f" is true
      And the property "focusReason" of "f" is 7

    Scenario: Losing the keyboard to the Tab key is told too
      When the user touches 50, 35
      And the user presses "Tab"
      Then the property "activeFocus" of "f" is false
      And the property "focusReason" of "f" is 1
      And the property "focusReason" of "other" is 1

  @qt-parity
  Rule: It is edited like a TextInput

    Scenario: Typing fills the field and hides the placeholder
      When the property "placeholderText" of "f" is set to "Name"
      And the user presses "Tab"
      And the user types "abc"
      Then the property "text" of "f" is "abc"
      And the property "length" of "f" is 3
      And the property "hint.visible" of "f" is false

    Scenario: Return ends the editing
      When the user presses "Tab"
      And the user types "abc"
      And the user presses "Return"
      Then the property "log" of "f" is "finished "
      And the property "activeFocus" of "f" is true

    Scenario: Leaving the field ends the editing
      When the user presses "Tab"
      And the user presses "Tab"
      Then the property "log" of "f" is "finished "
      And the property "activeFocus" of "other" is true

    Scenario: A text set by a script leaves the cursor at its end
      When the property "text" of "f" is set to "abc"
      Then the property "cursorPosition" of "f" is 3

  # The oracle has no input method.
  @ljos-only
  Rule: An input method composes in the field

    Scenario: What is being composed hides the placeholder
      When the property "placeholderText" of "f" is set to "Name"
      And the method "forceActiveFocus" of "f" is called
      And the input method composes "ni"
      Then the property "hint.visible" of "f" is false
      And the property "preeditText" of "f" is "ni"
      When the input method composes ""
      Then the property "hint.visible" of "f" is true

    Scenario: What is committed is typed
      When the method "forceActiveFocus" of "f" is called
      And the input method commits "日本"
      Then the property "text" of "f" is "日本"
      And the property "hint.visible" of "f" is false

  @ljos-only @a11y
  Rule: A screen reader is told what the field is for

    Scenario: A field is editable text, described by its placeholder
      When the property "placeholderText" of "f" is set to "Your name"
      And the property "Accessible.name" of "f" is set to "Name"
      Then the object "f" is exposed as a "textinput" named "Name"
      And the "textinput" named "Name" is described as "Your name"

    Scenario: A description its author gave stays
      When the property "Accessible.name" of "f" is set to "Name"
      And the property "Accessible.description" of "f" is set to "As in your passport"
      And the property "placeholderText" of "f" is set to "Your name"
      Then the "textinput" named "Name" is described as "As in your passport"

    Scenario: A field that hides its text does not tell it
      When the property "Accessible.name" of "f" is set to "Secret"
      And the property "text" of "f" is set to "hunter2"
      And the property "echoMode" of "f" is set to 2
      Then the "passwordinput" named "Secret" has the value "•••••••"
