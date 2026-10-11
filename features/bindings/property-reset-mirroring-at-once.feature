# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-layoutmirroring.html
#   https://doc.qt.io/qt-6/qml-qtquick-layoutmirroring.html#enabled-attached-prop
#
# Qt 6.11.2 gives "false,0,40" after the value is taken back too: an item
# that does not itself pass mirroring on is left as it was until an
# ancestor next passes something on, so it stays unmirrored below the
# mirrored ancestor it now follows. With childrenInherit on the item Qt
# follows the ancestors at once (property-reset-mirroring.feature). Here an
# item is mirrored by what its ancestors say from the moment it stops
# having a value of its own; there is no state in between to keep.

@qt-diverges
Feature: An item whose mirroring is taken back follows its ancestors at once
  Assigning undefined to LayoutMirroring.enabled takes the item's own
  value back. From then on the item is mirrored when an ancestor passes
  mirroring on, whether or not it passes mirroring on itself.

  Scenario: An item that does not pass mirroring on is mirrored as its ancestors say at once
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        width: 200; height: 100
        property string seen
        LayoutMirroring.enabled: true
        LayoutMirroring.childrenInherit: true
        Item {
          id: kid
          width: 50; height: 50
          anchors.left: parent.left
          LayoutMirroring.enabled: false
          Rectangle { id: leaf; width: 10; height: 10; anchors.left: parent.left }
        }
        function look() { seen = kid.LayoutMirroring.enabled + "," + kid.x + "," + leaf.x }
        function back() { kid.LayoutMirroring.enabled = undefined; look() }
      }
      """
    When the method "look" of "root" is called
    Then the property "seen" of "root" is "false,0,40"
    When the method "back" of "root" is called
    Then the property "seen" of "root" is "true,150,40"
