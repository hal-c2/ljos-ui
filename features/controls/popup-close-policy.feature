# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#closePolicy-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#modal-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: When a popup closes by itself
  The close policy says which presses and releases beside a popup close
  it, and whether Escape does. A modal popup keeps the pointer from what
  is under it; another popup lets a press beside it through.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root
        objectName: "root"
        width: 400; height: 400
        property int clicks: 0
        MouseArea { anchors.fill: parent; onClicked: root.clicks++ }
        Item {
          x: 100; y: 100; width: 200; height: 200
          Popup { objectName: "p"; x: 50; y: 50; width: 100; height: 100 }
        }
      }
      """

  @api-QtQuick.Controls.Popup.closePolicy
  Scenario Outline: A popup that is not modal and a touch <where>, closing on <policy>
    When the property "closePolicy" of "p" is set to <flags>
    And the method "open" of "p" is called
    And the user touches <x>, <y>
    Then the property "visible" of "p" is <open>
    And the property "clicks" of "root" is <clicks>

    Examples:
      | policy                 | flags | where              | x   | y   | open  | clicks |
      | nothing                | 0     | outside its parent | 20  | 20  | true  | 1      |
      | nothing                | 0     | on it              | 200 | 200 | true  | 0      |
      | press outside          | 1     | outside its parent | 20  | 20  | false | 1      |
      | press outside          | 1     | beside it          | 120 | 120 | false | 1      |
      | press outside          | 1     | on it              | 200 | 200 | true  | 0      |
      | press outside parent   | 2     | outside its parent | 20  | 20  | false | 1      |
      | press outside parent   | 2     | beside it          | 120 | 120 | true  | 1      |
      | release outside        | 4     | outside its parent | 20  | 20  | false | 1      |
      | release outside        | 4     | on it              | 200 | 200 | true  | 0      |
      | release outside parent | 8     | outside its parent | 20  | 20  | false | 1      |
      | release outside parent | 8     | beside it          | 120 | 120 | true  | 1      |
      | Escape                 | 16    | outside its parent | 20  | 20  | true  | 1      |

  @api-QtQuick.Controls.Popup.modal
  Scenario Outline: A modal popup closes on a touch <where>, closing on <policy>
    When the property "modal" of "p" is set to true
    And the property "closePolicy" of "p" is set to <flags>
    And the method "open" of "p" is called
    And the user touches <x>, <y>
    Then the property "visible" of "p" is false
    And the property "clicks" of "root" is 0

    Examples:
      | policy                 | flags | where              | x   | y   |
      | press outside          | 1     | outside its parent | 20  | 20  |
      | press outside          | 1     | beside it          | 120 | 120 |
      | press outside parent   | 2     | outside its parent | 20  | 20  |
      | release outside        | 4     | outside its parent | 20  | 20  |
      | release outside parent | 8     | outside its parent | 20  | 20  |

  Scenario: A popup closes on a press beside it and on Escape unless told otherwise
    Then the property "closePolicy" of "p" is 17

  Scenario Outline: The close policies have the values Qt gives them
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        objectName: "names"
        property int none: Popup.NoAutoClose
        property int press: Popup.CloseOnPressOutside
        property int pressParent: Popup.CloseOnPressOutsideParent
        property int release: Popup.CloseOnReleaseOutside
        property int releaseParent: Popup.CloseOnReleaseOutsideParent
        property int esc: Popup.CloseOnEscape
        property int both: Popup.CloseOnEscape | Popup.CloseOnReleaseOutsideParent
      }
      """
    Then the property "<property>" of "names" is <value>

    Examples:
      | property      | value |
      | none          | 0     |
      | press         | 1     |
      | pressParent   | 2     |
      | release       | 4     |
      | releaseParent | 8     |
      | esc           | 16    |
      | both          | 24    |

  Scenario: A press on the popup under the top one keeps both open
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 400
        Popup { objectName: "under"; x: 20; y: 20; width: 100; height: 100 }
        Popup { objectName: "over"; x: 200; y: 200; width: 100; height: 100 }
      }
      """
    When the method "open" of "under" is called
    And the method "open" of "over" is called
    And the user touches 50, 50
    Then the property "visible" of "under" is true
    And the property "visible" of "over" is false

  Scenario: A press beside two popups closes both
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 400
        Popup { objectName: "under"; x: 20; y: 20; width: 100; height: 100 }
        Popup { objectName: "over"; x: 200; y: 200; width: 100; height: 100 }
      }
      """
    When the method "open" of "under" is called
    And the method "open" of "over" is called
    And the user touches 350, 50
    Then the property "visible" of "under" is false
    And the property "visible" of "over" is false

  Scenario: A press beside a modal popup does not close the popup under it
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 400
        Popup { objectName: "under"; x: 20; y: 20; width: 100; height: 100 }
        Popup { objectName: "over"; x: 200; y: 200; width: 100; height: 100; modal: true }
      }
      """
    When the method "open" of "under" is called
    And the method "open" of "over" is called
    And the user touches 350, 50
    Then the property "visible" of "under" is true
    And the property "visible" of "over" is false
