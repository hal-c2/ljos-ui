# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#visible-prop
#   https://doc.qt.io/qt-6/qml-qtquick-animation.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Animations of items that are not shown
  Hiding an item does not stop its animations: values, signals and scripts arrive as they would
  for a visible item.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log
        Item {
          id: page; objectName: "page"; visible: false
          Rectangle { id: box; objectName: "box" }
        }
        NumberAnimation {
          objectName: "anim"; target: box; property: "x"; from: 0; to: 100; duration: 200
          onFinished: log += "finished "
        }
        SequentialAnimation {
          objectName: "seq"
          NumberAnimation { target: box; property: "y"; from: 0; to: 100; duration: 100 }
          ScriptAction { script: log += "script:" + box.y + " " }
          PauseAnimation { duration: 100 }
        }
        NumberAnimation {
          objectName: "forever"; target: box; property: "width"; from: 0; to: 100; duration: 200
          loops: Animation.Infinite
        }
      }
      """

  Scenario: An animation of a hidden item reaches its end and reports finished
    When the method "start" of "anim" is called
    And 1 second passes
    Then the property "x" of "box" is 100
    And the property "log" of "root" is "finished "
    And the property "running" of "anim" is false

  Scenario: A script in a sequence on a hidden item runs
    When the method "start" of "seq" is called
    And 1 second passes
    Then the property "log" of "root" is "script:100 "

  Scenario: An item shown after its animation ended is where the animation left it
    When the method "start" of "anim" is called
    And 1 second passes
    And the property "visible" of "page" is set to true
    Then the property "x" of "box" is 100

  Scenario: An endless animation of a hidden item keeps running
    When the method "start" of "forever" is called
    And 1 second passes
    Then the property "running" of "forever" is true
