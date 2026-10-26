# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#item-prop
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#sourceComponent-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A Loader that switches what it loads goes from one item to the next
  A Loader given another component destroys its item and makes the next one. Its item changes
  once, from the old item to the new one: it does not tell of an empty item in between, though
  what reads the item while the next one is being made reads null.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        Component { id: a; Item { objectName: "a"; Component.onDestruction: root.log += "-a;" } }
        Component { id: b
          Item { objectName: "b"
            Component.onCompleted: root.log += "+b(" + (l.item ? l.item.objectName : "null") + ");" } }
        Loader { id: l; sourceComponent: a
          onLoaded: root.log += "loaded;"
          onItemChanged: root.log += "item=" + (item ? item.objectName : "null") + ";"
          onStatusChanged: root.log += "status=" + status + ";" }
        function swap() { log = ""; l.sourceComponent = b }
      }
      """

  Scenario: The item changes once, straight to the new one
    When the method "swap" of "root" is called
    Then the property "log" of "root" is "-a;+b(null);item=b;loaded;"
