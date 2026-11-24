# Sources:
#   docs/DIALECT.md (§4.16 the controls)
#   docs/design/color.md, docs/design/motion.md
#   https://doc.qt.io/qt-6/qml-qtquick-controls-scrollbar.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-scrollview.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-pane.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-frame.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-groupbox.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-page.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-swipeview.html
#   https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html
# Ljós's own style is Ljós's: Qt has no Ljos.Controls to compare it with, so
# every scenario is `@ljos-only`. The colours expected are those
# crates/ljos_design/src/roles.rs derives from white, black and #0066cc.

@ljos-only
Feature: Ljós's own style: scroll bars, views and the containers a window is laid out in
  The containers of Ljos.Controls are drawn in Ljós's own style, as its
  buttons and fields are: a pane and a page are the canvas, a frame and a
  group box are a thin edge with the radius, and a scroll bar is a thin
  rounded handle that shows while its view scrolls. Each takes the
  theme's roles, so it follows the three colours in light and dark, and
  each passes the design audit.

  Rule: A scroll bar is a thin handle that shows while its view scrolls

    Background:
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Controls
        Item {
          width: 200; height: 200
          Component.onCompleted: { Theme.canvas = "#ffffff"; Theme.text = "#000000"; Theme.accent = "#0066cc" }
          ScrollBar { objectName: "bar"; x: 180; height: 200; size: 0.5 }
        }
        """

    Scenario: The handle is rounded and in a colour read at 3:1 on the ground
      Then the property "implicitWidth" of "bar" is 10
      And the property "contentItem.width" of "bar" is 6
      And the property "contentItem.radius" of "bar" is 3
      And the property "contentItem.color" of "bar" is "#707070"

    Scenario: The handle shows while the bar is in use and fades a while after
      Then the property "contentItem.opacity" of "bar" is 0
      When the property "active" of "bar" is set to true
      Then the property "contentItem.opacity" of "bar" is 1
      When the property "active" of "bar" is set to false
      And 400 milliseconds pass
      Then the property "contentItem.opacity" of "bar" is 1
      When 600 milliseconds pass
      Then the property "contentItem.opacity" of "bar" is 0

    Scenario: Under the pointer the bar shows its track and the handle darkens
      When the property "active" of "bar" is set to true
      And the user moves the pointer to 185, 50
      Then the property "background.visible" of "bar" is true
      And the property "background.color" of "bar" is "#ededed"
      And the property "contentItem.color" of "bar" is "#575757"

    Scenario: A bar that takes no pointer is thinner and shows no track
      When the property "interactive" of "bar" is set to false
      Then the property "implicitWidth" of "bar" is 6
      When the property "active" of "bar" is set to true
      And the user moves the pointer to 182, 50
      Then the property "background.visible" of "bar" is false

  Rule: A scroll view scrolls with the style's bars

    Scenario: Turning the wheel over a view shows its bar
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Layout
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          width: 300; height: 200
          Component.onCompleted: { Theme.canvas = "#ffffff"; Theme.text = "#000000"; Theme.accent = "#0066cc" }
          ScrollView {
            id: view
            objectName: "view"
            width: 200; height: 100
            ScrollBar.vertical: ScrollBar { objectName: "bar" }
            Column {
              Repeater {
                model: 20
                Label { required property int index; text: "Line " + index }
              }
            }
          }
        }
        """
      Then the property "contentItem.opacity" of "bar" is 0
      And the property "contentItem.color" of "bar" is "#707070"
      When the user turns the wheel 1 notch down at 100, 50
      Then the property "contentItem.opacity" of "bar" is 1

  Rule: A pane and a page are the canvas; a frame and a group box are a thin edge

    Background:
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          width: 480; height: 400
          readonly property real frameGap: group.background.y - group.implicitLabelHeight
          readonly property real insetGap: group.topPadding - group.background.y
          Component.onCompleted: { Theme.canvas = "#ffffff"; Theme.text = "#000000"; Theme.accent = "#0066cc" }
          Pane { objectName: "pane"; Label { text: "On the pane" } }
          Frame { objectName: "frame"; y: 60; Label { text: "In a frame" } }
          GroupBox {
            id: group
            objectName: "group"; y: 120; title: "Sync"
            Label { text: "Every hour" }
          }
          Page {
            objectName: "page"; y: 240; width: 200; height: 120
            header: Label { text: "Settings" }
          }
          Control { objectName: "control"; x: 300 }
        }
        """

    Scenario Outline: The <part> has the theme's look
      Then the property "<property>" of "<part>" is <value>

      Examples:
        | part    | property            | value         |
        | pane    | background.color    | "#ffffff"     |
        | pane    | padding             | 12            |
        | pane    | font.pixelSize      | 13            |
        | frame   | background.color    | "#00000000"   |
        | frame   | background.border.color | "#d6d6d6" |
        | frame   | background.radius   | 8             |
        | frame   | padding             | 12            |
        | group   | background.border.color | "#d6d6d6" |
        | group   | background.radius   | 8             |
        | page    | background.color    | "#ffffff"     |
        | page    | font.pixelSize      | 13            |
        | control | font.pixelSize      | 13            |

    Scenario: A group box's title is heavier and starts where its content does
      Then the property "label.font.weight" of "group" is 600
      And the property "label.color" of "group" is "#000000"
      And the property "label.x" of "group" is 12
      And the property "contentItem.x" of "group" is 12

    Scenario: A group box's frame starts below its title and the content inside the frame
      Then the property "frameGap" of "root" is 8
      And the property "insetGap" of "root" is 12

  Rule: An application window's ground is the canvas

    Scenario: The window is the canvas and its controls take the theme's type
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Controls
        ApplicationWindow {
          objectName: "window"
          width: 320; height: 200
          Component.onCompleted: { Theme.canvas = "#ffffff"; Theme.text = "#000000"; Theme.accent = "#0066cc" }
          Label { text: "Hello" }
        }
        """
      Then the property "color" of "window" is "#ffffff"
      And the property "font.pixelSize" of "window" is 13

  Rule: A swipe view moves to its page, and with reduced motion fades it in

    Background:
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          width: 200; height: 100
          SwipeView {
            id: view
            objectName: "view"
            width: 200; height: 100
            Label { text: "First" }
            Label { text: "Second" }
          }
        }
        """

    Scenario: The view slides to the page it is told to show
      When the property "currentIndex" of "view" is set to 1
      And 100 milliseconds pass
      Then the property "contentItem.contentX" of "view" is at least 1
      And the property "contentItem.contentX" of "view" is at most 199
      When 400 milliseconds pass
      Then the property "contentItem.contentX" of "view" is 200
      And the property "contentItem.opacity" of "view" is 1

    Scenario: With reduced motion it is on the page at once and the page fades in
      Given the system setting "reduced motion" is true
      When the property "currentIndex" of "view" is set to 1
      Then the property "contentItem.contentX" of "view" is 200
      And the property "contentItem.opacity" of "view" is at most 0.99
      When 200 milliseconds pass
      Then the property "contentItem.opacity" of "view" is 1

  Rule: Every one of them passes the design audit

    Scenario Outline: A page of containers in <scheme> has nothing to report
      Given the system setting "color scheme" is "<scheme>"
      And the QML:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        import Ljos.Controls
        Page {
          width: 400; height: 400
          title: "Settings"
          Pane {
            x: 8; y: 8; width: 384; height: 380
            Column { spacing: 12
              GroupBox {
                title: "Sync"
                Column {
                  CheckBox { text: "Every hour" }
                  CheckBox { text: "On battery"; checked: true }
                }
              }
              Frame { Label { text: "Two files changed" } }
              ScrollView {
                width: 240; height: 80
                Column { Repeater { model: 10; Label { required property int index; text: "Line " + index } } }
              }
            }
          }
        }
        """
      Then the design audit finds nothing

      Examples:
        | scheme |
        | light  |
        | dark   |
