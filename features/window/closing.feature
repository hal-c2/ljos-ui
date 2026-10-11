# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-window.html
#   https://doc.qt.io/qt-6/qml-qtquick-closeevent.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Closing a window
  Before a window closes it emits `closing` with an event whose `accepted`
  a handler may set to false to keep the window open.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        component Asking: Window {
          width: 300; height: 200
          property int asked: 0
          property bool refuse: false
          onClosing: (close) => {
            asked += 1
            close.accepted = !refuse
            refuse = false
          }
        }
        Asking { objectName: "w"; visible: true }
        Asking { objectName: "unseen" }
      }
      """

  @api-QtQuick.Window.close
  Scenario: close() hides the window
    When the method "close" of "w" is called
    Then the property "visible" of "w" is false
    And the property "visibility" of "w" is 0

  @api-QtQuick.Window.closing
  Scenario: The window asks once before it closes
    When the method "close" of "w" is called
    Then the property "asked" of "w" is 1

  @api-QtQuick.Window.closing @api-QtQuick.CloseEvent.accepted
  Scenario: A handler that does not accept keeps the window open
    When the property "refuse" of "w" is set to true
    And the method "close" of "w" is called
    Then the property "visible" of "w" is true
    And the property "asked" of "w" is 1

  @api-QtQuick.Window.closing @api-QtQuick.CloseEvent.accepted
  Scenario: A window that was kept open closes when it is asked again
    When the property "refuse" of "w" is set to true
    And the method "close" of "w" is called
    And the method "close" of "w" is called
    Then the property "visible" of "w" is false
    And the property "asked" of "w" is 2

  @api-QtQuick.Window.close
  Scenario: A window that was never shown does not ask
    When the method "close" of "unseen" is called
    Then the property "asked" of "unseen" is 0

  @api-QtQuick.Window.close
  Scenario: A window that was shown and hidden still asks
    When the method "hide" of "w" is called
    And the method "close" of "w" is called
    Then the property "asked" of "w" is 1

  @api-QtQuick.Window.close
  Scenario: A closed window can be shown again
    When the method "close" of "w" is called
    And the method "show" of "w" is called
    Then the property "visible" of "w" is true
    And the property "visibility" of "w" is 2
