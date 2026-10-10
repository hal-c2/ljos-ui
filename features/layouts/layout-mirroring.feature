# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-layoutmirroring.html
#   https://doc.qt.io/qt-6/qtquick-positioning-righttoleft.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: LayoutMirroring
  LayoutMirroring swaps left and right for anchors, positioners and layouts.
  It never touches an x that was set directly.

  Rule: Anchors

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "top"
          width: 200; height: 100
          property bool flip: true
          LayoutMirroring.enabled: flip
          LayoutMirroring.childrenInherit: true
          Item { id: c; objectName: "c"; width: 40; anchors.left: parent.left; anchors.leftMargin: 7 }
          Item { objectName: "d"; width: 30; anchors.left: c.right; anchors.leftMargin: 5 }
          Item { objectName: "e"; width: 30; anchors.right: c.left; anchors.rightMargin: 2 }
          Item {
            objectName: "f"; width: 30
            anchors.horizontalCenter: c.left; anchors.horizontalCenterOffset: 4
          }
          Item {
            objectName: "g"
            anchors.left: parent.left; anchors.leftMargin: 20
            anchors.horizontalCenter: parent.horizontalCenter
          }
          Item { objectName: "raw"; x: 12; width: 10 }
        }
        """
      When 100 milliseconds pass

    Scenario Outline: Left and right anchors, their margins and the centre offset are swapped
      Then the property "x" of "<item>" is <x>
      And the property "width" of "<item>" is <width>

      Examples:
        | item | x   | width |
        | c    | 153 | 40    |
        | d    | 118 | 30    |
        | e    | 195 | 30    |
        | f    | 174 | 30    |
        | g    | 20  | 160   |
        | raw  | 12  | 10    |

    Scenario Outline: Switching mirroring off puts the anchors back
      When the property "flip" of "top" is set to false
      And 100 milliseconds pass
      Then the property "x" of "<item>" is <x>

      Examples:
        | item | x   |
        | c    | 7   |
        | d    | 52  |
        | e    | -25 |
        | f    | -4  |
        | raw  | 12  |

  Rule: Who is mirrored

    Scenario Outline: Children inherit mirroring only through childrenInherit, and their own setting wins
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 100
          LayoutMirroring.enabled: true
          LayoutMirroring.childrenInherit: true
          Item {
            objectName: "off"; width: 200; height: 50
            LayoutMirroring.enabled: false
            anchors.left: parent.left
            Item { objectName: "below"; width: 40; anchors.left: parent.left }
          }
          Item {
            objectName: "alone"; width: 100; height: 50
            LayoutMirroring.childrenInherit: false
            anchors.left: parent.left
            Item { objectName: "under"; width: 40; anchors.left: parent.left }
          }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>

      Examples:
        | item  | x   |
        | off   | 0   |
        | below | 160 |
        | alone | 200 |
        | under | 60  |

    Scenario Outline: Mirroring without childrenInherit stays on the item itself
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 100
          Item {
            objectName: "self"; width: 200; height: 50
            LayoutMirroring.enabled: true
            anchors.left: parent.left
            Item {
              objectName: "child"; width: 40
              LayoutMirroring.childrenInherit: true
              anchors.left: parent.left
              Item { objectName: "grandchild"; width: 10; anchors.left: parent.left }
            }
          }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>

      Examples:
        | item       | x   |
        | self       | 100 |
        | child      | 0   |
        | grandchild | 0   |

  Rule: Positioners

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "top"; property bool flip: true
          LayoutMirroring.enabled: flip; LayoutMirroring.childrenInherit: true
          Row {
            objectName: "row"; padding: 5; rightPadding: 20; spacing: 2
            Item { objectName: "r1"; width: 40; height: 20 }
            Item { objectName: "r2"; width: 30; height: 20 }
          }
          Row {
            objectName: "rtl"; layoutDirection: Qt.RightToLeft
            Item { objectName: "t1"; width: 40; height: 20 }
            Item { objectName: "t2"; width: 30; height: 20 }
          }
          Flow {
            objectName: "flow"; width: 100
            Item { objectName: "f1"; width: 40; height: 20 }
            Item { objectName: "f2"; width: 30; height: 20 }
          }
          Grid {
            objectName: "grid"; columns: 2
            Item { objectName: "g1"; width: 40; height: 20 }
            Item { objectName: "g2"; width: 30; height: 20 }
          }
        }
        """
      When 100 milliseconds pass

    Scenario Outline: A mirrored positioner runs the other way and keeps its paddings
      Then the property "x" of "<item>" is <x>

      Examples:
        | item | x  |
        | r1   | 37 |
        | r2   | 5  |
        | t1   | 0  |
        | t2   | 40 |
        | f1   | 60 |
        | f2   | 30 |
        | g1   | 30 |
        | g2   | 0  |

    Scenario Outline: The effective direction says which way a positioner runs
      Then the property "layoutDirection" of "<positioner>" is <set>
      And the property "effectiveLayoutDirection" of "<positioner>" is <effective>

      Examples:
        | positioner | set | effective |
        | row        | 0   | 1         |
        | rtl        | 1   | 0         |
        | flow       | 0   | 1         |
        | grid       | 0   | 1         |

    Scenario Outline: Switching mirroring off runs the positioners forward again
      When the property "flip" of "top" is set to false
      And 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "effectiveLayoutDirection" of "row" is 0

      Examples:
        | item | x  |
        | r1   | 5  |
        | r2   | 47 |
        | t1   | 30 |
        | f2   | 40 |
        | g2   | 40 |

  Rule: Layouts

    Scenario Outline: A mirrored layout runs right to left, and right-to-left mirrored runs left to right
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        Item {
          LayoutMirroring.enabled: true
          LayoutMirroring.childrenInherit: true
          RowLayout {
            width: 200
            Item { objectName: "a"; width: 40; height: 20 }
            Item { objectName: "b"; height: 20; Layout.fillWidth: true }
          }
          RowLayout {
            width: 200; layoutDirection: Qt.RightToLeft
            Item { objectName: "c"; width: 40; height: 20 }
            Item { objectName: "d"; height: 20; Layout.fillWidth: true }
          }
          GridLayout {
            width: 200; columns: 2
            Item { objectName: "e"; width: 40; height: 20 }
            Item { objectName: "f"; height: 20; Layout.fillWidth: true }
            Item { objectName: "g"; width: 10; height: 20 }
          }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>

      Examples:
        | item | x   |
        | a    | 160 |
        | b    | 0   |
        | c    | 0   |
        | d    | 45  |
        | e    | 160 |
        | f    | 0   |
        | g    | 190 |
