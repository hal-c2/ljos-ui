# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#visualFocus-prop
#   https://doc.qt.io/qt-6/qtquickcontrols-customize.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The Basic style shows where the keyboard is
  A control that a key brought the keyboard to has a frame in the
  highlight colour, two pixels wide. One that has no keyboard has none.

  Scenario Outline: A <control> that Tab stopped at has a frame in the highlight colour
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
          width: 200; height: 60
          color: "#ffffff"
          <control> { objectName: "c"; x: 10; y: 10; width: 140; height: 40 }
      }
      """
    When the user presses "Tab"
    Then the property "activeFocus" of "c" is true
    And the pixel at 80, 10 of the window is "#0066ff"
    And the pixel at 80, 11 of the window is "#0066ff"
    And the pixel at 80, 12 of the window is "<inside>"

    Examples:
      | control   | inside  |
      | Button    | #e0e0e0 |
      | ComboBox  | #e0e0e0 |
      | SpinBox   | #ffffff |
      | TextField | #ffffff |

  Scenario Outline: The frame of a <control> goes around all of it
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
          width: 200; height: 60
          color: "#ffffff"
          <control> { objectName: "c"; x: 10; y: 10; width: 140; height: 40 }
      }
      """
    When the user presses "Tab"
    Then the pixel at 10, 30 of the window is "#0066ff"
    And the pixel at 11, 30 of the window is "#0066ff"
    And the pixel at 12, 30 of the window is "<inside>"

    Examples:
      | control   | inside  |
      | Button    | #e0e0e0 |
      | ComboBox  | #e0e0e0 |
      | TextField | #ffffff |

  Scenario: The frame of a spin box is around its number and leaves its buttons out
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
          width: 200; height: 60
          color: "#ffffff"
          SpinBox { objectName: "c"; x: 10; y: 10; width: 140; height: 40 }
      }
      """
    When the user presses "Tab"
    Then the pixel at 10, 30 of the window is "#e0e0e0"
    And the pixel at 12, 12 of the window is "#e0e0e0"

  Scenario Outline: A <control> that has no keyboard has the edge <edge>
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
          width: 200; height: 60
          color: "#ffffff"
          <control> { objectName: "c"; x: 10; y: 10; width: 140; height: 40 }
      }
      """
    Then the pixel at 80, 10 of the window is "<edge>"
    And the pixel at 80, 11 of the window is "<inside>"

    Examples:
      | control   | edge    | inside  |
      | Button    | #e0e0e0 | #e0e0e0 |
      | ComboBox  | #e0e0e0 | #e0e0e0 |
      | SpinBox   | #e0e0e0 | #ffffff |
      | TextField | #bdbdbd | #ffffff |

  Scenario: A text area shows no frame where the keyboard is
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
          width: 200; height: 60
          color: "#ffffff"
          TextArea { objectName: "c"; x: 10; y: 10; width: 140; height: 40 }
      }
      """
    When the user presses "Tab"
    Then the property "activeFocus" of "c" is true
    And the pixel at 80, 10 of the window is "#ffffff"
    And the pixel at 10, 30 of the window is "#ffffff"

  Scenario: A click on a combo box brings the keyboard and no frame
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
          width: 200; height: 60
          color: "#ffffff"
          ComboBox { objectName: "c"; x: 10; y: 10; width: 140; height: 40 }
      }
      """
    When the user touches 30, 30
    And the user presses "Escape"
    Then the property "activeFocus" of "c" is true
    And the pixel at 80, 10 of the window is "#e0e0e0"
