# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-combobox.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-textfield.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: An editable combo box
  A combo box that is `editable` holds a field to type in. What is typed
  is its `editText`; Return accepts it, and a row with that text becomes
  the current one.

  Rule: The content of a combo box is a text field

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 400; height: 400; color: "white"
          ComboBox { objectName: "c"; editable: true; model: ["One", "Two", "Three"] }
          ComboBox { objectName: "plain"; y: 50; model: ["One", "Two", "Three"] }
          ComboBox { objectName: "off"; y: 100; editable: true; enabled: false; model: ["One"] }
          ComboBox { objectName: "flat"; y: 150; editable: true; flat: true; model: ["One"] }
        }
        """

    @api-QtQuick.Controls.ComboBox.editable
    Scenario Outline: The field of the combo box "<combo>" has <property> <value>
      Then the property "contentItem.<property>" of "<combo>" is <value>

      Examples:
        | combo | property           | value     |
        | c     | text               | "One"     |
        | c     | enabled            | true      |
        | c     | x                  | 0         |
        | c     | y                  | 0         |
        | c     | width              | 100       |
        | c     | height             | 40        |
        | c     | leftPadding        | 12        |
        | c     | activeFocusOnTab   | true      |
        | c     | color              | "#353637" |
        | c     | background.visible | true      |
        | plain | text               | "One"     |
        | plain | enabled            | false     |
        | plain | width              | 100       |
        | plain | color              | "#26282a" |
        | plain | background.visible | false     |
        | off   | enabled            | false     |
        | flat  | background.visible | false     |

    Scenario Outline: The field is framed, and the indicator is beside it
      Then the pixel at <x>, <y> of the window is "<colour>"

      Examples:
        | x   | y   | colour  |
        | 0   | 0   | #e0e0e0 |
        | 1   | 1   | #ffffff |
        | 50  | 39  | #e0e0e0 |
        | 98  | 20  | #ffffff |
        | 99  | 20  | #e0e0e0 |
        | 120 | 5   | #e0e0e0 |
        | 1   | 101 | #e0e0e0 |
        | 1   | 151 | #ffffff |
        | 120 | 155 | #ffffff |

    Scenario Outline: The field that has the keyboard has a frame two pixels wide
      When the user touches 60, 20
      Then the pixel at <x>, <y> of the window is "<colour>"

      Examples:
        | x   | y  | colour  |
        | 1   | 1  | #0066ff |
        | 2   | 2  | #ffffff |
        | 50  | 38 | #0066ff |
        | 98  | 20 | #0066ff |
        | 100 | 20 | #e0e0e0 |

    @api-QtQuick.Controls.ComboBox.inputMethodHints
    Scenario: The field is told not to complete what is typed by itself
      Then the property "inputMethodHints" of "c" is 64
      And the property "contentItem.inputMethodHints" of "c" is 64

    @api-QtQuick.Controls.ComboBox.selectTextByMouse
    Scenario: The pointer selects no text unless the combo box says so
      Then the property "contentItem.selectByMouse" of "c" is false

    Scenario: The field takes no text while the popup is open
      When the user touches 120, 20
      Then the property "contentItem.readOnly" of "c" is true

    Scenario: A combo box that becomes editable has a field that can be typed in
      When the property "editable" of "plain" is set to true
      Then the property "contentItem.enabled" of "plain" is true
      And the property "contentItem.background.visible" of "plain" is true
      And the property "contentItem.color" of "plain" is "#353637"

  Rule: What is typed is the edit text

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 400; height: 400; color: "white"
          ComboBox { objectName: "c"; editable: true; model: ["One", "Two", "Three"] }
          Button { objectName: "other"; y: 100 }
        }
        """

    @api-QtQuick.Controls.ComboBox.editText
    Scenario: The edit text starts as the text of the current row
      Then the property "editText" of "c" is "One"

    Scenario: The edit text follows the current row
      When the property "currentIndex" of "c" is set to 1
      Then the property "editText" of "c" is "Two"
      And the property "contentItem.text" of "c" is "Two"

    Scenario: An edit text that is set is shown and leaves the current row
      When the property "editText" of "c" is set to "Two"
      Then the property "contentItem.text" of "c" is "Two"
      And the property "currentIndex" of "c" is 0
      And the property "currentText" of "c" is "One"

    Scenario: A touch on the field gives it the keyboard and opens nothing
      When the user touches 95, 20
      Then the property "activeFocus" of "c" is true
      And the property "contentItem.activeFocus" of "c" is true
      And the property "contentItem.cursorPosition" of "c" is 3
      And the property "popup.visible" of "c" is false

    Scenario: The Tab key gives the field the keyboard
      When the user presses "Tab"
      Then the property "contentItem.activeFocus" of "c" is true

    Scenario: The Tab key goes on from the field to what comes next
      When the user presses "Tab"
      And the user presses "Tab"
      Then the property "activeFocus" of "other" is true

    Scenario: The Tab key with Shift comes back to the field
      When the user presses "Tab"
      And the user presses "Tab"
      And the user presses "Shift+Tab"
      Then the property "contentItem.activeFocus" of "c" is true

    Scenario: A combo box that is given the keyboard has it itself
      When the property "focus" of "c" is set to true
      Then the property "activeFocus" of "c" is true
      And the property "contentItem.activeFocus" of "c" is false

    Scenario: A typed character leaves the current row as it is
      When the user touches 95, 20
      And the user types "x"
      Then the property "editText" of "c" is "Onex"
      And the property "currentIndex" of "c" is 0
      And the property "currentText" of "c" is "One"
      And the property "displayText" of "c" is "One"

    Scenario: The Space key types a space
      When the user touches 95, 20
      And the user presses "Space"
      Then the property "editText" of "c" is "One "
      And the property "popup.visible" of "c" is false

    Scenario: The Backspace key takes a character away
      When the user touches 95, 20
      And the user presses "Backspace"
      Then the property "editText" of "c" is "On"
      And the property "currentIndex" of "c" is 0

    Scenario: The Escape key changes nothing
      When the user touches 95, 20
      And the user types "x"
      And the user presses "Escape"
      Then the property "editText" of "c" is "Onex"
      And the property "contentItem.activeFocus" of "c" is true

    Scenario: The keyboard going elsewhere leaves what was typed
      When the user touches 95, 20
      And the user types "x"
      And the user touches 50, 120
      Then the property "activeFocus" of "c" is false
      And the property "editText" of "c" is "Onex"
      And the property "currentIndex" of "c" is 0

    Scenario Outline: The <key> key in the field moves to another row
      When the property "currentIndex" of "c" is set to 1
      And the user touches 95, 20
      And the user presses "<key>"
      Then the property "currentIndex" of "c" is <row>
      And the property "editText" of "c" is "<text>"

      Examples:
        | key  | row | text  |
        | Down | 2   | Three |
        | Up   | 0   | One   |

    Scenario: The cursor stays at the end of the text of the row moved to
      When the user touches 95, 20
      And the user presses "Down"
      Then the property "contentItem.cursorPosition" of "c" is 3
      And the property "contentItem.selectedText" of "c" is ""

    Scenario: The Down key leaves what was typed for the next row
      When the user touches 95, 20
      And the user types "x"
      And the user presses "Down"
      Then the property "currentIndex" of "c" is 1
      And the property "editText" of "c" is "Two"

    Scenario: The Home key moves the cursor and not the row
      When the property "currentIndex" of "c" is set to 1
      And the user touches 95, 20
      And the user presses "Home"
      Then the property "currentIndex" of "c" is 1
      And the property "contentItem.cursorPosition" of "c" is 0

    Scenario: A row made current replaces what was typed
      When the user touches 95, 20
      And the user types "x"
      And the property "currentIndex" of "c" is set to 1
      Then the property "editText" of "c" is "Two"
      And the property "contentItem.text" of "c" is "Two"

    @api-QtQuick.Controls.ComboBox.selectAll
    Scenario: All of the text can be selected
      When the method "selectAll" of "c" is called
      Then the property "contentItem.selectedText" of "c" is "One"

  Rule: A row that starts with what is typed completes it

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 400; height: 400; color: "white"
          ComboBox { objectName: "c"; editable: true; model: ["One", "Two", "Three"] }
        }
        """
      When the user touches 95, 20
      And the method "selectAll" of "c" is called

    Scenario Outline: Typing "<typed>" shows "<text>" with "<selected>" selected
      When the user types "<typed>"
      Then the property "contentItem.text" of "c" is "<text>"
      And the property "contentItem.selectedText" of "c" is "<selected>"
      And the property "editText" of "c" is "<text>"
      And the property "currentIndex" of "c" is 0

      Examples:
        | typed | text  | selected |
        | T     | Two   | wo       |
        | t     | two   | wo       |
        | th    | three | ree      |
        | Thx   | Thx   |          |
        | x     | x     |          |

    Scenario: The cursor is after what was typed
      When the user types "T"
      Then the property "contentItem.cursorPosition" of "c" is 1

    Scenario: The Backspace key takes the completion away and completes nothing
      When the user types "t"
      And the user presses "Backspace"
      Then the property "contentItem.text" of "c" is "t"
      And the property "contentItem.selectedText" of "c" is ""

    Scenario: Typing on goes over the completion
      When the user types "Tw"
      Then the property "contentItem.text" of "c" is "Two"
      And the property "contentItem.selectedText" of "c" is "o"

  Rule: The Return key accepts the text

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 400; height: 400; color: "white"
          ComboBox {
            id: c; objectName: "c"; editable: true
            model: ["One", "Two", "Three"]
            property int times: 0
            property bool keep: false
            property int rowWhenAccepted: -2
            onAccepted: {
              times++
              rowWhenAccepted = currentIndex
              if (keep && find(editText) === -1) model = model.concat([editText])
            }
          }
        }
        """
      When the user touches 95, 20

    @api-QtQuick.Controls.ComboBox.accepted
    Scenario: The Return key says the text was accepted
      When the user presses "Return"
      Then the property "times" of "c" is 1
      And the property "currentIndex" of "c" is 0
      And the property "popup.visible" of "c" is false

    Scenario: A text that is no row leaves no row current
      When the user types "x"
      And the user presses "Return"
      Then the property "times" of "c" is 1
      And the property "currentIndex" of "c" is -1
      And the property "currentText" of "c" is ""
      And the property "displayText" of "c" is ""
      And the property "editText" of "c" is "Onex"

    Scenario: The handler sees the row that was current before
      When the user types "x"
      And the user presses "Return"
      Then the property "rowWhenAccepted" of "c" is 0

    Scenario: A text the handler adds to the model becomes the current row
      When the property "keep" of "c" is set to true
      And the user types "x"
      And the user presses "Return"
      Then the property "count" of "c" is 4
      And the property "currentIndex" of "c" is 3
      And the property "currentText" of "c" is "Onex"

    Scenario: Accepting the same text again changes nothing
      When the user types "x"
      And the user presses "Return"
      And the user presses "Return"
      Then the property "times" of "c" is 2
      And the property "currentIndex" of "c" is -1
      And the property "editText" of "c" is "Onex"

    Scenario: A row made current after a text that is no row shows its text
      When the user types "x"
      And the user presses "Return"
      And the property "currentIndex" of "c" is set to 0
      Then the property "editText" of "c" is "One"

    Scenario: An empty text leaves no row current
      When the method "selectAll" of "c" is called
      And the user presses "Backspace"
      And the user presses "Return"
      Then the property "currentIndex" of "c" is -1
      And the property "editText" of "c" is ""

    Scenario: A row that has the text is current before the handler runs
      When the method "selectAll" of "c" is called
      And the user types "two"
      And the user presses "Return"
      Then the property "rowWhenAccepted" of "c" is 1

    Scenario: A completed text is accepted whole
      When the method "selectAll" of "c" is called
      And the user types "t"
      And the user presses "Return"
      Then the property "currentIndex" of "c" is 1
      And the property "editText" of "c" is "Two"
      And the property "contentItem.selectedText" of "c" is ""
      And the property "contentItem.cursorPosition" of "c" is 3

    Scenario: The row is found whatever the case of its letters
      When the method "selectAll" of "c" is called
      And the user types "two"
      And the user presses "Return"
      Then the property "currentIndex" of "c" is 1
      And the property "editText" of "c" is "Two"
      And the property "times" of "c" is 1

  Rule: A validator says which texts are accepted

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 400; height: 400; color: "white"
          ComboBox {
            objectName: "words"; editable: true; model: ["One", "Two"]
            validator: IntValidator { top: 9 }
            property int times: 0
            onAccepted: times++
          }
          ComboBox {
            objectName: "numbers"; y: 50; editable: true; model: ["1", "2"]
            validator: IntValidator { top: 9 }
            property int times: 0
            onAccepted: times++
          }
          ComboBox {
            objectName: "fixed"; y: 100; model: ["One", "Two"]
            validator: IntValidator { top: 9 }
          }
        }
        """

    @api-QtQuick.Controls.ComboBox.validator @api-QtQuick.Controls.ComboBox.acceptableInput
    Scenario Outline: The text of "<combo>" is acceptable: <acceptable>
      Then the property "acceptableInput" of "<combo>" is <acceptable>

      Examples:
        | combo   | acceptable |
        | words   | false      |
        | numbers | true       |

    Scenario: A combo box that was never editable has nothing to refuse
      Then the property "acceptableInput" of "fixed" is true

    Scenario: A combo box that stops being editable keeps what the validator said
      When the property "editable" of "words" is set to false
      Then the property "acceptableInput" of "words" is false

    Scenario: A combo box that becomes editable has no answer until its text changes
      When the property "editable" of "fixed" is set to true
      Then the property "acceptableInput" of "fixed" is true

    Scenario: The Return key accepts nothing the validator refuses
      When the user touches 95, 20
      And the user presses "Return"
      Then the property "times" of "words" is 0

    Scenario: The Return key accepts what the validator allows
      When the user touches 95, 70
      And the user presses "Return"
      Then the property "times" of "numbers" is 1

    Scenario: A character the validator refuses is not typed
      When the user touches 95, 70
      And the user types "x"
      Then the property "editText" of "numbers" is "1"

  Rule: The indicator opens the popup

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 400; height: 400; color: "white"
          ComboBox { objectName: "c"; editable: true; model: ["One", "Two", "Three"] }
        }
        """

    Scenario: A touch on the indicator opens the popup and gives the combo box the keyboard
      When the user touches 120, 20
      Then the property "popup.visible" of "c" is true
      And the property "down" of "c" is true
      And the property "activeFocus" of "c" is true
      And the property "contentItem.activeFocus" of "c" is false
      And the property "highlightedIndex" of "c" is 0

    Scenario: A row of the popup is picked with the keys
      When the user touches 120, 20
      And the user presses "Down"
      And the user presses "Return"
      Then the property "currentIndex" of "c" is 1
      And the property "editText" of "c" is "Two"
      And the property "popup.visible" of "c" is false

    Scenario: A row of the popup is picked with a touch
      When the user touches 120, 20
      And the user touches 50, 100
      Then the property "currentIndex" of "c" is 1
      And the property "editText" of "c" is "Two"
      And the property "popup.visible" of "c" is false

    Scenario: A touch on the field closes the open popup and gives the field the keyboard
      When the user touches 120, 20
      And the user touches 50, 20
      Then the property "popup.visible" of "c" is false
      And the property "contentItem.activeFocus" of "c" is true

    Scenario: A touch on the indicator takes the keyboard from the field
      When the user touches 95, 20
      And the user touches 120, 20
      Then the property "popup.visible" of "c" is true
      And the property "activeFocus" of "c" is true
      And the property "contentItem.activeFocus" of "c" is false
      And the property "contentItem.focus" of "c" is false

    Scenario: The combo box keeps the keyboard when a row was picked
      When the user touches 95, 20
      And the user touches 120, 20
      And the user presses "Down"
      And the user presses "Return"
      Then the property "currentIndex" of "c" is 1
      And the property "activeFocus" of "c" is true
      And the property "contentItem.activeFocus" of "c" is false

    Scenario Outline: The <key> key does nothing for a combo box that has the keyboard itself
      When the user touches 120, 20
      And the user presses "Escape"
      And the user presses "<key>"
      Then the property "popup.visible" of "c" is false
      And the property "editText" of "c" is "One"

      Examples:
        | key    |
        | Return |
        | Space  |

    Scenario: A combo box that has the keyboard itself types nothing
      When the user touches 120, 20
      And the user presses "Escape"
      And the user types "T"
      Then the property "editText" of "c" is "One"
      And the property "currentIndex" of "c" is 0
