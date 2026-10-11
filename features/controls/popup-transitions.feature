# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#enter-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#exit-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#opened-prop
#   https://doc.qt.io/qt-6/qml-qtquick-transition.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Popup transitions
  A popup comes with its `enter` transition and goes with its `exit`
  transition. It is opened when it has come and closed when it has gone.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 400; height: 400
        property string log: ""
        property bool faint: p.opacity > 0 && p.opacity < 1
        property bool small: p.scale > 0.5 && p.scale < 1
        function both() { p.open(); p.close() }
        Popup {
          id: p; objectName: "p"; width: 100; height: 100
          enter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 400 } }
          exit: Transition { NumberAnimation { property: "scale"; from: 1; to: 0.5; duration: 400 } }
          onAboutToShow: root.log += "aboutToShow "
          onOpened: root.log += "opened "
          onAboutToHide: root.log += "aboutToHide "
          onClosed: root.log += "closed "
          onVisibleChanged: root.log += visible ? "visible " : "hidden "
        }
      }
      """

  Rule: A popup is opened when its enter transition has run

    @api-QtQuick.Controls.Popup.enter
    Scenario Outline: The <property> of a popup that is coming is <value>
      When the method "open" of "p" is called
      Then the property "<property>" of "p" is <value>

      Examples:
        | property      | value |
        | visible       | true  |
        | opened        | false |
        | opacity       | 0     |
        | enter.running | true  |
        | exit.running  | false |

    @api-QtQuick.Controls.Popup.aboutToShow
    Scenario: A popup that is coming has said that it is about to show
      When the method "open" of "p" is called
      Then the property "log" of "root" is "aboutToShow visible "

    @api-QtQuick.Controls.Popup.enter
    Scenario: The transition takes the popup there over its time
      When the method "open" of "p" is called
      And 200 milliseconds pass
      Then the property "faint" of "root" is true
      And the property "opened" of "p" is false

    @api-QtQuick.Controls.Popup.opened
    Scenario: A popup says that it is opened when the transition is over
      When the method "open" of "p" is called
      And 700 milliseconds pass
      Then the property "log" of "root" is "aboutToShow visible opened "
      And the property "opened" of "p" is true
      And the property "opacity" of "p" is 1
      And the property "enter.running" of "p" is false

    @api-QtQuick.Controls.Popup.visible
    Scenario: A popup that is made visible comes the same way
      When the property "visible" of "p" is set to true
      Then the property "log" of "root" is "aboutToShow visible "
      And the property "enter.running" of "p" is true

  Rule: A popup is closed when its exit transition has run

    Background:
      When the method "open" of "p" is called
      And 700 milliseconds pass
      And the property "log" of "root" is set to ""

    @api-QtQuick.Controls.Popup.exit
    Scenario Outline: The <property> of a popup that is going is <value>
      When the method "close" of "p" is called
      Then the property "<property>" of "p" is <value>

      Examples:
        | property      | value |
        | visible       | true  |
        | opened        | false |
        | exit.running  | true  |
        | enter.running | false |

    @api-QtQuick.Controls.Popup.aboutToHide
    Scenario: A popup that is going has said that it is about to hide
      When the method "close" of "p" is called
      Then the property "log" of "root" is "aboutToHide "

    @api-QtQuick.Controls.Popup.exit
    Scenario: The transition takes the popup away over its time
      When the method "close" of "p" is called
      And 200 milliseconds pass
      Then the property "small" of "root" is true
      And the property "visible" of "p" is true

    @api-QtQuick.Controls.Popup.closed
    Scenario: A popup says that it is closed when the transition is over
      When the method "close" of "p" is called
      And 700 milliseconds pass
      Then the property "log" of "root" is "aboutToHide hidden closed "
      And the property "visible" of "p" is false
      And the property "exit.running" of "p" is false

    @api-QtQuick.Controls.Popup.exit
    Scenario: A popup that has gone is as large as it was before it went
      When the method "close" of "p" is called
      And 700 milliseconds pass
      Then the property "scale" of "p" is 1
      And the property "opacity" of "p" is 1

    @api-QtQuick.Controls.Popup.visible
    Scenario: A popup that is made invisible stays visible until it has gone
      When the property "visible" of "p" is set to false
      Then the property "log" of "root" is "aboutToHide "
      And the property "visible" of "p" is true

    @api-QtQuick.Controls.Popup.visible
    Scenario: A popup that is made invisible goes the same way
      When the property "visible" of "p" is set to false
      And 700 milliseconds pass
      Then the property "log" of "root" is "aboutToHide hidden closed "

    @api-QtQuick.Controls.Popup.open
    Scenario: A popup that is opened while it goes comes back
      When the method "close" of "p" is called
      And 100 milliseconds pass
      And the method "open" of "p" is called
      Then the property "log" of "root" is "aboutToHide aboutToShow visible "
      And the property "exit.running" of "p" is false
      And the property "enter.running" of "p" is true

    @api-QtQuick.Controls.Popup.open
    Scenario: A popup that came back is opened and never closed
      When the method "close" of "p" is called
      And 100 milliseconds pass
      And the method "open" of "p" is called
      And 700 milliseconds pass
      Then the property "log" of "root" is "aboutToHide aboutToShow visible opened "
      And the property "visible" of "p" is true
      And the property "opened" of "p" is true

  Rule: A popup that is closed while it comes goes from where it is

    @api-QtQuick.Controls.Popup.close
    Scenario: The enter transition stops and the exit transition runs
      When the method "open" of "p" is called
      And 100 milliseconds pass
      And the method "close" of "p" is called
      Then the property "log" of "root" is "aboutToShow visible aboutToHide "
      And the property "enter.running" of "p" is false
      And the property "exit.running" of "p" is true

    @api-QtQuick.Controls.Popup.close
    Scenario: It is closed without having been opened
      When the method "open" of "p" is called
      And 100 milliseconds pass
      And the method "close" of "p" is called
      And 700 milliseconds pass
      Then the property "log" of "root" is "aboutToShow visible aboutToHide hidden closed "
      And the property "visible" of "p" is false

    @api-QtQuick.Controls.Popup.close
    Scenario: A popup that is opened and closed in one go still goes
      When the method "both" of "root" is called
      And 700 milliseconds pass
      Then the property "log" of "root" is "aboutToShow visible aboutToHide hidden closed "
      And the property "visible" of "p" is false
