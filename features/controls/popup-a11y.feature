# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-dialog.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menu.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-tooltip.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-combobox.html
#   docs/ARCHITECTURE.md (section 14)
# The Qt oracle cannot see the accessibility tree through QML alone, so these
# scenarios run on Ljós only.

@ljos-only @a11y
Feature: Popups with an assistive technology
  A dialog, a menu, a tool tip and a combo box say what they are, a popup is
  in the tree only while it is shown, and the keyboard is said to be inside
  the popup that has it.

  Rule: A dialog

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 400; height: 400
          Button { objectName: "under"; text: "Under" }
          Dialog {
            objectName: "d"
            title: "Save?"
            modal: true
            standardButtons: Dialog.Ok | Dialog.Cancel
            Item {
              objectName: "field"; width: 50; height: 20; focus: true
              Accessible.role: Accessible.EditableText
              Accessible.name: "Name"
            }
          }
          Popup { objectName: "plain"; width: 50; height: 50
                  Text { text: "Hint"; Accessible.role: Accessible.StaticText } }
        }
        """
      And an assistive technology is listening

    Scenario: A dialog that is not shown is not in the tree
      Then the accessibility tree has no "dialog" named "Save?"
      And the accessibility tree has no "button" named "OK"

    @api-QtQuick.Controls.Dialog.title
    Scenario: An open dialog is a dialog named by its title
      When the method "open" of "d" is called
      Then the accessibility tree has a "dialog" named "Save?"
      And the "dialog" named "Save?" is inside the "window" named ""

    Scenario: What is in a dialog is inside it in the tree
      When the method "open" of "d" is called
      Then the "button" named "OK" is inside the "dialog" named "Save?"
      And the "button" named "Cancel" is inside the "dialog" named "Save?"
      And the "textinput" named "Name" is inside the "dialog" named "Save?"

    Scenario: A dialog that closed is out of the tree again
      When the method "open" of "d" is called
      And the method "reject" of "d" is called
      Then the accessibility tree has no "dialog" named "Save?"

    @api-QtQuick.Controls.Popup.focus
    Scenario: The keyboard is said to be inside the open dialog
      When the method "open" of "d" is called
      Then the accessibility tree's focus is the "textinput" named "Name"

    Scenario: The keyboard is said to be back in the window when the dialog closes
      When the method "open" of "d" is called
      And the method "reject" of "d" is called
      Then the accessibility tree's focus is the "window" named ""

    Scenario: A screen reader's press on a button of the dialog closes it
      When the method "open" of "d" is called
      And the user clicks the "button" "OK"
      Then the property "visible" of "d" is false
      And the property "result" of "d" is 1

    Scenario: A plain popup is a dialog without a name
      When the method "open" of "plain" is called
      Then the accessibility tree has a "dialog" named ""
      And the "label" named "Hint" is inside the "dialog" named ""

  Rule: A menu

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string log: ""
          Menu {
            objectName: "m"
            title: "File"
            MenuItem { text: "Open"; onTriggered: root.log += "open;" }
            MenuItem { text: "Wrap"; checkable: true }
            MenuSeparator {}
            MenuItem { text: "Quit"; enabled: false }
          }
        }
        """
      And an assistive technology is listening

    @api-QtQuick.Controls.Menu.title
    Scenario: An open menu is a menu named by its title
      When the method "open" of "m" is called
      Then the accessibility tree has a "menu" named "File"

    Scenario: A menu that is not shown is not in the tree
      Then the accessibility tree has no "menu" named "File"
      And the accessibility tree has no "menuitem" named "Open"

    @api-QtQuick.Controls.MenuItem
    Scenario: The items of a menu are menu items inside it
      When the method "open" of "m" is called
      Then the "menuitem" named "Open" is inside the "menu" named "File"
      And the "menuitem" named "Wrap" is inside the "menu" named "File"
      And the "menuitem" named "Quit" is inside the "menu" named "File"

    Scenario: The keyboard is said to be on the current item of the menu
      When the method "open" of "m" is called
      And the user presses "Down"
      And the user presses "Down"
      Then the accessibility tree's focus is the "menuitem" named "Wrap"

    Scenario: A screen reader's press on a menu item triggers it
      When the method "open" of "m" is called
      And the user clicks the "menuitem" "Open"
      Then the property "log" of "root" is "open;"
      And the property "visible" of "m" is false

  Rule: A tool tip

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 400; height: 400
          Button {
            objectName: "b"; text: "Save"
            ToolTip { objectName: "t"; text: "Writes the file" }
          }
        }
        """
      And an assistive technology is listening

    @api-QtQuick.Controls.ToolTip.text
    Scenario: A shown tool tip is a tool tip named by its text
      When the method "open" of "t" is called
      Then the accessibility tree has a "tooltip" named "Writes the file"

    Scenario: A tool tip that is not shown is not in the tree
      Then the accessibility tree has no "tooltip" named "Writes the file"

    Scenario: A tool tip takes its new text as its name
      When the method "open" of "t" is called
      And the property "text" of "t" is set to "Saves"
      Then the accessibility tree has a "tooltip" named "Saves"

  Rule: A combo box

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

    @api-QtQuick.Controls.ComboBox.displayText
    Scenario: A combo box is a combo box named by the text it shows
      Then the object "c" is exposed as a "combobox" named "One"

    Scenario: A combo box takes the text of its new row as its name
      When the property "currentIndex" of "c" is set to 2
      Then the object "c" is exposed as a "combobox" named "Three"

    Scenario: The rows of a closed combo box are not in the tree
      Then the accessibility tree has no "listitem" named "Two"

    @api-QtQuick.Controls.ComboBox.popup
    Scenario: A screen reader's press on a combo box opens its list
      When the user clicks the "combobox" "One"
      Then the property "popup.visible" of "c" is true
      And the accessibility tree has a "list" named ""
      And the "listitem" named "Two" is inside the "list" named ""

    @api-QtQuick.Controls.ItemDelegate
    Scenario: A screen reader's press on a row picks it
      When the user clicks the "combobox" "One"
      And the user clicks the "listitem" "Two"
      Then the property "currentIndex" of "c" is 1
      And the property "popup.visible" of "c" is false
      And the object "c" is exposed as a "combobox" named "Two"

    Scenario: The keyboard stays on the combo box while its list is open
      When the user clicks the "combobox" "One"
      Then the accessibility tree's focus is the "combobox" named "One"
