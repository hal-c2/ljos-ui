# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#active-prop
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#item-prop
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#destruction-signal
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (loaders), seeds 101 and 202: buckets value:onCompleted:text,
# value:onChanged+onCompleted:text and value:log:empty-ljos (see
# crates/ljos_fuzz/known/loaders.txt).

@qt-parity
Feature: The order in which a Loader tells of its object
  A Loader turned off destroys its object and tells that its item and then its status changed before it
  tells that it is no longer active. One turned on with nothing to load still tells that its
  item changed.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        Component { id: c
          Item { objectName: "m"
            Component.onCompleted: root.log += "+m;"
            Component.onDestruction: root.log += "-m;" } }
        Loader { id: l; objectName: "l"; sourceComponent: c
          onLoaded: root.log += "loaded;"
          onItemChanged: root.log += "item=" + (item ? item.objectName : "null") + ";"
          onStatusChanged: root.log += "status=" + status + ";"
          onActiveChanged: root.log += "active=" + active + ";" }
        Loader { id: e; objectName: "e"; active: false
          onItemChanged: root.log += "e.item=" + (item ? item.objectName : "null") + ";" }
        function off() { log = ""; l.active = false }
        function onEmpty() { log = ""; e.active = true }
      }
      """

  @backlog
  Scenario: A Loader turned off tells of its item and status before it is inactive
    When the method "off" of "root" is called
    Then the property "log" of "root" is "-m;item=null;status=0;active=false;"

  @backlog
  Scenario: A Loader with nothing to load tells that its item changed when turned on
    When the method "onEmpty" of "root" is called
    Then the property "log" of "root" is "e.item=null;"
