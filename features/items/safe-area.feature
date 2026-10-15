# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-safearea.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Safe area
  `SafeArea.margins` says how far an item reaches into what is unsafe
  to draw in; `SafeArea.additionalMargins` is what an item keeps free
  itself. A platform that covers nothing leaves every margin at 0.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200
        height: 200
        property real rootTop: SafeArea.margins.top
        property real rootLeft: SafeArea.margins.left
        property real rootRight: SafeArea.margins.right
        property real rootBottom: SafeArea.margins.bottom
        property real ownTop: kept.SafeArea.margins.top
        property real ownLeft: kept.SafeArea.margins.left
        property real ownBottom: kept.SafeArea.margins.bottom
        property real nested: nestedChild.SafeArea.margins.top
        property bool identical: kept.SafeArea === kept.SafeArea
        Item {
          id: kept
          width: 100
          height: 100
          SafeArea.additionalMargins.top: 7
          SafeArea.additionalMargins.left: 3
          Item { id: nestedChild }
        }
      }
      """

  @api-QtQuick.SafeArea.margins
  Scenario Outline: Without a host the window's margins are 0
    Then the property "<property>" of "root" is <value>

    Examples:
      | property   | value |
      | rootTop    | 0     |
      | rootLeft   | 0     |
      | rootRight  | 0     |
      | rootBottom | 0     |

  @api-QtQuick.SafeArea.additionalMargins
  Scenario Outline: An item's own additional margins are its margins
    Then the property "<property>" of "root" is <value>

    Examples:
      | property  | value |
      | ownTop    | 7     |
      | ownLeft   | 3     |
      | ownBottom | 0     |

  @api-QtQuick.SafeArea.additionalMargins
  Scenario: What an item keeps free is not passed to the items inside it
    Then the property "nested" of "root" is 0

  @api-QtQuick.SafeArea.margins
  Scenario: The attached object is one per item
    Then the property "identical" of "root" is true
