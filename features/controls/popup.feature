# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html
#   https://doc.qt.io/qt-6/qtquickcontrols-popups.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Popup
  A popup is shown above everything else in its window when it is opened
  and is not there when it is closed. It is as large as its content and
  its padding, and it is not a child of the item that declares it.

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
          objectName: "holder"
          x: 100; y: 50; width: 200; height: 100
          Popup {
            objectName: "p"
            x: 20; y: 30
            Rectangle { objectName: "kid"; implicitWidth: 50; implicitHeight: 30 }
            onAboutToShow: root.log += "aboutToShow "
            onAboutToHide: root.log += "aboutToHide "
            onOpened: root.log += "opened "
            onClosed: root.log += "closed "
            onVisibleChanged: root.log += "visible "
            onOpenedChanged: root.log += "openedChanged "
          }
        }
        Popup { objectName: "empty" }
      }
      """

  Scenario Outline: A popup starts closed, with the defaults of the Basic style
    Then the property "<property>" of "empty" is <value>

    Examples:
      | property       | value |
      | visible        | false |
      | opened         | false |
      | modal          | false |
      | dim            | false |
      | focus          | false |
      | padding        | 12    |
      | margins        | -1    |
      | topMargin      | -1    |
      | closePolicy    | 17    |
      | z              | 0     |
      | x              | 0     |
      | implicitWidth  | 24    |
      | implicitHeight | 24    |
      | width          | 24    |
      | spacing        | 0     |
      | enabled        | true  |
      | opacity        | 1     |
      | scale          | 1     |
      | activeFocus    | false |

  @api-QtQuick.Controls.Popup.contentItem @api-QtQuick.Controls.Popup.background
  Scenario Outline: A popup is as large as its content and its padding
    Then the property "<property>" of "p" is <value>

    Examples:
      | property                | value     |
      | contentWidth            | 50        |
      | contentHeight           | 30        |
      | implicitWidth           | 74        |
      | implicitHeight          | 54        |
      | width                   | 74        |
      | height                  | 54        |
      | availableWidth          | 50        |
      | availableHeight         | 30        |
      | contentItem.x           | 12        |
      | contentItem.y           | 12        |
      | contentItem.width       | 50        |
      | contentItem.height      | 30        |
      | background.width        | 74        |
      | background.height       | 54        |
      | background.color        | "#ffffff" |
      | background.border.color | "#353637" |
      | contentChildren.length  | 1         |
      | contentData.length      | 1         |

  @api-QtQuick.Controls.Popup.open @api-QtQuick.Controls.Popup.visible
  Scenario: Opening shows the popup
    When the method "open" of "p" is called
    Then the property "visible" of "p" is true
    And the property "opened" of "p" is true
    And the property "visible" of "kid" is true

  @api-QtQuick.Controls.Popup.close
  Scenario: Closing hides the popup
    When the method "open" of "p" is called
    And the method "close" of "p" is called
    Then the property "visible" of "p" is false
    And the property "opened" of "p" is false

  Scenario: What is in a closed popup is not visible
    Then the property "visible" of "kid" is false

  @api-QtQuick.Controls.Popup.aboutToShow @api-QtQuick.Controls.Popup.opened
  Scenario: Opening says so before and after
    When the method "open" of "p" is called
    Then the property "log" of "root" is "aboutToShow visible openedChanged opened "

  @api-QtQuick.Controls.Popup.aboutToHide @api-QtQuick.Controls.Popup.closed
  Scenario: Closing says so before and after
    When the method "open" of "p" is called
    And the property "log" of "root" is set to ""
    And the method "close" of "p" is called
    Then the property "log" of "root" is "aboutToHide openedChanged visible closed "

  Scenario: Opening an open popup does nothing
    When the method "open" of "p" is called
    And the property "log" of "root" is set to ""
    And the method "open" of "p" is called
    Then the property "log" of "root" is ""

  Scenario: Closing a closed popup does nothing
    When the method "close" of "p" is called
    Then the property "log" of "root" is ""

  Scenario: Setting visible opens the popup
    When the property "visible" of "p" is set to true
    Then the property "opened" of "p" is true

  Scenario: Clearing visible closes the popup
    When the method "open" of "p" is called
    And the property "visible" of "p" is set to false
    Then the property "opened" of "p" is false

  @api-QtQuick.Controls.Popup.parent
  Scenario: The parent of a popup is the item that declares it
    Then the property "parent.objectName" of "p" is "holder"

  Scenario: What is put into a popup becomes a child of its content item
    Then the property "parent.x" of "kid" is 12
    And the property "parent.width" of "kid" is 50
    And the property "x" of "kid" is 0

  @api-QtQuick.Controls.Popup.x @api-QtQuick.Controls.Popup.y
  Scenario: A popup is placed from the corner of its parent
    When the method "open" of "p" is called
    Then the property "contentItem.parent.x" of "p" is 120
    And the property "contentItem.parent.y" of "p" is 80
    And the property "x" of "p" is 20

  Scenario: An open popup follows its position
    When the method "open" of "p" is called
    And the property "x" of "p" is set to 40
    Then the property "contentItem.parent.x" of "p" is 140

  Scenario: A popup follows the size of its content
    When the property "implicitWidth" of "kid" is set to 100
    Then the property "width" of "p" is 124
    And the property "contentItem.width" of "p" is 100

  Scenario: A popup that is given a size keeps it
    When the property "width" of "p" is set to 200
    And the property "height" of "p" is set to 100
    Then the property "contentItem.width" of "p" is 176
    And the property "contentItem.height" of "p" is 76
    And the property "background.width" of "p" is 200
    And the property "implicitWidth" of "p" is 74
