# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-contextmenu.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.Controls.ContextMenu.menu
Feature: A context menu from the templates
  `ContextMenu` is in QtQuick.Templates as well as in QtQuick.Controls,
  so a control written on the templates alone can attach its menu.

  Scenario: A file that imports only the templates attaches a menu
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Templates as T
      Item {
        id: i; objectName: "root"
        T.ContextMenu.menu: T.Menu { objectName: "m" }
        property string menu: T.ContextMenu.menu.objectName
      }
      """
    Then the property "menu" of "root" is "m"
