# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-accessibility.html
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-abstractbutton.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-checkbox.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-combobox.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menu.html
#   docs/ARCHITECTURE.md (section 14)
# The documentation says that the controls are accessible out of the box and
# does not list what each one says. What a control says of itself here is our
# reading of what its properties mean; the Qt oracle cannot read it through
# QML, so these scenarios run on Ljós only.

@ljos-only @a11y
Feature: What the controls say about their state
  A control tells an assistive technology how it is, as well as what it is,
  without the application saying anything.

  Rule: Buttons that are checked

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        ApplicationWindow {
          width: 400; height: 400
          Column {
            CheckBox { objectName: "agree"; text: "Agree" }
            CheckBox { objectName: "all"; text: "All"; tristate: true }
            Switch { objectName: "wifi"; text: "Wi-Fi" }
            RadioButton { objectName: "daily"; text: "Daily" }
            Button { objectName: "save"; text: "Save" }
            Button { objectName: "bold"; text: "Bold"; checkable: true }
            CheckBox {
              objectName: "own"; text: "Own"
              Accessible.checked: true
            }
          }
        }
        """
      And an assistive technology is listening

    Scenario Outline: A control that can be checked says so
      Then the "<role>" named "<name>" is checkable
      And the "<role>" named "<name>" is not checked

      Examples:
        | role        | name  |
        | checkbox    | Agree |
        | checkbox    | All   |
        | switch      | Wi-Fi |
        | radiobutton | Daily |
        | button      | Bold  |

    Scenario: A plain button cannot be checked
      Then the "button" named "Save" is not checkable

    @api-QtQuick.Controls.AbstractButton.checked
    Scenario Outline: A checked control says that it is checked
      When the property "checked" of "<object>" is set to true
      Then the "<role>" named "<name>" is checked

      Examples:
        | object | role        | name  |
        | agree  | checkbox    | Agree |
        | wifi   | switch      | Wi-Fi |
        | daily  | radiobutton | Daily |
        | bold   | button      | Bold  |

    Scenario: A control that is unchecked again says so
      When the property "checked" of "agree" is set to true
      And the property "checked" of "agree" is set to false
      Then the "checkbox" named "Agree" is not checked

    Scenario: A click of a screen reader is heard back as the new state
      When the user clicks the "checkbox" "Agree"
      Then the "checkbox" named "Agree" is checked

    @api-QtQuick.Controls.CheckBox.checkState
    Scenario: A partially checked box says so
      When the property "checkState" of "all" is set to 1
      Then the "checkbox" named "All" is partially checked
      And the "checkbox" named "All" is not checked

    @api-QtQuick.Controls.AbstractButton.checkable
    Scenario: A button that is made checkable says so
      When the property "checkable" of "save" is set to true
      Then the "button" named "Save" is checkable

    Scenario: What the application says wins over the state of the control
      Then the "checkbox" named "Own" is checked

  Rule: Buttons that are held down or switched off

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        ApplicationWindow {
          width: 400; height: 400
          Column {
            objectName: "column"
            Button { objectName: "save"; text: "Save" }
            Button { objectName: "ok"; text: "OK"; highlighted: true }
          }
        }
        """
      And an assistive technology is listening

    @api-QtQuick.Controls.AbstractButton.down
    Scenario: A button that is down is pressed
      When the property "down" of "save" is set to true
      Then the "button" named "Save" is pressed

    Scenario: A button at rest is not pressed
      Then the "button" named "Save" is not pressed

    Scenario: A button that is not enabled is disabled
      When the property "enabled" of "save" is set to false
      Then the "button" named "Save" is disabled

    Scenario: A button inside something that is not enabled is disabled
      When the property "enabled" of "column" is set to false
      Then the "button" named "Save" is disabled

  Rule: Text controls

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        ApplicationWindow {
          width: 400; height: 400
          Column {
            TextField { objectName: "user"; Accessible.name: "User" }
            TextField {
              objectName: "secret"; Accessible.name: "Secret"
              echoMode: TextInput.Password
            }
            TextArea { objectName: "notes"; Accessible.name: "Notes" }
            Label { text: "Volume" }
          }
        }
        """
      And an assistive technology is listening

    Scenario: A field that can be typed into is not read-only
      Then the "textinput" named "User" is not read-only

    @api-QtQuick.Controls.TextField.readOnly
    Scenario: A read-only field says so
      When the property "readOnly" of "user" is set to true
      Then the "textinput" named "User" is read-only

    @api-QtQuick.Controls.TextArea.readOnly
    Scenario: A read-only text area says so
      When the property "readOnly" of "notes" is set to true
      Then the "multilinetextinput" named "Notes" is read-only

    Scenario: A field that hides what is typed is a password field
      Then the accessibility tree has a "passwordinput" named "Secret"

    Scenario: A field that shows what is typed again is a plain field
      When the property "echoMode" of "secret" is set to 0
      Then the accessibility tree has a "textinput" named "Secret"

    Scenario: A label is read-only
      Then the "label" named "Volume" is read-only

  Rule: A combo box and its rows

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 400; height: 400
          ComboBox { objectName: "c"; model: ["One", "Two", "Three"] }
        }
        """
      And an assistive technology is listening

    Scenario: A closed combo box can be expanded and is not
      Then the "combobox" named "One" is expandable
      And the "combobox" named "One" is not expanded

    @api-QtQuick.Controls.ComboBox.popup
    Scenario: A combo box whose list is open is expanded
      When the user clicks the "combobox" "One"
      Then the "combobox" named "One" is expanded

    Scenario: A combo box whose list closed again is not expanded
      When the user clicks the "combobox" "One"
      And the user presses "Escape"
      Then the "combobox" named "One" is not expanded

    Scenario Outline: A row says where it is among the rows
      When the user clicks the "combobox" "One"
      Then the "listitem" named "<row>" is item <place> of 3

      Examples:
        | row   | place |
        | One   | 1     |
        | Two   | 2     |
        | Three | 3     |

    @api-QtQuick.Controls.ComboBox.currentIndex
    Scenario: The row that is picked is the selected one
      When the property "currentIndex" of "c" is set to 1
      And the user clicks the "combobox" "Two"
      Then the "listitem" named "Two" is selected
      And the "listitem" named "One" is not selected

    Scenario: A screen reader can ask a combo box to expand
      When an assistive technology asks the "combobox" named "One" to expand
      Then the property "popup.visible" of "c" is true

    Scenario: A screen reader can ask a combo box to collapse
      When the user clicks the "combobox" "One"
      And an assistive technology asks the "combobox" named "One" to collapse
      Then the property "popup.visible" of "c" is false

  Rule: The items of a menu

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 400; height: 400
          Menu {
            objectName: "m"
            title: "File"
            MenuItem { text: "Open" }
            MenuItem { objectName: "wrap"; text: "Wrap"; checkable: true }
            MenuSeparator {}
            MenuItem { text: "Quit"; enabled: false }
          }
        }
        """
      And an assistive technology is listening

    Scenario Outline: A menu item says where it is among the items
      When the method "open" of "m" is called
      Then the "menuitem" named "<item>" is item <place> of 3

      Examples:
        | item | place |
        | Open | 1     |
        | Wrap | 2     |
        | Quit | 3     |

    @api-QtQuick.Controls.MenuItem
    Scenario: A checkable menu item says so
      When the method "open" of "m" is called
      Then the "menuitem" named "Wrap" is checkable
      And the "menuitem" named "Wrap" is not checked
      And the "menuitem" named "Open" is not checkable

    Scenario: A checked menu item says so
      When the method "open" of "m" is called
      And the property "checked" of "wrap" is set to true
      Then the "menuitem" named "Wrap" is checked

    Scenario: A menu item that is not enabled is disabled
      When the method "open" of "m" is called
      Then the "menuitem" named "Quit" is disabled

  Rule: Controls that have a value

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 250
          SpinBox {
            objectName: "n"; from: 2; to: 12; value: 5
            Accessible.name: "Count"
          }
          Flickable {
            y: 60; width: 150; height: 100
            contentWidth: 150; contentHeight: 400
            ScrollBar.vertical: ScrollBar { objectName: "v"; Accessible.name: "Down" }
          }
        }
        """
      And an assistive technology is listening

    @api-QtQuick.Controls.SpinBox.value
    Scenario: A spin box says where it is between its ends
      Then the "spinbutton" named "Count" is at 5 in the range 2 to 12

    Scenario: The range of a spin box follows its value
      When the property "value" of "n" is set to 9
      Then the "spinbutton" named "Count" is at 9 in the range 2 to 12

    @api-QtQuick.Controls.ScrollBar.position
    Scenario: A scroll bar says where it is between its ends
      Then the "scrollbar" named "Down" is at 0 in the range 0 to 0.75
