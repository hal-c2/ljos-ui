# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menubaritem.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menubar.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: MenuBarItem
  An item of a menu bar is a button with the title of a menu. A press on
  it opens the menu under it, and while a menu of the bar is open the
  arrow keys and the pointer go from one menu to the next.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root
        objectName: "root"
        width: 400; height: 400
        property Item first: null
        property Item second: null
        property string log: ""
        Component.onCompleted: { first = bar.itemAt(0); second = bar.itemAt(1) }
        MenuBar {
          id: bar; objectName: "bar"; width: 400
          delegate: MenuBarItem {
            width: 60
            onTriggered: root.log += "triggered "
            onClicked: root.log += "clicked "
          }
          Menu {
            title: "File"
            MenuItem { text: "New"; onTriggered: root.log += "new " }
            MenuItem { text: "Open" }
          }
          Menu { title: "Edit"; MenuItem { text: "Cut" } }
        }
      }
      """

  Rule: An item of the bar is a button for its menu

    @api-QtQuick.Controls.MenuBarItem.menu @api-QtQuick.Controls.MenuBarItem.menuBar
    Scenario Outline: The <property> of an item is <value>
      Then the property "first.<property>" of "root" is <value>

      Examples:
        | property                  | value       |
        | x                         | 0           |
        | y                         | 0           |
        | width                     | 60          |
        | height                    | 40          |
        | implicitHeight            | 40          |
        | padding                   | 6           |
        | leftPadding               | 12          |
        | rightPadding              | 16          |
        | topPadding                | 6           |
        | bottomPadding             | 6           |
        | spacing                   | 6           |
        | focusPolicy               | 0           |
        | checkable                 | false       |
        | display                   | 2           |
        | hoverEnabled              | true        |
        | highlighted               | false       |
        | down                      | false       |
        | text                      | "File"      |
        | menu.title                | "File"      |
        | menu.visible              | false       |
        | menu.parent.text          | "File"      |
        | menu.closePolicy          | 26          |
        | menuBar.objectName        | "bar"       |
        | background.color          | "#00000000" |
        | background.implicitWidth  | 40          |
        | background.implicitHeight | 40          |
        | contentItem.color         | "#26282a"   |

    Scenario: The items stand side by side
      Then the property "second.x" of "root" is 60
      And the property "second.text" of "root" is "Edit"

  Rule: A press on an item opens its menu under it

    @api-QtQuick.Controls.MenuBarItem.triggered
    Scenario Outline: The <property> of an item whose menu is open is <value>
      When the user touches 10, 20
      Then the property "first.<property>" of "root" is <value>

      Examples:
        | property          | value     |
        | menu.visible      | true      |
        | menu.x            | 0         |
        | menu.y            | 40        |
        | menu.activeFocus  | true      |
        | menu.currentIndex | -1        |
        | highlighted       | true      |
        | down              | false     |
        | activeFocus       | false     |
        | background.color  | "#bdbdbd" |

    Scenario: The other items are as they were
      When the user touches 10, 20
      Then the property "second.highlighted" of "root" is false
      And the property "second.menu.visible" of "root" is false

    Scenario: The menu opens when the pointer goes down
      When the user presses the pointer at 10, 20
      Then the property "first.menu.visible" of "root" is true
      And the property "first.down" of "root" is true
      And the property "log" of "root" is "triggered "

    Scenario: An item says that it was triggered, and then that it was clicked
      When the user touches 10, 20
      Then the property "log" of "root" is "triggered clicked "

    Scenario: A press on the item of the open menu closes the menu
      When the user touches 10, 20
      And the user touches 10, 20
      Then the property "first.menu.visible" of "root" is false
      And the property "log" of "root" is "triggered clicked triggered clicked "

    Scenario: The item that closed its menu has the keyboard
      When the user touches 10, 20
      And the user touches 10, 20
      Then the property "first.activeFocus" of "root" is true
      And the property "activeFocus" of "bar" is true

    Scenario: The Escape key closes the menu
      When the user touches 10, 20
      And the user presses "Escape"
      Then the property "first.menu.visible" of "root" is false
      And the property "first.highlighted" of "root" is false
      And the property "first.activeFocus" of "root" is false

    Scenario: A press outside the menu closes it
      When the user touches 10, 20
      And the user touches 300, 300
      Then the property "first.menu.visible" of "root" is false
      And the property "first.highlighted" of "root" is false

    Scenario: An item of the menu that is chosen closes the menu
      When the user touches 10, 20
      And the user touches 20, 60
      Then the property "log" of "root" is "triggered clicked new "
      And the property "first.menu.visible" of "root" is false
      And the property "first.highlighted" of "root" is false

    Scenario: The Down key goes into the menu
      When the user touches 10, 20
      And the user presses "Down"
      Then the property "first.menu.currentIndex" of "root" is 0

    Scenario: The Return key chooses the current item of the menu
      When the user touches 10, 20
      And the user presses "Down"
      And the user presses "Return"
      Then the property "log" of "root" is "triggered clicked new "
      And the property "first.menu.visible" of "root" is false

    Scenario: The Space key opens the menu of the item that has the keyboard
      When the user touches 10, 20
      And the user touches 10, 20
      And the user presses "Space"
      Then the property "first.menu.visible" of "root" is true
      And the property "first.highlighted" of "root" is true

  Rule: The arrow keys go from the open menu to the next

    Scenario: The Right key opens the menu of the next item
      When the user touches 10, 20
      And the user presses "Right"
      Then the property "second.menu.visible" of "root" is true
      And the property "second.highlighted" of "root" is true
      And the property "second.menu.activeFocus" of "root" is true

    Scenario: The menu that was open is closed
      When the user touches 10, 20
      And the user presses "Right"
      Then the property "first.menu.visible" of "root" is false
      And the property "first.highlighted" of "root" is false

    Scenario: The Right key goes from the last menu to the first
      When the user touches 10, 20
      And the user presses "Right"
      And the user presses "Right"
      Then the property "first.menu.visible" of "root" is true
      And the property "second.menu.visible" of "root" is false

    Scenario: The Left key goes from the first menu to the last
      When the user touches 10, 20
      And the user presses "Left"
      Then the property "second.menu.visible" of "root" is true
      And the property "first.menu.visible" of "root" is false

    Scenario: The Left key leaves a menu whose item is the current one
      When the user touches 70, 20
      And the user touches 70, 20
      And the user presses "Space"
      And the user presses "Down"
      And the user presses "Left"
      Then the property "first.menu.visible" of "root" is true
      And the property "second.menu.visible" of "root" is false

    Scenario: The Escape key closes the menu the keys went to
      When the user touches 10, 20
      And the user presses "Right"
      And the user presses "Escape"
      Then the property "second.menu.visible" of "root" is false
      And the property "second.highlighted" of "root" is false
      And the property "first.highlighted" of "root" is false

  Rule: The pointer over an item highlights it

    @api-QtQuick.Controls.MenuBarItem.highlighted
    Scenario: The item under the pointer is highlighted
      When the user moves the pointer to 10, 20
      Then the property "first.highlighted" of "root" is true
      And the property "first.background.color" of "root" is "#bdbdbd"
      And the property "first.menu.visible" of "root" is false

    Scenario: The item the pointer left is not
      When the user moves the pointer to 10, 20
      And the user moves the pointer to 300, 300
      Then the property "first.highlighted" of "root" is false

    Scenario: The pointer over another item moves the open menu there
      When the user touches 10, 20
      And the user moves the pointer to 70, 20
      Then the property "second.menu.visible" of "root" is true
      And the property "second.highlighted" of "root" is true
      And the property "first.menu.visible" of "root" is false
      And the property "first.highlighted" of "root" is false

    Scenario: The item of the open menu stays highlighted when the pointer leaves
      When the user touches 10, 20
      And the user moves the pointer to 70, 20
      And the user moves the pointer to 300, 300
      Then the property "second.menu.visible" of "root" is true
      And the property "second.highlighted" of "root" is true

    Scenario: A press on the item the pointer moved the menu to closes the menu
      When the user touches 10, 20
      And the user moves the pointer to 70, 20
      And the user touches 70, 20
      Then the property "second.menu.visible" of "root" is false
      And the property "first.menu.visible" of "root" is false
      And the property "second.highlighted" of "root" is true
