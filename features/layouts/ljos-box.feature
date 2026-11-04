# Sources:
#   docs/DIALECT.md (Ljós's own API, the compat pass, the checker's rules)
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-rowlayout.html
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html
#   https://doc.qt.io/qt-6/qml-qtquick-row.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# `Ljos.Ui`'s `Row`, `Column` and `Box` are Ljós's own: one arrangement in the place of Qt's
# positioners and layouts. Where its rule and RowLayout's agree the RowLayout as written is the
# oracle, and the Ljós file is held to its numbers. The turning box is calqlatr's shape (a display
# that fills over a pad that keeps its size, corpus/qtdoc/examples/demos/calqlatr/Main.qml), written
# here as a case of its own.

Feature: Ljós's one arrangement: Row, Column and Box
  A Row of Ljós's API lays its children out side by side, a Column one
  under another, and a Box either way, as its `direction` says. A child
  asks for its size with the `Layout` it is given: its preferred size if
  it has one, else the width or height its file gives it, else its
  implicit size; and it fills, and is held to a minimum and a maximum, as
  in Qt's layouts. A box of children that only have sizes places them as
  a positioner would, and costs what a positioner costs.

  Rule: A Row agrees with RowLayout where their rules agree

    @qt-parity
    Scenario Outline: A RowLayout of fixed children and one that fills
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          objectName: "row"
          width: 400
          Rectangle { objectName: "a"; implicitWidth: 40; implicitHeight: 30 }
          Rectangle { objectName: "b"; Layout.fillWidth: true; implicitHeight: 30 }
          Rectangle { objectName: "c"; implicitWidth: 60; implicitHeight: 30 }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "width" of "<item>" is <width>
      And the property "height" of "row" is 30

      Examples:
        | item | x   | width |
        | a    | 0   | 40    |
        | b    | 45  | 290   |
        | c    | 340 | 60    |

    @ljos-only
    Scenario Outline: The same children in a Ljós Row
      Given the QML:
        """qml
        import Ljos.Ui
        Row {
          objectName: "row"
          width: 400
          spacing: 5
          Rectangle { objectName: "a"; implicitWidth: 40; implicitHeight: 30 }
          Rectangle { objectName: "b"; Layout.fillWidth: true; implicitHeight: 30 }
          Rectangle { objectName: "c"; implicitWidth: 60; implicitHeight: 30 }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "width" of "<item>" is <width>
      And the property "height" of "row" is 30

      Examples:
        | item | x   | width |
        | a    | 0   | 40    |
        | b    | 45  | 290   |
        | c    | 340 | 60    |

    # Qt's RowLayout shares room out in whole pixels: three children that fill 100 are 33, 34
    # and 33 wide (qml6). Ljós's Row does too, so a filled bar's edges fall on pixels.
    @qt-parity
    Scenario Outline: Room shared out is shared in whole pixels
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          width: 100
          spacing: 0
          Rectangle { objectName: "a"; Layout.fillWidth: true; implicitHeight: 10 }
          Rectangle { objectName: "b"; Layout.fillWidth: true; implicitHeight: 10 }
          Rectangle { objectName: "c"; Layout.fillWidth: true; implicitHeight: 10 }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "width" of "<item>" is <width>

      Examples:
        | item | x  | width |
        | a    | 0  | 33    |
        | b    | 33 | 34    |
        | c    | 67 | 33    |

    @ljos-only
    Scenario Outline: The same room shared out by a Ljós Row
      Given the QML:
        """qml
        import Ljos.Ui
        Row {
          width: 100
          Rectangle { objectName: "a"; Layout.fillWidth: true; implicitHeight: 10 }
          Rectangle { objectName: "b"; Layout.fillWidth: true; implicitHeight: 10 }
          Rectangle { objectName: "c"; Layout.fillWidth: true; implicitHeight: 10 }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "width" of "<item>" is <width>

      Examples:
        | item | x  | width |
        | a    | 0  | 33    |
        | b    | 33 | 34    |
        | c    | 67 | 33    |

  Rule: What a child's file says is what it asks for

    # Qt's RowLayout gives a child with an implicit width that width, whatever width its file
    # gives it (an Item of implicit width 80 and width 50 is 80 wide in it, qml6); its Row keeps
    # the 50. Ljós has one rule: the width the file gives is the request.
    @ljos-only @qt-diverges
    Scenario: An explicit width is the request
      Given the QML:
        """qml
        import Ljos.Ui
        Row {
          objectName: "row"
          width: 400
          Item { objectName: "a"; implicitWidth: 80; width: 50; height: 20 }
          Item { objectName: "b"; implicitWidth: 30; implicitHeight: 20 }
          Rectangle { objectName: "c"; Layout.fillWidth: true; implicitHeight: 20 }
        }
        """
      When 100 milliseconds pass
      Then the property "width" of "a" is 50
      And the property "x" of "b" is 50
      And the property "width" of "b" is 30
      And the property "x" of "c" is 80
      And the property "width" of "c" is 320

    @ljos-only
    Scenario: A box of children that only have sizes packs them at the start, as a Row positioner does
      Given the QML:
        """qml
        import Ljos.Ui
        Row {
          objectName: "row"
          width: 400
          height: 50
          spacing: 4
          padding: 3
          Item { objectName: "a"; width: 20.5; height: 10 }
          Item { objectName: "skipped"; width: 0; height: 10 }
          Item { objectName: "b"; width: 30; height: 40 }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "a" is 3
      And the property "y" of "a" is 3
      And the property "x" of "b" is 27.5
      And the property "y" of "b" is 3
      And the property "implicitWidth" of "row" is 60.5
      And the property "implicitHeight" of "row" is 46

  Rule: A Box turns

    @ljos-only
    Scenario: Flipping the direction lays the same children out the other way
      Given the QML:
        """qml
        import Ljos.Ui
        Box {
          objectName: "box"
          property bool wide: false
          width: 200
          height: 300
          direction: wide ? Box.Row : Box.Column
          Rectangle { objectName: "display"; Layout.fillWidth: true; Layout.fillHeight: true }
          Rectangle { objectName: "pad"; implicitWidth: 80; implicitHeight: 120 }
        }
        """
      When 100 milliseconds pass
      Then the property "y" of "pad" is 180
      And the property "width" of "display" is 200
      And the property "height" of "display" is 180
      When the property "wide" of "box" is set to true
      And 100 milliseconds pass
      Then the property "x" of "pad" is 120
      And the property "y" of "pad" is 0
      And the property "width" of "display" is 120
      And the property "height" of "display" is 300

  Rule: A box's child is not anchored

    @ljos-only @headless
    Scenario: An anchor on a child of a Row is reported
      Given the file "Bar.qml":
        """qml
        import Ljos.Ui
        Row {
          Rectangle { anchors.left: parent.left; width: 10; height: 10 }
          Item { width: 10; height: 10; anchors { verticalCenter: parent.verticalCenter } }
          Item { Item { anchors.fill: parent } }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Bar.qml:3:23: error: `anchors.left` on a child of a `Row` is overridden by it; use `Layout.alignment` or move it out of the Row [layout-anchored]
        Bar.qml:4:51: error: `anchors.verticalCenter` on a child of a `Row` is overridden by it; use `Layout.alignment` or move it out of the Row [layout-anchored]
        1 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

  Rule: Qt's layouts and positioners are lowered to it where they agree

    @ljos-only @headless
    Scenario: A file with a RowLayout and a Column would be lowered
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Layouts
        Rectangle {
          width: 320; height: 120; color: "white"
          Column {
            x: 10; y: 10; spacing: 4
            Rectangle { width: 60; height: 20; color: "#c03030" }
            Rectangle { width: 40; height: 20; color: "#30c030" }
          }
          RowLayout {
            x: 80; y: 10; width: 230
            Rectangle { implicitWidth: 30; implicitHeight: 30; color: "#3030c0" }
            Rectangle { Layout.fillWidth: true; implicitHeight: 20; color: "#c0c030" }
            Rectangle { Layout.preferredWidth: 50; implicitHeight: 40; color: "#30c0c0" }
          }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        would lower ./Main.qml (13 edits)
        1 files, 1 lowered, 0 kept
        """
      When the author runs "ljos render . --out kept.png --settle 0 --compat keep"
      And the author runs "ljos render . --out lowered.png --settle 0 --compat lower"
      Then the pictures "kept.png" and "lowered.png" have the same pixels

    @ljos-only @headless
    Scenario: A RowLayout whose child gives a Text a width is kept
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          Text { text: "Name"; width: 80 }
          Rectangle { Layout.fillWidth: true; implicitHeight: 20 }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:4:24: `width` of a `Text` in a `RowLayout` is not its size there, and would be in Ljós's `Row`; give it `Layout.preferredWidth`
        1 files, 0 lowered, 1 kept
        """

    @ljos-only @headless
    Scenario: A Row positioner whose child asks a layout to fill it is kept
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Layouts
        Row {
          width: 200
          Rectangle { width: 20; height: 20 }
          Rectangle { Layout.fillWidth: true; height: 20 }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:6:15: `Layout.fillWidth` is ignored by Qt's `Row` and would be honoured by Ljós's
        1 files, 0 lowered, 1 kept
        """
