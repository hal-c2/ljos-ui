# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#closePolicy-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#modal-prop

# The oracle hears that a touch has arrived through the window's content,
# which a modal popup that stays open cuts off, and it sends whole clicks
# only. These scenarios follow the documentation and run on Ljós only.
@ljos-only
Feature: A popup and the pointer, where the oracle cannot look
  A modal popup that stays open keeps the pointer from what is under it.
  Closing on a press happens when the pointer goes down, closing on a
  release when it goes up.

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

  @api-QtQuick.Controls.Popup.modal
  Scenario Outline: A modal popup stays open on a touch <where>, closing on <policy>
    When the property "modal" of "p" is set to true
    And the property "closePolicy" of "p" is set to <flags>
    And the method "open" of "p" is called
    And the user touches <x>, <y>
    Then the property "visible" of "p" is true
    And the property "clicks" of "root" is 0

    Examples:
      | policy                 | flags | where              | x   | y   |
      | press outside          | 1     | on it              | 200 | 200 |
      | press outside parent   | 2     | beside it          | 120 | 120 |
      | release outside parent | 8     | beside it          | 120 | 120 |
      | Escape                 | 16    | outside its parent | 20  | 20  |
      | nothing                | 0     | outside its parent | 20  | 20  |

  @api-QtQuick.Controls.Popup.closePolicy
  Scenario: A popup that closes on a release is still open while the pointer is down
    When the property "closePolicy" of "p" is set to 4
    And the method "open" of "p" is called
    And the user presses the pointer at 20, 20
    Then the property "visible" of "p" is true

  Scenario: A popup that closes on a release closes when the pointer goes up
    When the property "closePolicy" of "p" is set to 4
    And the method "open" of "p" is called
    And the user presses the pointer at 20, 20
    And the user releases the pointer at 20, 20
    Then the property "visible" of "p" is false

  Scenario: A popup that closes on a press closes while the pointer is still down
    When the method "open" of "p" is called
    And the user presses the pointer at 20, 20
    Then the property "visible" of "p" is false
