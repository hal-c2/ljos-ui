# Sources:
#   https://doc.qt.io/qt-6/qtqml-documents-definetypes.html
#   https://doc.qt.io/qt-6/qtqml-uiforms.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Types written in a .ui.qml file
  Qt Design Studio writes a form as `NameForm.ui.qml`, and the file next to it
  builds on it as `NameForm`. The type is called by the name before `.ui.qml`:
  a property declared with that type takes an object of the form, or of a type
  built on it. Printed, the object keeps `.ui` in its class name.

  Background:
    Given the file "PanelForm.ui.qml":
      """qml
      import QtQuick
      Item { property int level: 1 }
      """
    And the file "Panel.qml":
      """qml
      import QtQuick
      PanelForm { level: 2 }
      """
    And the file "Holder.qml":
      """qml
      import QtQuick
      Item { property PanelForm panel }
      """

  Scenario: A script puts an object of the form in a property of its type
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property PanelForm held
        property int level: held ? held.level : 0
        PanelForm { id: form }
        Component.onCompleted: held = form
      }
      """
    Then the property "level" of "root" is 1

  Scenario: A script puts an object built on the form in a property of its type
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property PanelForm held
        property int level: held ? held.level : 0
        Panel { id: panel }
        Component.onCompleted: held = panel
      }
      """
    Then the property "level" of "root" is 2

  Scenario: A binding gives the property of another file an object built on the form
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int level: holder.panel ? holder.panel.level : 0
        Panel { id: panel }
        Holder { id: holder; panel: panel }
      }
      """
    Then the property "level" of "root" is 2

  Scenario: The object's class name keeps .ui
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string printed: String(form).replace(/_QMLTYPE.*/, "")
        PanelForm { id: form }
      }
      """
    Then the property "printed" of "root" is "PanelForm.ui"
