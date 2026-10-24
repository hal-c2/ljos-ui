# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-scrollview.html
#   https://doc.qt.io/qt-6/qtquickcontrols-customize.html#customizing-scrollview
#   https://doc.qt.io/qt-6/qml-qtquick-controls-scrollbar.html
# Verified on Qt 6.11.2 (qml6, offscreen and vnc).

@qt-parity
Feature: ScrollView
  A scroll view shows part of something larger and gives it scroll bars.
  It scrolls a Flickable: the one declared in it, or one it makes around
  the item declared in it.

  Rule: A view with nothing in it

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 250
          ScrollView { objectName: "bare" }
          ScrollView {
            objectName: "view"
            x: 50; y: 20; width: 200; height: 100; padding: 10
          }
        }
        """

    @api-QtQuick.Controls.ScrollView.contentWidth @api-QtQuick.Controls.ScrollView.contentHeight
    Scenario Outline: A view starts out without a size and without content
      Then the property "<property>" of "bare" is <value>

      Examples:
        | property        | value |
        | implicitWidth   | 0     |
        | implicitHeight  | 0     |
        | contentWidth    | -1    |
        | contentHeight   | -1    |
        | padding         | 0     |
        | availableWidth  | 0     |
        | availableHeight | 0     |
        | focusPolicy     | 0     |
        | wheelEnabled    | true  |
        | clip            | false |

    @api-QtQuick.Controls.ScrollView.availableWidth
    @api-QtQuick.Controls.ScrollView.availableHeight
    Scenario Outline: The view makes a Flickable that fills it inside the padding
      Then the property "<property>" of "view" is <value>

      Examples:
        | property                  | value |
        | availableWidth            | 180   |
        | availableHeight           | 80    |
        | contentItem.x             | 10    |
        | contentItem.y             | 10    |
        | contentItem.width         | 180   |
        | contentItem.height        | 80    |
        | contentItem.clip          | true  |
        | contentItem.interactive   | true  |
        | contentItem.contentWidth  | -1    |
        | contentItem.contentHeight | -1    |
        | contentItem.contentY      | 0     |

  Rule: Around one item

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 250
          ScrollView {
            objectName: "view"
            width: 200; height: 100
            property Item down: ScrollBar.vertical
            property Item across: ScrollBar.horizontal
            Rectangle {
              objectName: "page"
              implicitWidth: 500; implicitHeight: 300
              color: "#336699"
            }
          }
        }
        """

    @api-QtQuick.Controls.ScrollView.contentWidth @api-QtQuick.Controls.ScrollView.contentHeight
    @api-QtQuick.Controls.ScrollView.effectiveScrollBarWidth
    @api-QtQuick.Controls.ScrollView.effectiveScrollBarHeight
    Scenario Outline: The content is as big as the item asks to be
      Then the property "<property>" of "view" is <value>

      Examples:
        | property                   | value |
        | contentWidth               | 500   |
        | contentHeight              | 300   |
        | implicitWidth              | 500   |
        | implicitHeight             | 300   |
        | implicitContentWidth       | 500   |
        | implicitContentHeight      | 300   |
        | availableWidth             | 200   |
        | availableHeight            | 100   |
        | contentItem.width          | 200   |
        | contentItem.height         | 100   |
        | contentItem.contentWidth   | 500   |
        | contentItem.contentHeight  | 300   |
        | contentChildren.length     | 1     |
        | effectiveScrollBarWidth    | 10    |
        | effectiveScrollBarHeight   | 10    |

    Scenario: The item is put into the content of the Flickable
      Then the property "parent.parent.parent.objectName" of "page" is "view"
      And the property "x" of "page" is 0
      And the property "y" of "page" is 0

    @api-QtQuick.Controls.ScrollBar.vertical
    Scenario Outline: The vertical bar runs down the right edge of the view
      Then the property "<property>" of "view" is <value>

      Examples:
        | property               | value  |
        | down.parent.objectName | "view" |
        | down.x                 | 190    |
        | down.y                 | 0      |
        | down.width             | 10     |
        | down.height            | 100    |
        | down.orientation       | 2      |
        | down.policy            | 0      |
        | down.visible           | true   |
        | down.interactive       | true   |
        | down.position          | 0      |
        | down.minimumSize       | 0.1    |

    @api-QtQuick.Controls.ScrollBar.horizontal
    Scenario Outline: The horizontal bar runs along the bottom edge of the view
      Then the property "<property>" of "view" is <value>

      Examples:
        | property                 | value  |
        | across.parent.objectName | "view" |
        | across.x                 | 0      |
        | across.y                 | 90     |
        | across.width             | 200    |
        | across.height            | 10     |
        | across.orientation       | 1      |
        | across.size              | 0.4    |
        | across.position          | 0      |

    Scenario: The bars show the share of the content in view
      Then the property "down.size" of "view" is within 0.0001 of 0.3333
      And the property "across.size" of "view" is within 0.0001 of 0.4

    Scenario: The content follows what the item asks for
      When the property "implicitHeight" of "page" is set to 900
      Then the property "contentHeight" of "view" is 900
      And the property "contentItem.contentHeight" of "view" is 900
      And the property "implicitHeight" of "view" is 900
      And the property "down.size" of "view" is within 0.0001 of 0.1111

    Scenario: A content size that was given is kept
      When the property "contentHeight" of "view" is set to 1200
      Then the property "contentItem.contentHeight" of "view" is 1200
      When the property "implicitHeight" of "page" is set to 100
      Then the property "contentHeight" of "view" is 1200
      And the property "contentItem.contentHeight" of "view" is 1200
      And the property "contentWidth" of "view" is 500

    Scenario: The bars follow the content
      When the property "contentItem.contentY" of "view" is set to 75
      And the property "contentItem.contentX" of "view" is set to 150
      Then the property "down.position" of "view" is 0.25
      And the property "across.position" of "view" is 0.3

    Scenario: The content follows the bars
      When the property "down.position" of "view" is set to 0.5
      Then the property "contentItem.contentY" of "view" is 150

    Scenario: The view follows its own size
      When the property "width" of "view" is set to 250
      And the property "height" of "view" is set to 150
      Then the property "contentItem.width" of "view" is 250
      And the property "down.x" of "view" is 240
      And the property "down.height" of "view" is 150
      And the property "down.size" of "view" is 0.5
      And the property "across.y" of "view" is 140
      And the property "across.width" of "view" is 250

  Rule: The policies of the bars

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 250
          ScrollView {
            objectName: "view"
            width: 200; height: 100
            property Item down: ScrollBar.vertical
            property Item across: ScrollBar.horizontal
            ScrollBar.vertical.policy: ScrollBar.AlwaysOn
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            Rectangle { implicitWidth: 500; implicitHeight: 300 }
          }
        }
        """

    @api-QtQuick.Controls.ScrollBar.policy
    Scenario Outline: A bar that is switched off is not there and takes no room
      Then the property "<property>" of "view" is <value>

      Examples:
        | property                 | value |
        | down.policy              | 2     |
        | down.visible             | true  |
        | down.contentItem.opacity | 0.75  |
        | effectiveScrollBarWidth  | 10    |
        | across.policy            | 1     |
        | across.visible           | false |
        | effectiveScrollBarHeight | 0     |

    Scenario: A bar that is switched on again is back
      When the property "across.policy" of "view" is set to 0
      Then the property "across.visible" of "view" is true
      And the property "effectiveScrollBarHeight" of "view" is 10
      When the property "down.policy" of "view" is set to 1
      Then the property "down.visible" of "view" is false
      And the property "effectiveScrollBarWidth" of "view" is 0

    Scenario: A policy is set on the bar the view attaches without a word
      Then the log has no warnings

  Rule: Inside the padding

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 250
          ScrollView {
            objectName: "view"
            width: 100; height: 80
            topPadding: 5; leftPadding: 7; rightPadding: 3; bottomPadding: 9
            property Item down: ScrollBar.vertical
            property Item across: ScrollBar.horizontal
            Rectangle { implicitWidth: 500; implicitHeight: 300 }
          }
        }
        """

    Scenario Outline: The Flickable and the bars keep to the padding
      Then the property "<property>" of "view" is <value>

      Examples:
        | property           | value |
        | availableWidth     | 90    |
        | availableHeight    | 66    |
        | implicitWidth      | 510   |
        | implicitHeight     | 314   |
        | contentItem.x      | 7     |
        | contentItem.y      | 5     |
        | contentItem.width  | 90    |
        | contentItem.height | 66    |
        | down.x             | 90    |
        | down.y             | 5     |
        | down.height        | 66    |
        | down.size          | 0.22  |
        | across.x           | 7     |
        | across.y           | 70    |
        | across.width       | 90    |
        | across.size        | 0.18  |

  Rule: Around a Flickable

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 250
          ScrollView {
            objectName: "view"
            width: 200; height: 100
            property Item down: ScrollBar.vertical
            property Item across: ScrollBar.horizontal
            Flickable {
              objectName: "f"
              contentWidth: 500; contentHeight: 300
              boundsBehavior: Flickable.StopAtBounds
              Rectangle { width: 500; height: 300; color: "#336699" }
            }
          }
        }
        """

    Scenario Outline: A Flickable declared in the view is the one it scrolls
      Then the property "<property>" of "view" is <value>

      Examples:
        | property               | value |
        | contentItem.objectName | "f"   |
        | contentWidth           | 500   |
        | contentHeight          | 300   |
        | implicitWidth          | 500   |
        | implicitHeight         | 300   |
        | contentChildren.length | 1     |
        | down.x                 | 190   |
        | down.height            | 100   |
        | across.y               | 90    |
        | across.size            | 0.4   |

    Scenario Outline: The Flickable fills the view and is otherwise left as it was
      Then the property "<property>" of "f" is <value>

      Examples:
        | property          | value  |
        | parent.objectName | "view" |
        | x                 | 0      |
        | y                 | 0      |
        | width             | 200    |
        | height            | 100    |
        | clip              | false  |
        | interactive       | true   |

    Scenario: The bars and the Flickable follow each other
      When the property "contentY" of "f" is set to 75
      Then the property "down.position" of "view" is 0.25
      When the property "across.position" of "view" is set to 0.5
      Then the property "contentX" of "f" is 250

    Scenario: The content size of the view is the Flickable's, either way
      When the property "contentHeight" of "view" is set to 800
      Then the property "contentHeight" of "f" is 800
      When the property "contentWidth" of "f" is set to 900
      Then the property "contentWidth" of "view" is 900
      And the property "across.size" of "view" is within 0.0001 of 0.2222

  Rule: Around several items

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 250
          ScrollView {
            objectName: "loose"
            width: 100; height: 100
            property Item down: ScrollBar.vertical
            Rectangle { objectName: "one"; width: 100; height: 50 }
            Rectangle { x: 300; y: 200; width: 100; height: 50 }
          }
          ScrollView {
            objectName: "sized"
            x: 100; width: 200; height: 100
            contentWidth: 400; contentHeight: 600
            property Item down: ScrollBar.vertical
            Rectangle { width: 100; height: 50 }
            Rectangle { x: 300; y: 550; width: 100; height: 50 }
          }
        }
        """

    Scenario Outline: Several items say nothing about the size of the content
      Then the property "<property>" of "loose" is <value>

      Examples:
        | property                  | value |
        | contentWidth              | -1    |
        | contentHeight             | -1    |
        | implicitWidth             | 0     |
        | contentChildren.length    | 2     |
        | contentItem.contentHeight | -1    |
        | down.size                 | 1     |

    Scenario: The items are in the content all the same
      Then the property "parent.parent.parent.objectName" of "one" is "loose"

    Scenario Outline: A content size given to the view is the Flickable's
      Then the property "<property>" of "sized" is <value>

      Examples:
        | property                  | value |
        | contentWidth              | 400   |
        | contentHeight             | 600   |
        | implicitWidth             | 400   |
        | implicitHeight            | 600   |
        | contentItem.contentWidth  | 400   |
        | contentItem.contentHeight | 600   |

    Scenario: The bars of a view with a given content size show its share
      Then the property "down.size" of "sized" is within 0.0001 of 0.1667

  Rule: The pointer

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 300; height: 250
          property int clicks: 0
          ScrollView {
            objectName: "view"
            width: 200; height: 100
            property Item down: ScrollBar.vertical
            property Item across: ScrollBar.horizontal
            Rectangle {
              implicitWidth: 500; implicitHeight: 300
              color: "#336699"
              MouseArea { width: 100; height: 100; onClicked: root.clicks++ }
            }
          }
        }
        """

    Scenario: A press on the track of a bar scrolls the content there
      When the user presses the pointer at 195, 70
      Then the property "down.pressed" of "view" is true
      And the property "contentItem.contentY" of "view" is 163
      When the user releases the pointer at 195, 70
      Then the property "down.pressed" of "view" is false

    Scenario: Dragging the handle of a bar scrolls the content
      When the user presses the pointer at 195, 10
      And the user moves the pointer to 195, 34
      Then the property "contentItem.contentY" of "view" is within 0.01 of 75
      And the property "contentItem.dragging" of "view" is false
      When the user releases the pointer at 195, 34
      Then the property "contentItem.contentY" of "view" is within 0.01 of 75

    Scenario: Both bars show while the pointer is over one of them
      When the user moves the pointer to 195, 50
      Then the property "down.hovered" of "view" is true
      And the property "down.active" of "view" is true
      And the property "across.active" of "view" is true
      When the user moves the pointer to 250, 200
      Then the property "down.active" of "view" is false
      And the property "across.active" of "view" is false

    Scenario: What is in the view is clicked as anywhere else
      When the user presses the pointer at 50, 50
      And the user releases the pointer at 50, 50
      Then the property "clicks" of "root" is 1

  Rule: The keyboard

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          objectName: "root"
          width: 300; height: 250
          property int passed: 0
          Keys.onPressed: passed++
          ScrollView {
            objectName: "view"
            width: 200; height: 100
            focus: true
            property Item down: ScrollBar.vertical
            property Item across: ScrollBar.horizontal
            Rectangle { implicitWidth: 500; implicitHeight: 300; color: "#336699" }
          }
        }
        """

    Scenario: The arrow keys step the bars of a view that has the focus
      Then the property "activeFocus" of "view" is true
      When the user presses "Down"
      Then the property "contentItem.contentY" of "view" is within 0.01 of 30
      When the user presses "Down"
      Then the property "contentItem.contentY" of "view" is within 0.01 of 60
      When the user presses "Up"
      Then the property "contentItem.contentY" of "view" is within 0.01 of 30
      When the user presses "Right"
      Then the property "contentItem.contentX" of "view" is within 0.01 of 50
      When the user presses "Left"
      Then the property "contentItem.contentX" of "view" is within 0.01 of 0

    Scenario: The keys stop at the ends of the content
      When the user presses "Up"
      Then the property "contentItem.contentY" of "view" is 0
      When the property "down.position" of "view" is set to 0.6
      And the user presses "Down"
      Then the property "contentItem.contentY" of "view" is within 0.01 of 200

    Scenario: A step of the bar is the step of the key
      When the property "down.stepSize" of "view" is set to 0.25
      And the user presses "Down"
      Then the property "contentItem.contentY" of "view" is within 0.01 of 75

    Scenario Outline: The <key> key does not scroll a view
      When the property "contentItem.contentY" of "view" is set to 90
      And the user presses "<key>"
      Then the property "contentItem.contentY" of "view" is 90

      Examples:
        | key      |
        | PgDown   |
        | PgUp     |
        | Home     |
        | End      |
        | Space    |

    Scenario: The view keeps the arrow keys and passes the others on
      When the user presses "Down"
      Then the property "passed" of "root" is 0
      When the user presses "Up"
      And the user presses "Up"
      Then the property "passed" of "root" is 0
      When the user presses "PgDown"
      And the user presses "A"
      Then the property "passed" of "root" is 2

    Scenario: A view without the focus leaves the keys alone
      When the property "focus" of "view" is set to false
      And the user presses "Down"
      Then the property "contentItem.contentY" of "view" is 0
