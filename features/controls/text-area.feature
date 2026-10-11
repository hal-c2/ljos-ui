# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-textarea.html
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html
#   https://doc.qt.io/qt-6/qtquickcontrols-customize.html#customizing-textarea
# Verified on Qt 6.11.2 (qml6, offscreen; pointer and key input over its vnc platform).

Feature: TextArea
  A text area is a TextEdit with a background, a text shown while it is
  empty, and the font and palette of the controls around it. The numbers
  are those of the bundled font, Liberation Sans, at 14 pixels.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 300
        TextArea {
          objectName: "a"
          x: 20; y: 20; width: 200; height: 60
          font.family: "Liberation Sans"; font.pixelSize: 14
          readonly property Item hint: children[0]
          property string log: ""
          onPressed: (e) => log += "pressed " + e.x + "," + e.y + " "
          onReleased: (e) => log += "released " + e.x + "," + e.y + " "
          onPressAndHold: (e) => log += "held " + e.x + "," + e.y + " "
          onEditingFinished: log += "finished "
        }
        TextArea {
          objectName: "other"
          x: 20; y: 100
          font.family: "Liberation Sans"; font.pixelSize: 14
          text: "hello brave"
        }
      }
      """

  @qt-parity
  Rule: The Basic style

    Scenario Outline: An area starts out as a padded TextEdit
      Then the property "<property>" of "a" is <value>

      Examples:
        | property                 | value |
        | implicitWidth            | 16    |
        | implicitHeight           | 28    |
        | implicitBackgroundWidth  | 0     |
        | implicitBackgroundHeight | 0     |
        | padding                  | 6     |
        | leftPadding              | 10    |
        | topPadding               | 6     |
        | rightPadding             | 6     |
        | bottomPadding            | 6     |
        | topInset                 | 0     |
        | verticalAlignment        | 32    |
        | wrapMode                 | 0     |
        | lineCount                | 1     |
        | activeFocusOnTab         | true  |
        | selectByMouse            | true  |
        | hoverEnabled             | true  |
        | hovered                  | false |
        | focusReason              | 7     |

    Scenario Outline: Its colours are those of the palette
      Then the property "<property>" of "a" is <value>

      Examples:
        | property             | value       |
        | color                | "#353637"   |
        | selectionColor       | "#0066ff"   |
        | selectedTextColor    | "#090909"   |
        | placeholderTextColor | "#88353637" |

    @api-QtQuick.Controls.TextArea.background
    Scenario Outline: The background is a frame that is not shown
      Then the property "<property>" of "a" is <value>

      Examples:
        | property                | value     |
        | background.visible      | false     |
        | background.x            | 0         |
        | background.y            | 0         |
        | background.width        | 200       |
        | background.height       | 60        |
        | background.z            | -1        |
        | background.border.color | "#bdbdbd" |
        | background.border.width | 1         |

    Scenario: The frame of the area with the keyboard is thicker and in the highlight colour
      When the method "forceActiveFocus" of "a" is called
      Then the property "background.border.color" of "a" is "#0066ff"
      And the property "background.border.width" of "a" is 2
      And the property "background.visible" of "a" is false

    Scenario: An area that is not enabled is drawn in the colours for that
      When the property "enabled" of "a" is set to false
      Then the property "color" of "a" is "#7f353637"
      And the property "background.border.color" of "a" is "#e4e4e4"

    Scenario: The text takes its colour from the palette of the area
      When the property "palette.text" of "a" is set to "#aa0000"
      Then the property "color" of "a" is "#aa0000"

  @qt-parity
  Rule: Its size comes from the text, the background and the placeholder

    Scenario: An area is as large as its text and padding
      Then the property "implicitWidth" of "other" is within 0.01 of 84.44
      And the property "implicitHeight" of "other" is 28
      And the property "width" of "other" is within 0.01 of 84.44

    Scenario: Each line makes the area taller
      When the property "text" of "a" is set to "one\ntwo"
      Then the property "lineCount" of "a" is 2
      And the property "implicitHeight" of "a" is 44

    Scenario: An empty area is as wide as its placeholder
      When the property "placeholderText" of "a" is set to "placeholder"
      Then the property "implicitWidth" of "a" is within 0.01 of 88.34

    @api-QtQuick.Controls.TextArea.implicitBackgroundWidth
    @api-QtQuick.Controls.TextArea.implicitBackgroundHeight
    Scenario: A background with a size of its own is the least the area is
      When the property "background.implicitWidth" of "a" is set to 120
      And the property "background.implicitHeight" of "a" is set to 60
      Then the property "implicitBackgroundWidth" of "a" is 120
      And the property "implicitWidth" of "a" is 120
      And the property "implicitHeight" of "a" is 60

    @api-QtQuick.Controls.TextArea.topInset @api-QtQuick.Controls.TextArea.leftInset
    @api-QtQuick.Controls.TextArea.rightInset @api-QtQuick.Controls.TextArea.bottomInset
    Scenario: Insets move the edges of the background inwards
      When the property "leftInset" of "a" is set to 5
      And the property "topInset" of "a" is set to 2
      And the property "rightInset" of "a" is set to 3
      And the property "bottomInset" of "a" is set to 4
      Then the property "background.x" of "a" is 5
      And the property "background.y" of "a" is 2
      And the property "background.width" of "a" is 192
      And the property "background.height" of "a" is 54

  @qt-parity
  Rule: The placeholder shows while the area is empty

    @api-QtQuick.Controls.TextArea.placeholderText
    Scenario Outline: The placeholder lies where the text would be
      When the property "placeholderText" of "a" is set to "Notes"
      Then the property "<property>" of "a" is <value>

      Examples:
        | property               | value       |
        | placeholderText        | "Notes"     |
        | hint.text              | "Notes"     |
        | hint.visible           | true        |
        | hint.x                 | 10          |
        | hint.y                 | 6           |
        | hint.width             | 184         |
        | hint.height            | 48          |
        | hint.color             | "#88353637" |
        | hint.elide             | 1           |
        | hint.verticalAlignment | 32          |

    Scenario: A text hides the placeholder and an empty area shows it again
      When the property "placeholderText" of "a" is set to "Notes"
      And the property "text" of "a" is set to "x"
      Then the property "hint.visible" of "a" is false
      When the method "clear" of "a" is called
      Then the property "hint.visible" of "a" is true

    @api-QtQuick.Controls.TextArea.placeholderTextColor
    Scenario: The placeholder has a colour of its own
      When the property "placeholderTextColor" of "a" is set to "#123456"
      Then the property "hint.color" of "a" is "#123456"

    Scenario: A centred placeholder makes way for the cursor
      When the property "horizontalAlignment" of "a" is set to 4
      Then the property "hint.visible" of "a" is true
      When the method "forceActiveFocus" of "a" is called
      Then the property "hint.visible" of "a" is false

  @qt-parity
  Rule: The area knows whether the pointer is over it

    @api-QtQuick.Controls.TextArea.hovered
    Scenario: The pointer over the area
      When the user moves the pointer to 50, 35
      Then the property "hovered" of "a" is true
      And the property "hovered" of "other" is false
      When the user moves the pointer to 300, 250
      Then the property "hovered" of "a" is false

    @api-QtQuick.Controls.TextArea.hoverEnabled
    Scenario: An area that does not want to know is never hovered
      When the property "hoverEnabled" of "a" is set to false
      And the user moves the pointer to 50, 35
      Then the property "hovered" of "a" is false

  @qt-parity
  Rule: A press and a release are told, and a click takes the keyboard

    @api-QtQuick.Controls.TextArea.pressed @api-QtQuick.Controls.TextArea.released
    Scenario: A click is a press and a release where the pointer is in the area
      When the user touches 50, 35
      Then the property "log" of "a" is "pressed 30,15 released 30,15 "
      And the property "activeFocus" of "a" is true
      And the property "focusReason" of "a" is 0

    Scenario: The press alone does not take the keyboard
      When the user presses the pointer at 50, 35
      And 100 milliseconds pass
      Then the property "log" of "a" is "pressed 30,15 "
      And the property "activeFocus" of "a" is false

    @api-QtQuick.Controls.TextArea.pressAndHold
    Scenario: A press held for a while is a press and hold, which does not take the keyboard
      When the user presses the pointer at 50, 35
      And 1 second passes
      Then the property "log" of "a" is "pressed 30,15 held 30,15 "
      And the property "activeFocus" of "a" is false

    Scenario: A double click selects a word
      When the user double-clicks at 50, 110
      Then the property "selectedText" of "other" is "hello"

    Scenario: An area that is not enabled tells of nothing
      When the property "enabled" of "a" is set to false
      And the user touches 50, 35
      Then the property "log" of "a" is ""
      And the property "activeFocus" of "a" is false

  @qt-parity
  Rule: It is edited like a TextEdit

    @api-QtQuick.Controls.TextArea.focusReason
    Scenario: The Tab key gives the keyboard to an area and says so
      When the user presses "Tab"
      Then the property "activeFocus" of "a" is true
      And the property "focusReason" of "a" is 1

    Scenario: An area that cannot be typed into lets the Tab key move on
      When the method "forceActiveFocus" of "a" is called
      And the property "readOnly" of "a" is set to true
      And the user presses "Tab"
      Then the property "text" of "a" is ""
      And the property "activeFocus" of "other" is true

    Scenario: Return begins a new line and ends nothing
      When the user presses "Tab"
      And the user types "abc"
      And the user presses "Return"
      Then the property "lineCount" of "a" is 2
      And the property "length" of "a" is 4
      And the property "cursorPosition" of "a" is 4
      And the property "log" of "a" is ""

    Scenario: A text set by a script leaves the cursor at the beginning
      When the property "text" of "a" is set to "abc"
      Then the property "cursorPosition" of "a" is 0

    Scenario: A text too long for the area wraps when it is told to
      When the property "wrapMode" of "a" is set to 1
      And the property "text" of "a" is set to "one two three four five six seven eight nine ten"
      Then the property "lineCount" of "a" is 2
      And the property "contentWidth" of "a" is at most 184

  # The oracle's vnc platform sends the Tab key without the tab a keyboard
  # sends with it, so an area there types nothing. What a keyboard does is
  # in https://doc.qt.io/qt-6/qml-qtquick-controls-textarea.html#tab-focus
  @ljos-only
  Rule: The Tab key types a tab

    Scenario: Tab in an area that has the keyboard goes into its text
      When the method "forceActiveFocus" of "a" is called
      And the user types "ab"
      And the user presses "Tab"
      Then the property "text" of "a" is "ab\t"
      And the property "cursorPosition" of "a" is 3
      And the property "activeFocus" of "a" is true

  # The oracle has no input method.
  @ljos-only
  Rule: An input method composes in the area

    Scenario: What is being composed hides the placeholder
      When the property "placeholderText" of "a" is set to "Notes"
      And the method "forceActiveFocus" of "a" is called
      And the input method composes "ni"
      Then the property "hint.visible" of "a" is false
      And the property "preeditText" of "a" is "ni"
      When the input method commits "日本"
      Then the property "text" of "a" is "日本"

  @ljos-only @a11y
  Rule: A screen reader is told what the area is for

    Scenario: An area is editable text of many lines, described by its placeholder
      When the property "placeholderText" of "a" is set to "What happened"
      And the property "Accessible.name" of "a" is set to "Notes"
      Then the object "a" is exposed as a "multilinetextinput" named "Notes"
      And the "multilinetextinput" named "Notes" is described as "What happened"
