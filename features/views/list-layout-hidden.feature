# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A hidden ListView, or one in a Loader, lays out its rows with the frame
  A view that is not shown, or that a Loader made, lays out the rows its model gave it
  when the scene is next polished, as a shown view does: until then the length of its
  content is what its last layout made.

  Scenario: A hidden view and a view in a Loader are laid out with the frame too
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 100; height: 200
        property string seen
        ListModel { id: m }
        Component {
          id: list
          ListView {
            width: 100; height: 100
            model: m
            delegate: Rectangle { width: 100; height: 20 }
          }
        }
        Loader { id: shown; sourceComponent: list }
        Loader { id: hidden; y: 100; visible: false; sourceComponent: list }
        function add() {
          m.append({}); m.append({})
          seen = shown.item.contentHeight + "," + hidden.item.contentHeight
        }
        function now() { seen = shown.item.contentHeight + "," + hidden.item.contentHeight }
      }
      """
    When the method "add" of "root" is called
    Then the property "seen" of "root" is "0,0"
    When 20 milliseconds pass
    And the method "now" of "root" is called
    Then the property "seen" of "root" is "40,40"
