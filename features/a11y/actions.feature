# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html#pressAction-signal
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html#toggleAction-signal
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html#increaseAction-signal
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html#scrollDownAction-signal
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html#nextPageAction-signal
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html#showOnScreenAction-signal
#   docs/ARCHITECTURE.md (section 14)
# The Qt oracle cannot play an assistive technology, so these scenarios run on
# Ljós only. showOnScreenAction is documented for Qt 6.12.

@ljos-only @a11y
Feature: What an assistive technology asks of an item
  A request of a screen reader reaches the item as a signal of Accessible, and
  the item's handler does what was asked.

  Rule: An item with a handler for each request

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          width: 300; height: 200
          property string log: ""
          Rectangle {
            objectName: "it"
            width: 80; height: 30
            Accessible.role: Accessible.Button; Accessible.name: "It"
            Accessible.onPressAction: root.log += "press "
            Accessible.onToggleAction: root.log += "toggle "
            Accessible.onIncreaseAction: root.log += "increase "
            Accessible.onDecreaseAction: root.log += "decrease "
            Accessible.onScrollUpAction: root.log += "up "
            Accessible.onScrollDownAction: root.log += "down "
            Accessible.onScrollLeftAction: root.log += "left "
            Accessible.onScrollRightAction: root.log += "right "
            Accessible.onPreviousPageAction: root.log += "previous "
            Accessible.onNextPageAction: root.log += "next "
            Accessible.onShowOnScreenAction: root.log += "show "
            MouseArea { anchors.fill: parent; onClicked: root.log += "clicked " }
          }
        }
        """
      And an assistive technology is listening

    Scenario Outline: A request reaches the handler of its signal
      When an assistive technology asks the "button" named "It" to <request>
      Then the property "log" of "root" is "<heard> "

      Examples:
        | request             | heard    |
        | press               | press    |
        | increase            | increase |
        | decrease            | decrease |
        | scroll up           | up       |
        | scroll down         | down     |
        | scroll left         | left     |
        | scroll right        | right    |
        | scroll up a page    | previous |
        | scroll down a page  | next     |
        | scroll left a page  | previous |
        | scroll right a page | next     |
        | show itself         | show     |

    Scenario: A press on an item that can be checked is a toggle
      When the property "Accessible.checkable" of "it" is set to true
      And an assistive technology asks the "button" named "It" to press
      Then the property "log" of "root" is "toggle "

    Scenario: An item that is not enabled is asked nothing
      When the property "enabled" of "it" is set to false
      And an assistive technology asks the "button" named "It" to press
      And an assistive technology asks the "button" named "It" to increase
      Then the property "log" of "root" is ""

  Rule: An item without a handler

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          width: 300; height: 200
          property string log: ""
          Rectangle {
            objectName: "it"
            width: 80; height: 30
            Accessible.role: Accessible.CheckBox; Accessible.name: "It"
            MouseArea { anchors.fill: parent; onClicked: root.log += "clicked " }
          }
        }
        """
      And an assistive technology is listening

    Scenario: A press without a handler is a click on the item
      When an assistive technology asks the "checkbox" named "It" to press
      Then the property "log" of "root" is "clicked "

    Scenario Outline: Any other request without a handler does nothing
      When an assistive technology asks the "checkbox" named "It" to <request>
      Then the property "log" of "root" is ""
      And the log has no warnings

      Examples:
        | request            |
        | increase           |
        | decrease           |
        | scroll down        |
        | scroll down a page |
        | show itself        |

  Rule: An item in a view that scrolled

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          width: 300; height: 300
          property string log: ""
          Flickable {
            objectName: "view"
            y: 40; width: 200; height: 120; clip: true
            contentHeight: 600
            Column {
              Repeater {
                model: 20
                Rectangle {
                  required property int index
                  width: 200; height: 30
                  Accessible.role: Accessible.Button; Accessible.name: "Row " + index
                  MouseArea { anchors.fill: parent; onClicked: root.log += "row " + index + " " }
                }
              }
            }
          }
        }
        """
      And an assistive technology is listening

    # The click lands where the row is now, not where it was.
    Scenario: A press after a scroll clicks the row that was asked
      When the property "contentY" of "view" is set to 95
      And an assistive technology asks the "button" named "Row 5" to press
      Then the property "log" of "root" is "row 5 "
