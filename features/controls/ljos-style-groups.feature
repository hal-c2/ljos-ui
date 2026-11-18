# Sources:
#   docs/DIALECT.md (§4.16 the controls)
#   docs/site/controls/overview.md (Ljos.Controls.Basic has the same controls)
#   https://www.w3.org/WAI/ARIA/apg/patterns/tabs/
#   https://www.w3.org/WAI/WCAG22/Understanding/focus-appearance.html
# Ljós's own style is Ljós's: Qt has no Ljos.Controls to compare it with, so
# every scenario is `@ljos-only`. The colours expected are those
# crates/ljos_design/src/roles.rs derives from white, black and #0066cc.

@ljos-only
Feature: Ljós's own style: tabs, cards, popovers, split buttons and empty states
  Beside the single controls, Ljos.Controls has the parts an app is laid
  out with: a strip of tabs, a card that groups things on the surface, a
  popover that opens beside what it belongs to, a split button that pairs
  an action with a menu of its other options, and an empty state for a
  view with nothing to show yet. They are drawn with the same Theme as the
  other controls, and Ljos.Controls.Basic has each of them under the same
  name, drawn as the Basic style would, so a file moves between the two by
  its import alone.

  Rule: A tab is as wide as what it says, and the current one is raised and heavier

    Scenario: A strip of two tabs
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Controls
        Item {
          width: 400; height: 120
          Component.onCompleted: {
            Theme.canvas = "#ffffff"
            Theme.text = "#000000"
            Theme.accent = "#0066cc"
          }
          TabBar {
            objectName: "bar"; width: 400
            TabButton { objectName: "threads"; text: "Threads" }
            TabButton { objectName: "files"; text: "Files" }
          }
        }
        """
      Then the property "height" of "bar" is 36
      And the property "height" of "threads" is 28
      And the property "x" of "threads" is 0
      And the property "width" of "threads" is at most 80
      And the property "width" of "files" is at most 60
      And the property "background.radius" of "threads" is 6
      When 200 milliseconds pass
      Then the property "background.color" of "threads" is "#ededed"
      And the property "background.color" of "files" is "#00000000"
      And the property "font.pixelSize" of "files" is 12
      When the user clicks the "tab" "Files"
      And 200 milliseconds pass
      Then the property "currentIndex" of "bar" is 1
      And the property "background.color" of "files" is "#ededed"

    Scenario: A key that brings the focus to a tab shows its ring
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Controls
        Item {
          width: 400; height: 120
          Component.onCompleted: {
            Theme.canvas = "#ffffff"
            Theme.text = "#000000"
            Theme.accent = "#0066cc"
          }
          TabBar {
            TabButton { objectName: "threads"; text: "Threads"; focusPolicy: FocusPolicy.Strong }
          }
        }
        """
      Then the property "visible" of "focusRing" is false
      When the user presses "Tab"
      Then the property "visualFocus" of "threads" is true
      And the property "visible" of "focusRing" is true
      And the property "border.color" of "focusRing" is "#0066cc"

    Scenario: Tabs that do not fit end at the strip's edge
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls
        Item {
          width: 400; height: 120
          TabBar {
            objectName: "bar"; width: 120
            TabButton { text: "Threads" }
            TabButton { text: "Files" }
            TabButton { text: "Settings" }
          }
        }
        """
      Then the property "clip" of "bar" is true
      And the property "width" of "bar" is 120

  Rule: A card groups things on the surface

    Scenario: What is put in a card is inside its padding
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Controls
        Item {
          width: 320; height: 200
          Component.onCompleted: {
            Theme.canvas = "#ffffff"
            Theme.text = "#000000"
            Theme.accent = "#0066cc"
          }
          Card {
            objectName: "card"
            Label { objectName: "body"; text: "Two files changed" }
          }
        }
        """
      Then the property "padding" of "card" is 12
      And the property "background.radius" of "card" is 12
      And the property "background.color" of "card" is "#f5f5f5"
      And the property "background.border.width" of "card" is 1
      And the property "x" of "body" is 0
      And the property "contentItem.x" of "card" is 12

  Rule: A popover opens beside what it belongs to

    Scenario Outline: A popover on the <edge> edge of its button opens <where>
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          width: 400; height: 400
          Button {
            id: anchor; x: <x>; y: <y>; width: 80; text: "Filter"
            onClicked: pop.open()
            Popover {
              id: pop; objectName: "pop"; edge: <edge>
              width: 120; height: 60
            }
          }
          property real popX: pop.x
          property real popY: pop.y
        }
        """
      When the user clicks the "button" "Filter"
      And 200 milliseconds pass
      Then the property "visible" of "pop" is true
      And the property "popX" of "root" is <px>
      And the property "popY" of "root" is <py>

      # The window is 400 by 400, as the root is.
      Examples:
        | edge        | x  | y   | where                                      | px | py  |
        | Edge.Bottom | 20 | 20  | below it                                   | 0  | 32  |
        | Edge.Top    | 20 | 200 | above it                                   | 0  | -64 |
        | Edge.Right  | 20 | 20  | after it                                   | 84 | 0   |
        | Edge.Bottom | 20 | 360 | above it, as the window has no room below  | 0  | -64 |

    # The room on each side, and the gap, are the window's, wherever the
    # button is drawn and whatever its ancestors' scale; the margins the
    # popover keeps from the window's edges are not room.
    @api-QtQuick.Controls.Popup.margins
    Scenario Outline: A popover <how> opens <where>
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          width: 400; height: 400
          Item {
            width: 200; height: 200; scale: <scale>; transformOrigin: Item.TopLeft
            Button {
              x: <x>; y: <y>; width: 60; height: 20; text: "Filter"
              Popover { id: pop; objectName: "pop"; width: 100; height: <height>; margins: <margins> }
            }
          }
          property real popX: pop.x
          property real popY: pop.y
        }
        """
      When the method "open" of "pop" is called
      And 200 milliseconds pass
      Then the property "popX" of "root" is <px>
      And the property "popY" of "root" is <py>

      # The window is 400 by 400, as the root is. Scaled, the button is at
      # 300 to 340 in the window, and the popover 4 above it is at -52 in
      # the button's terms.
      Examples:
        | how                     | scale | x   | y   | height | margins | where                                       | px | py   |
        | under a scaled ancestor | 2     | 10  | 150 | 100    | 8       | above it, as the window has 56 below        | 0  | -52  |
        | with margins            | 1     | 200 | 250 | 120    | 12      | above it, as 114 is left below its margins  | 0  | -124 |
        | with room below         | 1     | 200 | 20  | 100    | 0       | below it                                    | 0  | 24   |

    Scenario: A popover fades in on the overlay surface and closes at once
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          width: 400; height: 300
          Component.onCompleted: {
            Theme.canvas = "#ffffff"
            Theme.text = "#000000"
            Theme.accent = "#0066cc"
          }
          Button {
            x: 20; y: 20; text: "Filter"
            onClicked: pop.visible ? pop.close() : pop.open()
            Popover { id: pop; objectName: "pop"; Label { text: "Only open" } }
          }
          property real fade: pop.opacity
          function hide() { pop.close() }
        }
        """
      When the user clicks the "button" "Filter"
      Then the property "fade" of "root" is within 0.15 of 0
      When 200 milliseconds pass
      Then the property "fade" of "root" is 1
      And the property "dim" of "pop" is false
      When the method "hide" of "root" is called
      Then the property "visible" of "pop" is false

    Scenario: Escape closes a popover that holds only text
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls
        Item {
          width: 400; height: 300
          Button {
            x: 20; y: 20; text: "Filter"
            onClicked: pop.open()
            Popover { id: pop; objectName: "pop"; Label { text: "Only open" } }
          }
        }
        """
      When the user clicks the "button" "Filter"
      And 200 milliseconds pass
      Then the property "visible" of "pop" is true
      When the user presses "Escape"
      Then the property "visible" of "pop" is false

  Rule: A split button pairs an action with a menu of its other options

    Scenario: The action's half runs it and the chevron's opens the menu below
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          width: 400; height: 300
          property string said: ""
          SplitButton {
            id: split; objectName: "split"; x: 20; y: 20
            text: "Run"
            onClicked: root.said += "run "
            onMenuRequested: root.said += "menu "
            menu: Menu {
              objectName: "options"
              MenuItem { text: "Run all" }
            }
          }
        }
        """
      Then the property "height" of "split" is 28
      And the accessibility tree has a "button" named "More Run options"
      When the user clicks the "button" "Run"
      Then the property "said" of "root" is "run "
      When the user clicks the "button" "More Run options"
      And 200 milliseconds pass
      Then the property "said" of "root" is "run menu "
      And the property "visible" of "options" is true
      And the property "y" of "options" is 32

    Scenario: A split button made wider keeps its menu under the chevron and is all action up to the seam
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          width: 400; height: 300
          property real menuEnd: options.x + options.width
          SplitButton {
            objectName: "split"; x: 20; y: 20
            text: "Run"
            menu: Menu {
              id: options; objectName: "options"
              MenuItem { text: "Run all" }
            }
          }
        }
        """
      When the user clicks the "button" "More Run options"
      And 200 milliseconds pass
      And the property "width" of "split" is set to 240
      Then the property "menuEnd" of "root" is 240
      And the property "width" of "action" is at least 200

    Scenario: A menu taken away while open stays under the chevron
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          width: 400; height: 300
          property real menuEnd: options.x + options.width
          SplitButton {
            objectName: "split"; x: 20; y: 20
            text: "Run"
            menu: Menu {
              id: options; objectName: "options"
              MenuItem { text: "Run all" }
            }
          }
        }
        """
      When the user clicks the "button" "More Run options"
      And 200 milliseconds pass
      And the property "menu" of "split" is set to null
      And the property "width" of "split" is set to 240
      Then the property "menuEnd" of "root" is 240

    Scenario: A compact split button shows its icon alone and is still named
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls
        Item {
          width: 400; height: 300
          SplitButton { objectName: "split"; x: 20; y: 20; text: "Add"; iconName: "plus"; compact: true }
        }
        """
      Then the accessibility tree has a "button" named "Add"
      And the property "width" of "split" is at most 64

  Rule: An empty state says why a view is empty and what to do

    Scenario: The main action is filled and each says it was chosen
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          width: 480; height: 320
          Component.onCompleted: {
            Theme.canvas = "#ffffff"
            Theme.text = "#000000"
            Theme.accent = "#0066cc"
          }
          property string said: ""
          EmptyState {
            objectName: "empty"
            title: "No threads yet"
            description: "Start one to talk to an agent."
            actionText: "New thread"
            secondaryActionText: "Import"
            onActionTriggered: root.said += "new "
            onSecondaryActionTriggered: root.said += "import "
          }
        }
        """
      Then the property "background.color" of "action" is "#0066cc"
      And the property "color" of "tile" is "#ffffff"
      When the user clicks the "button" "New thread"
      And the user clicks the "button" "Import"
      Then the property "said" of "root" is "new import "

    @api-QtQuick.Controls.Control.availableWidth @api-QtQuick.Item.implicitWidth
    Scenario Outline: The text of an empty state <how> <fits>
      Given the QML:
        """qml
        import QtQuick
        import <module>
        Item {
          id: root; objectName: "root"
          width: 480; height: 320
          EmptyState {
            id: empty; objectName: "empty"; <width>
            title: "A title that is a good deal wider than two hundred pixels"
            description: "A description that is a good deal wider than two hundred pixels, and wraps"
          }
          function part(item, name) {
            for (const child of item.children) {
              if (child.objectName === name) return child
              const found = part(child, name)
              if (found) return found
            }
            return null
          }
          property var got: []
          function measure() {
            got = [part(empty.contentItem, "title").width - empty.availableWidth,
                   part(empty.contentItem, "description").width - empty.availableWidth,
                   empty.implicitWidth]
          }
        }
        """
      When the method "measure" of "root" is called
      Then the property "got" of "root" is <got>

      # 360 is the width an empty state's text is kept to; Basic pads it by 12.
      Examples:
        | module               | how                         | width       | fits                                                  | got           |
        | Ljos.Controls        | narrower than its text      | width: 200  | is as wide as it, and it asks for its text's width     | [0, 0, 360]   |
        | Ljos.Controls        | as wide as it asks          |             | is as wide as it asks                                  | [0, 0, 360]   |
        | Ljos.Controls.Basic  | of Basic narrower than its text | width: 200 | is as wide as it, and it asks for its text's width  | [0, 0, 384]   |
        | Ljos.Controls.Basic  | of Basic as wide as it asks |             | is as wide as it asks                                  | [0, 0, 384]   |

    Scenario: An error shows a warning, in the warning colour
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Controls
        Item {
          width: 480; height: 320
          Component.onCompleted: {
            Theme.canvas = "#ffffff"
            Theme.text = "#000000"
            Theme.accent = "#0066cc"
          }
          EmptyState { objectName: "empty"; status: "error"; title: "Could not load" }
        }
        """
      Then the property "name" of "glyph" is "circle-alert"
      And the property "color" of "glyph" is not "#000000"

    Scenario: While loading the icon turns
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls
        Item {
          width: 480; height: 320
          EmptyState { objectName: "empty"; status: "loading"; title: "Loading threads" }
        }
        """
      When 400 milliseconds pass
      Then the property "name" of "glyph" is "circle-dashed"
      And the property "rotation" of "glyph" is at least 30

    Scenario: With reduced motion the icon pulses instead of turning
      Given the system setting "reduced motion" is true
      And the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls
        Item {
          width: 480; height: 320
          EmptyState { objectName: "empty"; status: "loading"; title: "Loading threads" }
        }
        """
      When 400 milliseconds pass
      Then the property "rotation" of "glyph" is 0
      And the property "opacity" of "glyph" is at most 0.9

    Scenario: When loading ends the icon stands upright again
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls
        Item {
          width: 480; height: 320
          EmptyState { objectName: "empty"; status: "loading"; title: "Loading threads" }
        }
        """
      When 400 milliseconds pass
      And the property "status" of "empty" is set to "error"
      Then the property "name" of "glyph" is "circle-alert"
      And the property "rotation" of "glyph" is 0
      And the property "opacity" of "glyph" is 1

    Scenario: With reduced motion, when loading ends the icon is whole again
      Given the system setting "reduced motion" is true
      And the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls
        Item {
          width: 480; height: 320
          EmptyState { objectName: "empty"; status: "loading"; title: "Loading threads" }
        }
        """
      When 400 milliseconds pass
      And the property "status" of "empty" is set to "error"
      Then the property "rotation" of "glyph" is 0
      And the property "opacity" of "glyph" is 1

  Rule: Every one of them passes the design audit

    Scenario Outline: Tabs, a card, a split button and an empty state in <scheme> have nothing to report
      Given the system setting "color scheme" is "<scheme>"
      And the QML:
        """qml
        import QtQuick
        import Ljos.Controls
        Rectangle {
          width: 560; height: 420; color: Theme.canvas
          Column {
            x: 16; y: 16; spacing: 12
            TabBar {
              TabButton { text: "Threads" }
              TabButton { text: "Files" }
            }
            Card {
              Label { text: "Two files changed" }
            }
            SplitButton { text: "Run"; iconName: "chevron-right" }
            EmptyState {
              title: "No threads yet"
              description: "Start one to talk to an agent."
              actionText: "New thread"
              secondaryActionText: "Import"
            }
          }
        }
        """
      Then the design audit finds nothing

      Examples:
        | scheme |
        | light  |
        | dark   |

    Scenario Outline: A popover that has faded in, in <scheme>, has nothing to report
      Given the system setting "color scheme" is "<scheme>"
      And the QML:
        """qml
        import QtQuick
        import Ljos.Controls
        Rectangle {
          width: 480; height: 400; color: Theme.canvas
          Button {
            x: 16; y: 16; text: "Filter"
            Popover {
              // A popover is a dialog to a reader, so it is named.
              Accessible.name: "Filters"
              Component.onCompleted: open()
              Column {
                spacing: 8
                CheckBox { text: "Only open" }
                CheckBox { text: "Mine"; checked: true }
              }
            }
          }
        }
        """
      When 200 milliseconds pass
      Then the design audit finds nothing

      Examples:
        | scheme |
        | light  |
        | dark   |

  Rule: Ljos.Controls.Basic has each of them, drawn as the Basic style would

    Scenario: A file moves to Basic's look by its import alone
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        import Ljos.Controls.Basic
        Item {
          id: root; objectName: "root"
          width: 480; height: 400
          property string said: ""
          Column {
            TabBar { objectName: "bar"; TabButton { objectName: "threads"; text: "Threads"; iconName: "folder" } TabButton { text: "Files" } }
            Card { objectName: "card"; Label { text: "Two files changed" } }
            SplitButton {
              objectName: "split"; text: "Run"; iconName: "chevron-right"
              onClicked: root.said += "run "; onMenuRequested: root.said += "menu "
            }
            EmptyState {
              objectName: "empty"; status: "loading"; title: "No threads yet"
              actionText: "New thread"; onActionTriggered: root.said += "new "
            }
            Button { text: "Filter"; Popover { objectName: "pop"; edge: Edge.Right } }
          }
        }
        """
      Then the property "background.radius" of "card" is 0
      And the property "padding" of "card" is 12
      And the property "background.radius" of "pop" is 0
      And the property "edge" of "pop" is 4
      And the property "iconName" of "threads" is "folder"
      When the user clicks the "button" "Run"
      And the user clicks the "button" "More Run options"
      And the user clicks the "button" "New thread"
      Then the property "said" of "root" is "run menu new "

    Scenario: An error with no title says so in words, as Basic has no icons
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls.Basic
        Item {
          width: 480; height: 320
          EmptyState { objectName: "empty"; status: "error" }
        }
        """
      Then the property "text" of "title" is "Something went wrong"
