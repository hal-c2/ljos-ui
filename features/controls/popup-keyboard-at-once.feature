# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#focus-prop
#   https://doc.qt.io/qt-6/qtquickcontrols-focus.html
# Verified on Qt 6.11.2 and 6.12.0 (qml6, offscreen).

@qt-parity
Feature: A popup that takes the keyboard as the content's keyboard moves
  A popup gives the keyboard back to what had it in the window's content
  when the popup took it. That holds when the popup opens before the
  window is shown, when the content has never had the keyboard yet, and
  when the content's keyboard moves in the same call that opens the popup.

  Scenario: A popup shown as the window loads gives the keyboard to the content when it closes
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 400
        Item { objectName: "outside"; focus: true }
        Popup {
          objectName: "early"
          visible: true
          focus: true
          Item { objectName: "inEarly"; focus: true }
        }
      }
      """
    Then the property "activeFocus" of "inEarly" is true
    When the method "close" of "early" is called
    Then the property "activeFocus" of "outside" is true
    And the property "activeFocus" of "inEarly" is false

  Scenario: A popup opened as the window completes gives the keyboard to the content when it closes
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 400
        Item { objectName: "outside"; focus: true }
        Popup {
          id: p
          objectName: "p"
          focus: true
          Item { objectName: "one"; focus: true }
        }
        Component.onCompleted: p.open()
      }
      """
    Then the property "activeFocus" of "one" is true
    When the method "close" of "p" is called
    Then the property "activeFocus" of "outside" is true

  Scenario: Focus given in the content and a popup opened in one call
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root
        objectName: "root"
        width: 400; height: 400
        Item { objectName: "outside"; focus: true }
        Item { id: other; objectName: "other" }
        Popup {
          id: p
          objectName: "p"
          focus: true
          Item { objectName: "one"; focus: true }
        }
        function focusOtherAndOpen() { other.forceActiveFocus(); p.open() }
      }
      """
    When the method "focusOtherAndOpen" of "root" is called
    Then the property "activeFocus" of "one" is true
    When the method "close" of "p" is called
    Then the property "activeFocus" of "other" is true
    And the property "activeFocus" of "outside" is false
