# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menubar.html
#   https://doc.qt.io/qt-6/qml-qtqml-models-instantiator.html
# Verified on Qt 6.12.0 (qml6, offscreen).

@qt-parity
Feature: Menus an Instantiator adds to a menu bar
  The menus declared in a menu bar are its own from the start, so the
  index an `onObjectAdded` handler gives counts them, even when it runs
  before the bar is complete.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 400; height: 400
        property string titles: ""
        function look() {
          var s = []
          for (var i = 0; i < bar.count; i++) s.push(bar.menuAt(i).title)
          titles = s.join(",")
        }
        function grow() { lm.append({ name: "P3" }); look() }
        ListModel { id: lm; ListElement { name: "P1" } ListElement { name: "P2" } }
        MenuBar {
          id: bar; objectName: "bar"; width: 400
          Menu { title: "File" }
          Instantiator {
            model: lm
            delegate: Menu { title: name }
            onObjectAdded: (index, object) => bar.insertMenu(index + 1, object)
          }
          Menu { title: "End" }
        }
        Component.onCompleted: look()
      }
      """

  @api-QtQuick.Controls.MenuBar.insertMenu
  Scenario: Menus inserted while the bar is made count the declared menus
    Then the property "titles" of "root" is "File,P1,P2,End"
    And the property "count" of "bar" is 4

  @api-QtQuick.Controls.MenuBar.insertMenu
  Scenario: A row added to the model later is inserted at its place
    When the method "grow" of "root" is called
    Then the property "titles" of "root" is "File,P1,P2,P3,End"
