# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-tooltip.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: ToolTip
  A tool tip is a small popup that says what something is. It can wait before
  it shows and go away by itself, and every item can use one that all of them
  share through the attached ToolTip properties.

  Rule: A tool tip of its own

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
          Item {
            x: 100; y: 150; width: 200; height: 100
            ToolTip { objectName: "plain"; text: "Hello" }
            ToolTip {
              objectName: "tip"
              contentItem: Item { implicitWidth: 60; implicitHeight: 20 }
              onOpened: root.log += "opened "
              onClosed: root.log += "closed "
            }
          }
        }
        """

    @api-QtQuick.Controls.ToolTip
    Scenario Outline: A tool tip starts with <property> <value>
      Then the property "<property>" of "plain" is <value>

      Examples:
        | property                | value     |
        | text                    | "Hello"   |
        | delay                   | 0         |
        | timeout                 | -1        |
        | visible                 | false     |
        | padding                 | 6         |
        | margins                 | 6         |
        | closePolicy             | 26        |
        | focus                   | false     |
        | modal                   | false     |
        | contentItem.text        | "Hello"   |
        | contentItem.color       | "#000000" |
        | contentItem.wrapMode    | 4         |
        | background.color        | "#ffffff" |
        | background.border.color | "#353637" |
        | background.border.width | 1         |

    Scenario Outline: An open tool tip sits above the middle of its parent with <property> <value>
      When the method "open" of "tip" is called
      Then the property "<property>" of "tip" is <value>

      Examples:
        | property       | value |
        | visible        | true  |
        | implicitWidth  | 72    |
        | implicitHeight | 32    |
        | width          | 72    |
        | height         | 32    |
        | x              | 64    |
        | y              | -35   |

    @api-QtQuick.Controls.ToolTip.text
    Scenario: The text of a tool tip is what its content shows
      When the property "text" of "plain" is set to "Other"
      Then the property "contentItem.text" of "plain" is "Other"

    @api-QtQuick.Controls.ToolTip.show
    Scenario: show gives the tool tip a text and opens it
      When the method "show" of "tip" is called with "Now"
      Then the property "visible" of "tip" is true
      And the property "text" of "tip" is "Now"
      And the property "timeout" of "tip" is -1

    Scenario: show can say how long the tool tip stays
      When the method "show" of "tip" is called with "Now", 200
      Then the property "timeout" of "tip" is 200
      And the property "visible" of "tip" is true
      When 500 milliseconds pass
      Then the property "visible" of "tip" is false
      And the property "log" of "root" is "opened closed "

    Scenario: show without a time leaves the timeout as it was
      When the property "timeout" of "tip" is set to 5000
      And the method "show" of "tip" is called with "Now"
      Then the property "timeout" of "tip" is 5000

    @api-QtQuick.Controls.ToolTip.hide
    Scenario: hide closes the tool tip
      When the method "show" of "tip" is called with "Now"
      And the method "hide" of "tip" is called
      Then the property "visible" of "tip" is false
      And the property "log" of "root" is "opened closed "

    @api-QtQuick.Controls.ToolTip.timeout
    Scenario: A tool tip with a timeout closes by itself
      When the property "timeout" of "tip" is set to 200
      And the method "open" of "tip" is called
      And 50 milliseconds pass
      Then the property "visible" of "tip" is true
      When 500 milliseconds pass
      Then the property "visible" of "tip" is false
      And the property "log" of "root" is "opened closed "

    Scenario: A tool tip without a timeout stays
      When the method "open" of "tip" is called
      And 1 second passes
      Then the property "visible" of "tip" is true

    Scenario: A timeout of nothing is no timeout
      When the property "timeout" of "tip" is set to 0
      And the method "open" of "tip" is called
      And 500 milliseconds pass
      Then the property "visible" of "tip" is true

    Scenario: A timeout given to an open tool tip counts from then
      When the method "open" of "tip" is called
      And 300 milliseconds pass
      And the property "timeout" of "tip" is set to 200
      And 50 milliseconds pass
      Then the property "visible" of "tip" is true
      When 500 milliseconds pass
      Then the property "visible" of "tip" is false

    Scenario: A timeout taken from an open tool tip lets it stay
      When the property "timeout" of "tip" is set to 200
      And the method "open" of "tip" is called
      And the property "timeout" of "tip" is set to -1
      And 500 milliseconds pass
      Then the property "visible" of "tip" is true

    @api-QtQuick.Controls.ToolTip.delay
    Scenario: A tool tip with a delay opens when the delay has passed
      When the property "delay" of "tip" is set to 300
      And the method "open" of "tip" is called
      Then the property "visible" of "tip" is false
      When 100 milliseconds pass
      Then the property "visible" of "tip" is false
      And the property "log" of "root" is ""
      When 500 milliseconds pass
      Then the property "visible" of "tip" is true
      And the property "log" of "root" is "opened "

    Scenario: A tool tip closed during its delay does not open
      When the property "delay" of "tip" is set to 300
      And the method "open" of "tip" is called
      And 100 milliseconds pass
      And the method "close" of "tip" is called
      And 500 milliseconds pass
      Then the property "visible" of "tip" is false
      And the property "log" of "root" is ""

    Scenario: The delay comes before the timeout
      When the property "delay" of "tip" is set to 300
      And the property "timeout" of "tip" is set to 400
      And the method "open" of "tip" is called
      And 500 milliseconds pass
      Then the property "visible" of "tip" is true
      When 500 milliseconds pass
      Then the property "visible" of "tip" is false

  Rule: The tool tip that all items share

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property var tip: a.ToolTip.toolTip
          property bool same: a.ToolTip.toolTip === b.ToolTip.toolTip
          property string shown: "" + a.ToolTip.visible + b.ToolTip.visible + c.ToolTip.visible
          property string own: c.ToolTip.text + "/" + c.ToolTip.timeout
          property string seen: ""
          function look() { seen = "" + a.ToolTip.visible + b.ToolTip.visible + c.ToolTip.visible }
          function of(n) { return n === "a" ? a : n === "b" ? b : c }
          function show(n) { of(n).ToolTip.visible = true }
          function unshow(n) { of(n).ToolTip.visible = false }
          function say(n, what, ms) { of(n).ToolTip.show(what, ms) }
          function hush(n) { of(n).ToolTip.hide() }
          function retext(n, what) { of(n).ToolTip.text = what }
          Item { id: a; objectName: "a"; y: 150; width: 80; height: 30; ToolTip.text: "About A" }
          Item { id: b; objectName: "b"; x: 200; y: 150; width: 80; height: 30
                 ToolTip.text: "About B"; ToolTip.timeout: 200 }
          Item { id: c; objectName: "c"; y: 250; width: 80; height: 30; ToolTip.delay: 300 }
        }
        """

    @api-QtQuick.Controls.ToolTip.toolTip
    Scenario: Every item has the same tool tip
      Then the property "same" of "root" is true

    Scenario Outline: The shared tool tip starts with <property> <value>
      Then the property "tip.<property>" of "root" is <value>

      Examples:
        | property    | value |
        | visible     | false |
        | text        | ""    |
        | delay       | 0     |
        | timeout     | -1    |
        | parent      | null  |
        | padding     | 6     |
        | margins     | 6     |
        | closePolicy | 26    |

    Scenario: An item starts with a tool tip that is not shown
      Then the property "shown" of "root" is "falsefalsefalse"

    @api-QtQuick.Controls.ToolTip.visible
    Scenario: Showing the tool tip of an item makes the shared one its own
      When the method "show" of "root" is called with "a"
      Then the property "tip.visible" of "root" is true
      And the property "tip.text" of "root" is "About A"
      And the property "tip.parent.objectName" of "root" is "a"
      And the property "tip.timeout" of "root" is -1
      And the property "shown" of "root" is "truefalsefalse"

    Scenario: The tool tip moves on to the next item that shows it
      When the method "show" of "root" is called with "a"
      And the method "show" of "root" is called with "b"
      Then the property "tip.visible" of "root" is true
      And the property "tip.text" of "root" is "About B"
      And the property "tip.parent.objectName" of "root" is "b"
      And the property "tip.timeout" of "root" is 200
      When the method "look" of "root" is called
      Then the property "seen" of "root" is "falsetruefalse"

    Scenario: The timeout of the item closes the shared tool tip
      When the method "show" of "root" is called with "b"
      And 500 milliseconds pass
      Then the property "tip.visible" of "root" is false
      And the property "shown" of "root" is "falsefalsefalse"

    Scenario: Hiding the tool tip of an item closes the shared one
      When the method "show" of "root" is called with "a"
      And the method "unshow" of "root" is called with "a"
      Then the property "tip.visible" of "root" is false
      And the property "shown" of "root" is "falsefalsefalse"

    Scenario: An item cannot hide the tool tip of another
      When the method "show" of "root" is called with "a"
      And the method "unshow" of "root" is called with "b"
      And the method "hush" of "root" is called with "b"
      Then the property "tip.visible" of "root" is true
      And the property "shown" of "root" is "truefalsefalse"

    @api-QtQuick.Controls.ToolTip.text
    Scenario: The text of the item that shows the tool tip is the text shown
      When the method "show" of "root" is called with "a"
      And the method "retext" of "root" is called with "a", "Changed"
      Then the property "tip.text" of "root" is "Changed"

    Scenario: The text of another item is not shown
      When the method "show" of "root" is called with "a"
      And the method "retext" of "root" is called with "b", "Changed"
      Then the property "tip.text" of "root" is "About A"

    @api-QtQuick.Controls.ToolTip.show
    Scenario: show of an item shows a text that is not the item's own
      When the method "say" of "root" is called with "a", "Said", 200
      Then the property "tip.visible" of "root" is true
      And the property "tip.text" of "root" is "Said"
      And the property "tip.timeout" of "root" is 200
      And the property "shown" of "root" is "truefalsefalse"
      When 500 milliseconds pass
      Then the property "tip.visible" of "root" is false

    Scenario: What show was given is not kept by the item
      When the method "say" of "root" is called with "c", "Said", 200
      Then the property "own" of "root" is "/-1"

    @api-QtQuick.Controls.ToolTip.hide
    Scenario: hide of an item closes the tool tip it showed
      When the method "say" of "root" is called with "a", "Said", 200
      And the method "hush" of "root" is called with "a"
      Then the property "tip.visible" of "root" is false
      And the property "shown" of "root" is "falsefalsefalse"

    @api-QtQuick.Controls.ToolTip.delay
    Scenario: The delay of the item comes before the shared tool tip shows
      When the method "say" of "root" is called with "c", "Late", 5000
      Then the property "tip.visible" of "root" is false
      And the property "shown" of "root" is "falsefalsefalse"
      When 100 milliseconds pass
      Then the property "tip.visible" of "root" is false
      When 500 milliseconds pass
      Then the property "tip.visible" of "root" is true
      And the property "tip.text" of "root" is "Late"
      And the property "shown" of "root" is "falsefalsetrue"

    Scenario: A tool tip hidden during its delay does not show
      When the method "say" of "root" is called with "c", "Late", 5000
      And 100 milliseconds pass
      And the method "hush" of "root" is called with "c"
      And 500 milliseconds pass
      Then the property "tip.visible" of "root" is false

    Scenario: A tool tip asked for during its delay shows when the delay has passed
      When the method "show" of "root" is called with "c"
      And 100 milliseconds pass
      Then the property "tip.visible" of "root" is false
      When 500 milliseconds pass
      Then the property "tip.visible" of "root" is true
      And the property "shown" of "root" is "falsefalsetrue"

    Scenario: A tool tip not asked for any more during its delay does not show
      When the method "show" of "root" is called with "c"
      And 100 milliseconds pass
      And the method "unshow" of "root" is called with "c"
      And 500 milliseconds pass
      Then the property "tip.visible" of "root" is false
      And the property "shown" of "root" is "falsefalsefalse"
