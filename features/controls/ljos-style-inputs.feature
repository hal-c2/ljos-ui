# Sources:
#   docs/DIALECT.md (§4.16 the controls)
#   docs/design/color.md, docs/design/motion.md
#   https://doc.qt.io/qt-6/qml-qtquick-controls-dialog.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-dialogbuttonbox.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-textarea.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-itemdelegate.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-radiobutton.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-toolbar.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-toolbutton.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-roundbutton.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-delaybutton.html
#   https://www.w3.org/WAI/ARIA/apg/patterns/radio/
# Ljós's own style is Ljós's: Qt has no Ljos.Controls to compare it with, so
# every scenario is `@ljos-only`. The colours expected are those
# crates/ljos_design/src/roles.rs derives from white, black and #0066cc.

@ljos-only
Feature: Ljós's own style: dialogs, text areas, rows, choices and the buttons of a tool bar
  The rest of what Basic draws, Ljos.Controls now draws in its own style:
  a dialog is a raised surface as a popup is, with the button that
  accepts it filled; a text area is drawn as a text field is; a row of a
  list, a radio button and the buttons of a tool bar take the theme's
  roles as the style's button and check box do. Each says its state by
  more than its colour, and each passes the design audit in light and
  dark.

  Rule: A dialog is a raised surface with its way forward filled

    Background:
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Controls
        Item {
          width: 480; height: 320
          Component.onCompleted: { Theme.canvas = "#ffffff"; Theme.text = "#000000"; Theme.accent = "#0066cc" }
          Dialog {
            id: dialog
            objectName: "dialog"
            title: "Delete the draft?"
            modal: true
            standardButtons: Dialog.Ok | Dialog.Cancel
            Label { text: "It cannot be brought back." }
          }
          Button { objectName: "ask"; text: "Ask"; onClicked: dialog.open() }
        }
        """

    Scenario: The dialog is on the overlay's surface with its title heavier
      When the user clicks the "button" "Ask"
      And 200 milliseconds pass
      Then the property "visible" of "dialog" is true
      And the property "background.children.1.color" of "dialog" is "#e6e6e6"
      And the property "background.children.1.radius" of "dialog" is 12
      And the property "header.font.weight" of "dialog" is 600

    Scenario: The button that accepts it is filled and the other is not
      When the user clicks the "button" "Ask"
      And 200 milliseconds pass
      Then the property "footer.contentChildren.0.text" of "dialog" is "OK"
      And the property "footer.contentChildren.0.primary" of "dialog" is true
      And the property "footer.contentChildren.1.text" of "dialog" is "Cancel"
      And the property "footer.contentChildren.1.primary" of "dialog" is false

    Scenario: Escape closes it and the accept button accepts it
      When the user clicks the "button" "Ask"
      And 200 milliseconds pass
      And the user presses "Escape"
      Then the property "visible" of "dialog" is false
      When the user clicks the "button" "Ask"
      And 200 milliseconds pass
      And the user clicks the "button" "OK"
      Then the property "visible" of "dialog" is false
      And the property "result" of "dialog" is 1

  Rule: A text area is drawn as a field is, and the view it scrolls in keeps the edge

    Background:
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          width: 480; height: 320
          Component.onCompleted: { Theme.canvas = "#ffffff"; Theme.text = "#000000"; Theme.accent = "#0066cc" }
          TextArea { objectName: "notes"; width: 200; placeholderText: "Notes" }
          ScrollView {
            id: view
            y: 120; width: 200; height: 80
            TextArea { id: scrolled; objectName: "scrolled"; text: "One\nTwo\nThree\nFour\nFive" }
          }
          readonly property bool scrolledEdge: scrolled.background.visible
        }
        """

    Scenario: Alone it is a field of many lines in the input's colour
      Then the property "background.visible" of "notes" is true
      And the property "background.color" of "notes" is "#f5f5f5"
      And the property "background.border.color" of "notes" is "#808080"
      And the property "background.radius" of "notes" is 8
      And the property "placeholderTextColor" of "notes" is "#6a6a6a"
      And the property "font.pixelSize" of "notes" is 13

    Scenario: With the keys its edge is the focus colour and twice as wide
      When the method "forceActiveFocus" of "notes" is called
      Then the property "background.border.color" of "notes" is "#0066cc"
      And the property "background.border.width" of "notes" is 2

    Scenario: In a scroll view it draws no edge that would scroll away
      Then the property "scrolledEdge" of "root" is false

  Rule: A row of a list is on the accent's surface where it is pointed at, and ringed where it is current

    Background:
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Controls
        Item {
          width: 240; height: 200
          Component.onCompleted: { Theme.canvas = "#ffffff"; Theme.text = "#000000"; Theme.accent = "#0066cc" }
          ItemDelegate { objectName: "row"; width: 240; text: "Inbox"; iconName: "search" }
          ItemDelegate { objectName: "current"; y: 40; width: 240; text: "Drafts"; highlighted: true }
        }
        """

    Scenario: A row is as tall as a control with room to breathe and its icon is muted
      Then the property "implicitHeight" of "row" is 36
      And the property "background.color" of "row" is "#00000000"
      And the property "contentItem.children.0.visible" of "row" is true

    Scenario: Under the pointer a row is on the accent's surface
      When the user moves the pointer to 100, 18
      Then the property "background.color" of "row" is "#bdcfe1"
      And the property "background.border.width" of "row" is 0

    Scenario: The current row has a ring too, so it is not its fill alone
      Then the property "background.color" of "current" is "#bdcfe1"
      And the property "background.border.width" of "current" is 2
      And the property "background.border.color" of "current" is "#0066cc"

  Rule: A radio button is a circle that fills with the accent when chosen

    Background:
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Layout
        import Ljos.Controls
        Item {
          width: 240; height: 120
          Component.onCompleted: { Theme.canvas = "#ffffff"; Theme.text = "#000000"; Theme.accent = "#0066cc" }
          Column {
            RadioButton { objectName: "daily"; text: "Daily"; checked: true }
            RadioButton { objectName: "weekly"; text: "Weekly" }
          }
        }
        """

    Scenario: The chosen one is filled and dotted; the other is an edge in the input's colour
      Then the property "indicator.color" of "daily" is "#0066cc"
      And the property "indicator.radius" of "daily" is 8
      And the property "indicator.border.color" of "weekly" is "#808080"
      And the property "indicator.color" of "weekly" is "#f5f5f5"

    Scenario: Choosing another lets go of the first
      When the user clicks the "radiobutton" "Weekly"
      Then the property "checked" of "weekly" is true
      And the property "checked" of "daily" is false
      And the property "indicator.color" of "weekly" is "#0066cc"

  Rule: A tool bar is the surface with an edge where it meets the page, and its buttons are flat until used

    Background:
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Layout
        import Ljos.Controls
        Item {
          width: 320; height: 200
          Component.onCompleted: { Theme.canvas = "#ffffff"; Theme.text = "#000000"; Theme.accent = "#0066cc" }
          ToolBar {
            objectName: "bar"
            width: 320
            Row {
              ToolButton { objectName: "find"; iconName: "search"; Accessible.name: "Find" }
              ToolButton { objectName: "pin"; text: "Pin"; checkable: true }
            }
          }
          RoundButton { objectName: "add"; y: 60; iconName: "plus"; highlighted: true; Accessible.name: "Add" }
        }
        """

    Scenario: The bar is the surface with its edge underneath
      Then the property "background.color" of "bar" is "#f5f5f5"
      And the property "background.children.0.color" of "bar" is "#d6d6d6"
      And the property "implicitHeight" of "bar" is 36

    Scenario: A tool button is flat until the pointer is over it
      Then the property "background.color" of "find" is "#00000000"
      When the user moves the pointer to 18, 18
      Then the property "background.color" of "find" is "#bdcfe1"

    Scenario: A checked tool button has a ring, so its state is not its fill alone
      When the user clicks the "button" "Pin"
      Then the property "checked" of "pin" is true
      And the property "background.border.width" of "pin" is 2

    Scenario: A highlighted round button is round and filled with the accent
      Then the property "background.radius" of "add" is 14
      And the property "background.color" of "add" is "#0066cc"

  Rule: A delay button acts once held, and shows how long is left along its foot

    Background:
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Controls
        Item {
          width: 240; height: 120
          Component.onCompleted: { Theme.canvas = "#ffffff"; Theme.text = "#000000"; Theme.accent = "#0066cc" }
          DelayButton { objectName: "wipe"; text: "Hold to wipe"; delay: 600 }
        }
        """

    Scenario: Held, a bar fills along its foot; held to the end it acts and is filled
      When the user presses the pointer at 40, 14
      And 300 milliseconds pass
      Then the property "progress" of "wipe" is within 0.15 of 0.5
      And the property "background.children.0.visible" of "wipe" is true
      When 400 milliseconds pass
      And the user releases the pointer at 40, 14
      Then the property "checked" of "wipe" is true
      And the property "background.color" of "wipe" is "#0066cc"

    Scenario: With reduced motion the bar still says how long is left
      Given the system setting "reduced motion" is true
      When the user presses the pointer at 40, 14
      And 300 milliseconds pass
      Then the property "progress" of "wipe" is within 0.15 of 0.5

  Rule: Every one of them passes the design audit

    Scenario Outline: A form of them in <scheme> has nothing to report
      Given the system setting "color scheme" is "<scheme>"
      And the QML:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        import Ljos.Controls
        Page {
          width: 400; height: 400
          title: "Compose"
          header: ToolBar {
            Row {
              ToolButton { iconName: "search"; Accessible.name: "Find" }
              ToolButton { text: "Pin"; checkable: true; checked: true }
            }
          }
          Column {
            x: 12; y: 12; spacing: 8
            TextArea { width: 360; placeholderText: "Notes"; Accessible.name: "Notes" }
            ItemDelegate { width: 360; text: "Inbox" }
            ItemDelegate { width: 360; text: "Drafts"; highlighted: true }
            RadioButton { text: "Daily"; checked: true }
            RadioButton { text: "Weekly" }
            Row { spacing: 8
              RoundButton { iconName: "plus"; highlighted: true; Accessible.name: "Add" }
              DelayButton { text: "Hold to wipe" }
            }
          }
        }
        """
      Then the design audit finds nothing

      Examples:
        | scheme |
        | light  |
        | dark   |
