# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-page.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Page
  A page is a pane with a header above its content and a footer below it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        Page {
          objectName: "page"
          width: 200; height: 150
          padding: 5; spacing: 3
          title: "Settings"
          header: ToolBar { objectName: "head"; ToolButton {} }
          footer: Rectangle { objectName: "foot"; implicitHeight: 19 }
          Rectangle { objectName: "kid"; implicitWidth: 30; implicitHeight: 20 }
        }
        Page { objectName: "empty"; y: 160 }
      }
      """

  @api-QtQuick.Controls.Page.header @api-QtQuick.Controls.Page.footer
  Scenario Outline: The header is at the top and the footer at the bottom, as wide as the page
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object | property          | value  |
      | head   | x                 | 0      |
      | head   | y                 | 0      |
      | head   | width             | 200    |
      | head   | height            | 40     |
      | head   | z                 | 1      |
      | head   | parent.objectName | "page" |
      | foot   | x                 | 0      |
      | foot   | y                 | 131    |
      | foot   | width             | 200    |
      | foot   | height            | 19     |
      | foot   | parent.objectName | "page" |

  Scenario Outline: The content lies between them, inside the padding and the spacing
    Then the property "<property>" of "page" is <value>

    Examples:
      | property           | value |
      | contentItem.x      | 5     |
      | contentItem.y      | 48    |
      | contentItem.width  | 190   |
      | contentItem.height | 75    |
      | availableWidth     | 190   |
      | availableHeight    | 140   |
      | contentWidth       | 30    |
      | contentHeight      | 20    |

  @api-QtQuick.Controls.Page.implicitHeaderHeight @api-QtQuick.Controls.Page.implicitFooterHeight
  Scenario Outline: The page is as large as its bars and its content need
    Then the property "<property>" of "page" is <value>

    Examples:
      | property             | value      |
      | implicitHeaderWidth  | 40         |
      | implicitHeaderHeight | 40         |
      | implicitFooterWidth  | 0          |
      | implicitFooterHeight | 19         |
      | implicitWidth        | 40         |
      | implicitHeight       | 95         |
      | title                | "Settings" |

  Scenario: What is put into a page becomes a child of its content item
    Then the property "parent.parent.objectName" of "kid" is "page"

  Scenario: A hidden header gives its room to the content
    When the property "visible" of "head" is set to false
    Then the property "contentItem.y" of "page" is 5
    And the property "contentItem.height" of "page" is 118
    And the property "implicitHeight" of "page" is 52

  Scenario: The bars follow the size of the page
    When the property "width" of "page" is set to 260
    And the property "height" of "page" is set to 180
    Then the property "width" of "head" is 260
    And the property "y" of "foot" is 161

  Scenario Outline: An empty page has no size
    Then the property "<property>" of "empty" is <value>

    Examples:
      | property         | value     |
      | implicitWidth    | 0         |
      | implicitHeight   | 0         |
      | padding          | 0         |
      | header           | null      |
      | footer           | null      |
      | title            | ""        |
      | background.color | "#ffffff" |
