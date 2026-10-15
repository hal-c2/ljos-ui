# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-abstractbutton.html
#   https://doc.qt.io/qt-6/qtquickcontrols-customize.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: AbstractButton
  AbstractButton is what every button is built on: it is pressed, released
  and clicked, and may be checked. It draws nothing itself.

  @qt-parity
  Rule: A click presses, releases and clicks

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 200
          AbstractButton {
            objectName: "b"
            width: 100; height: 40
            property string log: ""
            onPressed: log += "pressed "
            onReleased: log += "released "
            onCanceled: log += "canceled "
            onClicked: log += "clicked "
            onToggled: log += "toggled "
            onDoubleClicked: log += "doubleClicked "
            onPressAndHold: log += "pressAndHold "
          }
        }
        """

    Scenario Outline: A button starts out idle
      Then the property "<property>" of "b" is <value>

      Examples:
        | property           | value |
        | pressed            | false |
        | down               | false |
        | checked            | false |
        | checkable          | false |
        | autoExclusive      | false |
        | autoRepeat         | false |
        | autoRepeatDelay    | 300   |
        | autoRepeatInterval | 100   |
        | display            | 2     |
        | text               | ""    |
        | icon.width         | 0     |
        | icon.height        | 0     |
        | icon.name          | ""    |
        | implicitWidth      | 0     |
        | hoverEnabled       | true  |

    @api-QtQuick.Controls.AbstractButton.clicked
    Scenario: A click presses, releases and clicks
      When the user touches 30, 20
      Then the property "log" of "b" is "pressed released clicked "
      And the property "pressed" of "b" is false
      And the property "checked" of "b" is false

    @api-QtQuick.Controls.AbstractButton.pressX
    Scenario: The button remembers where it was pressed
      When the user touches 30, 20
      Then the property "pressX" of "b" is 30
      And the property "pressY" of "b" is 20

    Scenario: A click beside the button does nothing
      When the user touches 150, 100
      Then the property "log" of "b" is ""

    @api-QtQuick.Controls.AbstractButton.checkable
    Scenario: A click on a checkable button checks it and says so before the click
      When the property "checkable" of "b" is set to true
      And the user touches 30, 20
      Then the property "checked" of "b" is true
      And the property "log" of "b" is "pressed toggled released clicked "

    Scenario: A second click on a checkable button unchecks it
      When the property "checkable" of "b" is set to true
      And the user touches 30, 20
      And 1 second passes
      And the user touches 30, 20
      Then the property "checked" of "b" is false

    Scenario: A disabled button ignores a click
      When the property "enabled" of "b" is set to false
      And the user touches 30, 20
      Then the property "log" of "b" is ""

    @api-QtQuick.Controls.AbstractButton.click
    Scenario: click() does what a click of the user does
      When the method "click" of "b" is called
      Then the property "log" of "b" is "pressed released clicked "
      And the property "pressed" of "b" is false

    Scenario: click() checks a checkable button
      When the property "checkable" of "b" is set to true
      And the method "click" of "b" is called
      Then the property "checked" of "b" is true
      And the property "log" of "b" is "pressed toggled released clicked "

    Scenario: click() does nothing on a disabled button
      When the property "enabled" of "b" is set to false
      And the method "click" of "b" is called
      Then the property "log" of "b" is ""

    @api-QtQuick.Controls.AbstractButton.toggle
    Scenario: toggle() checks without a click, even a button that is not checkable
      When the method "toggle" of "b" is called
      Then the property "checked" of "b" is true
      And the property "log" of "b" is ""

    Scenario: toggle() unchecks a checked button
      When the property "checked" of "b" is set to true
      And the method "toggle" of "b" is called
      Then the property "checked" of "b" is false

    @api-QtQuick.Controls.AbstractButton.down
    Scenario: A script can hold the button down without pressing it
      When the property "down" of "b" is set to true
      Then the property "down" of "b" is true
      And the property "pressed" of "b" is false

  @qt-parity
  Rule: The ways to show text and icon have names

    @api-QtQuick.Controls.AbstractButton.display
    Scenario Outline: The display values have names
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          objectName: "names"
          property int iconOnly: AbstractButton.IconOnly
          property int textOnly: AbstractButton.TextOnly
          property int beside: AbstractButton.TextBesideIcon
          property int under: AbstractButton.TextUnderIcon
        }
        """
      Then the property "<property>" of "names" is <value>

      Examples:
        | property | value |
        | iconOnly | 0     |
        | textOnly | 1     |
        | beside   | 2     |
        | under    | 3     |

  @qt-parity @api-QtQuick.Controls.AbstractButton.autoExclusive
  Rule: Of the auto-exclusive buttons in one parent only one is checked

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 200
          AbstractButton {
            objectName: "one"; width: 50; height: 40
            checkable: true; autoExclusive: true; checked: true
            property int toggles: 0
            onToggled: toggles++
          }
          AbstractButton {
            objectName: "two"; x: 60; width: 50; height: 40
            checkable: true; autoExclusive: true
          }
          AbstractButton {
            objectName: "free"; x: 120; width: 50; height: 40
            checkable: true; checked: true
          }
        }
        """

    Scenario: Checking one unchecks the other
      When the property "checked" of "two" is set to true
      Then the property "checked" of "one" is false
      And the property "checked" of "free" is true

    Scenario: A click on the other one moves the check
      When the user touches 70, 20
      Then the property "checked" of "two" is true
      And the property "checked" of "one" is false

    Scenario: A click on the checked one leaves it checked
      When the user touches 10, 20
      Then the property "checked" of "one" is true
      And the property "toggles" of "one" is 0

    Scenario: A button that is not auto-exclusive is left alone
      When the user touches 130, 20
      Then the property "checked" of "free" is false
      And the property "checked" of "one" is true

  # A switch for the whole application, as a settings page has one: the
  # button's checked is bound to the setting, and its own click handler
  # writes the setting. The click checks or unchecks the button first, so
  # the handler writes what the binding already gives: no loop.
  @qt-parity
  Rule: A button bound to what its click handler writes

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 300; height: 200
          property bool active: true
          Button {
            id: button
            objectName: "b"
            width: 100; height: 40
            checkable: true
            checked: root.active
            Connections {
              function onClicked() { root.active = button.checked }
            }
          }
          Switch {
            objectName: "s"
            y: 60
            enabled: root.active
            checked: root.active
          }
        }
        """

    Scenario: A click turns the setting off and what reads it follows
      When the user touches 30, 20
      Then the property "checked" of "b" is false
      And the property "active" of "root" is false
      And the property "enabled" of "s" is false
      And the property "checked" of "s" is false
      And the log has no warnings

    Scenario: A second click turns it back on
      When the user touches 30, 20
      And 1 second passes
      And the user touches 30, 20
      Then the property "checked" of "b" is true
      And the property "active" of "root" is true
      And the property "enabled" of "s" is true

    Scenario: The binding of checked outlives the click
      When the user touches 30, 20
      And the property "active" of "root" is set to true
      Then the property "checked" of "b" is true

  # The Qt oracle sends whole clicks; what happens between the press and
  # the release was read from Qt with its own test tool (QtTest) instead.
  @ljos-only
  Rule: While the pointer is held

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 200
          AbstractButton {
            objectName: "b"
            width: 100; height: 40
            property string log: ""
            onPressed: log += "pressed "
            onReleased: log += "released "
            onCanceled: log += "canceled "
            onClicked: log += "clicked "
            onToggled: log += "toggled "
            onDoubleClicked: log += "doubleClicked "
            onPressAndHold: log += "pressAndHold "
          }
        }
        """

    Scenario: The button is pressed and down while the pointer is held on it
      When the user presses the pointer at 30, 20
      Then the property "pressed" of "b" is true
      And the property "down" of "b" is true
      And the property "log" of "b" is "pressed "

    Scenario: Moving off the button lets it up, moving back presses it again
      When the user presses the pointer at 30, 20
      And the user moves the pointer to 150, 100
      Then the property "pressed" of "b" is false
      When the user moves the pointer to 40, 20
      Then the property "pressed" of "b" is true
      And the property "log" of "b" is "pressed "

    @api-QtQuick.Controls.AbstractButton.canceled
    Scenario: Releasing outside the button cancels the click
      When the user presses the pointer at 30, 20
      And the user moves the pointer to 150, 100
      And the user releases the pointer at 150, 100
      Then the property "log" of "b" is "pressed canceled "
      And the property "pressed" of "b" is false

    @api-QtQuick.Controls.AbstractButton.doubleClicked
    Scenario: The second of two quick clicks is a double click and not a click
      When the user touches 30, 20
      And 100 milliseconds pass
      And the user touches 30, 20
      Then the property "log" of "b" is "pressed released clicked pressed doubleClicked released "

    Scenario: Two slow clicks are two clicks
      When the user touches 30, 20
      And 1 second passes
      And the user touches 30, 20
      Then the property "log" of "b" is "pressed released clicked pressed released clicked "

    @api-QtQuick.Controls.AbstractButton.pressAndHold
    Scenario: Holding the pointer down is a press-and-hold and not a click
      When the user presses the pointer at 30, 20
      And 1 second passes
      Then the property "log" of "b" is "pressed pressAndHold "
      When the user releases the pointer at 30, 20
      Then the property "log" of "b" is "pressed pressAndHold released "

    @api-QtQuick.Controls.AbstractButton.autoRepeat
    Scenario: An auto-repeating button clicks again and again while it is held
      When the property "autoRepeat" of "b" is set to true
      And the user presses the pointer at 30, 20
      And 250 milliseconds pass
      Then the property "log" of "b" is "pressed "
      When 100 milliseconds pass
      Then the property "log" of "b" is "pressed released clicked pressed "
      When 100 milliseconds pass
      Then the property "log" of "b" is "pressed released clicked pressed released clicked pressed "
      When the user releases the pointer at 30, 20
      Then the property "log" of "b" is "pressed released clicked pressed released clicked pressed released "

    Scenario: An auto-repeating button let go early clicks once
      When the property "autoRepeat" of "b" is set to true
      And the user presses the pointer at 30, 20
      And 100 milliseconds pass
      And the user releases the pointer at 30, 20
      Then the property "log" of "b" is "pressed released clicked "

    Scenario: Disabling a pressed button cancels the click
      When the user presses the pointer at 30, 20
      And the property "enabled" of "b" is set to false
      Then the property "pressed" of "b" is false
      And the property "log" of "b" is "pressed canceled "

    @api-QtQuick.Controls.Control.hovered
    Scenario: The button is hovered while the pointer is over it
      When the user moves the pointer to 30, 20
      Then the property "hovered" of "b" is true
      When the user moves the pointer to 150, 100
      Then the property "hovered" of "b" is false

    @api-QtQuick.Controls.Control.hoverEnabled
    Scenario: A button that does not take hover is never hovered
      When the property "hoverEnabled" of "b" is set to false
      And the user moves the pointer to 30, 20
      Then the property "hovered" of "b" is false
