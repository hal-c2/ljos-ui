# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#visible-prop
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#completed-signal
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Visibility signal parity

  Scenario: A child inside a statically hidden parent reads visible as false in Component.onCompleted with no signal
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property bool seenInCompleted: true
        property int childSignals: 0
        Item {
          id: hiddenParent; objectName: "hiddenParent"; visible: false
          Item {
            id: child; objectName: "child"
            onVisibleChanged: root.childSignals++
            Component.onCompleted: root.seenInCompleted = visible
          }
        }
      }
      """
    Then the property "seenInCompleted" of "root" is false
    And the property "childSignals" of "root" is 1
    When the property "visible" of "hiddenParent" is set to true
    Then the property "visible" of "child" is true
    And the property "childSignals" of "root" is 2

  Scenario: Showing an ancestor notifies deeper items before their parents
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        function step() { a.visible = false; b.visible = true; a.visible = true }
        Item {
          id: a; objectName: "a"; visible: true
          onVisibleChanged: root.log += "a:" + visible + ";"
          Item {
            id: b; objectName: "b"; visible: false
            onVisibleChanged: root.log += "b:" + visible + ";"
            Item {
              id: c; objectName: "c"; visible: true
              onVisibleChanged: root.log += "c:" + visible + ";"
            }
          }
        }
      }
      """
    When the method "step" of "root" is called
    Then the property "log" of "root" is "c:false;a:false;c:true;b:true;a:true;"
    And the property "visible" of "c" is true

  Scenario: Showing sibling subtrees uses post-order in child order
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        function show() { log = ""; branch.visible = true }
        Item {
          id: branch; objectName: "branch"; visible: false
          onVisibleChanged: root.log += "branch:" + visible + ";"
          Item {
            id: left; objectName: "left"
            onVisibleChanged: root.log += "left:" + visible + ";"
            Item { id: leftLeaf; onVisibleChanged: root.log += "leftLeaf:" + visible + ";" }
          }
          Item {
            id: right; objectName: "right"
            onVisibleChanged: root.log += "right:" + visible + ";"
            Item { id: rightLeaf; onVisibleChanged: root.log += "rightLeaf:" + visible + ";" }
          }
        }
      }
      """
    When the method "show" of "root" is called
    Then the property "log" of "root" is "leftLeaf:true;left:true;rightLeaf:true;right:true;branch:true;"

  Scenario: Hiding and showing three levels notifies descendants first in both directions
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        function step() { a.visible = false; a.visible = true }
        Item {
          id: a; objectName: "a"
          onVisibleChanged: root.log += "a:" + visible + ";"
          Item {
            id: b; objectName: "b"
            onVisibleChanged: root.log += "b:" + visible + ";"
            Item { id: c; onVisibleChanged: root.log += "c:" + visible + ";" }
          }
        }
      }
      """
    When the method "step" of "root" is called
    Then the property "log" of "root" is "c:false;b:false;a:false;c:true;b:true;a:true;"

  Scenario: Showing a hidden root notifies its descendants before the root
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; visible: false
        property string log: ""
        function show() { log = ""; root.visible = true }
        onVisibleChanged: root.log += "root:" + visible + ";"
        Item {
          id: child; objectName: "child"
          onVisibleChanged: root.log += "child:" + visible + ";"
          Item { id: grandchild; onVisibleChanged: root.log += "grandchild:" + visible + ";" }
        }
      }
      """
    When the method "show" of "root" is called
    Then the property "log" of "root" is "grandchild:true;child:true;root:true;"

  Scenario: Reparenting a subtree into a visible parent notifies descendants first
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        function move() { log = ""; moving.parent = target }
        Item { id: target }
        Item {
          id: source; visible: false
          Item {
            id: moving; objectName: "moving"
            onVisibleChanged: root.log += "moving:" + visible + ";"
            Item { id: leaf; objectName: "leaf"; onVisibleChanged: root.log += "leaf:" + visible + ";" }
          }
        }
      }
      """
    When the method "move" of "root" is called
    Then the property "log" of "root" is "leaf:true;moving:true;"
    And the property "visible" of "leaf" is true
