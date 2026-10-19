# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-behavior.html
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#implicitWidth-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A Behavior's start gives the property the value it starts from
  An item's width that was never given a value follows its implicit width. A Behavior on it
  starts its animation at the width the item shows, and that start gives the item the width as
  its own: from then on the implicit width no longer moves it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property real seen: -1
        property real seenStill: -1
        Rectangle {
          id: box
          implicitWidth: 50
          Behavior on width { NumberAnimation { duration: 1000 } }
        }
        Rectangle { id: still; implicitWidth: 50 }
        function give() {
          box.width = 200
          box.implicitWidth = 80
          seen = box.width
          still.implicitWidth = 80
          seenStill = still.width
        }
        function giveTwice() {
          box.width = 50
          box.width = 200
          box.implicitWidth = 80
          seen = box.width
        }
      }
      """

  Scenario: The start gives a width that followed the implicit width the value it shows
    When the method "give" of "root" is called
    Then the property "seen" of "root" is 50
    And the property "seenStill" of "root" is 80

  Scenario: A width given the value it shows, then another, keeps the one it started from
    When the method "giveTwice" of "root" is called
    Then the property "seen" of "root" is 50
