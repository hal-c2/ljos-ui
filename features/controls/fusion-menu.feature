# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-fusion.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menu.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menuitem.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menuseparator.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.Controls.Menu @api-QtQuick.Controls.MenuItem
@api-QtQuick.Controls.MenuSeparator
Feature: The Fusion style's menu
  A Fusion menu is in the base colour inside a thin border, and its items
  are as tall as their text, in the text colour, marked in the highlight
  colour only while they are highlighted.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Fusion
      Item {
        objectName: "root"
        width: 300; height: 200
        property Menu menu: m
        property MenuItem plain: a
        property MenuItem marked: b
        property MenuItem check: c
        property MenuSeparator line: s
        Component.onCompleted: { m.open(); m.currentIndex = 1 }
        Menu {
          id: m
          palette.window: "#405060"
          palette.base: "#102030"
          palette.text: "#ffffff"
          palette.highlight: "#00ff00"
          palette.highlightedText: "#ff0000"
          MenuItem { id: a; text: "Open File" }
          MenuItem { id: b; text: "Open URL" }
          MenuSeparator { id: s }
          MenuItem { id: c; text: "Check"; checkable: true; checked: true }
        }
      }
      """

  Scenario Outline: The <property> of the menu is <value>
    Then the property "menu.<property>" of "root" is <value>

    Examples:
      | property                  | value     |
      | implicitWidth             | 200       |
      | padding                   | 1         |
      | margins                   | 0         |
      | overlap                   | 2         |
      | background.color          | "#102030" |
      | background.border.color   | "#2e3945" |
      | background.border.width   | 1         |
      | background.implicitHeight | 20        |

  Scenario Outline: The <property> of the <item> item is <value>
    Then the property "<item>.<property>" of "root" is <value>

    Examples:
      | item   | property             | value     |
      | plain  | implicitWidth        | 200       |
      | plain  | implicitHeight       | 26        |
      | plain  | padding              | 6         |
      | plain  | spacing              | 6         |
      | plain  | contentItem.color    | "#ffffff" |
      | plain  | background.visible   | false     |
      | marked | contentItem.color    | "#ff0000" |
      | marked | background.visible   | true      |
      | marked | background.color     | "#00ff00" |
      | check  | implicitHeight       | 26        |
      | check  | indicator.width      | 14        |
      | check  | indicator.height     | 14        |

  Scenario Outline: The <property> of a separator is <value>
    Then the property "line.<property>" of "root" is <value>

    Examples:
      | property                  | value       |
      | implicitHeight            | 3           |
      | leftPadding               | 5           |
      | topPadding                | 1           |
      | contentItem.implicitWidth | 188         |
      | contentItem.color         | "#3c000000" |
