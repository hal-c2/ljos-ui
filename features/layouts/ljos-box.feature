# Sources:
#   docs/DIALECT.md (Ljós's own API, the compat pass, the checker's rules)
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-rowlayout.html
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html
#   https://doc.qt.io/qt-6/qml-qtquick-row.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# `Ljos.Layout`'s `Row`, `Column` and `Box` are Ljós's own: one arrangement in the place of Qt's
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
        import Ljos.Layout
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
        import Ljos.Layout
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
        import Ljos.Layout
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
        import Ljos.Layout
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

    @ljos-only
    Scenario: A child given a Layout, hidden and shown again, asks for its own size, not the one the box gave it
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        Row {
          objectName: "row"
          width: 300
          spacing: 0
          Rectangle { objectName: "filler"; Layout.fillWidth: true; implicitWidth: 50; height: 10 }
          Rectangle { objectName: "fixed"; width: 20; height: 10 }
        }
        """
      When 100 milliseconds pass
      Then the property "width" of "filler" is 280
      And the property "implicitWidth" of "row" is 70
      When the property "visible" of "filler" is set to false
      And 100 milliseconds pass
      Then the property "implicitWidth" of "row" is 20
      When the property "visible" of "filler" is set to true
      And 100 milliseconds pass
      Then the property "width" of "filler" is 280
      And the property "implicitWidth" of "row" is 70

  Rule: A box that is a layout takes a size from its child's content in whole pixels

    # Qt's RowLayout rounds a child's implicit size up: an Item of implicit size 20.5 x 10.25 is
    # 21 x 11 in one, and the next child is at 21 (qml6). Its Row, a positioner, places it at
    # 20.5. Ljós's box is a layout when any of its children has a `Layout`, and rounds the same
    # way, so no seam falls between two pixels.
    @qt-parity
    Scenario: A RowLayout rounds a child's implicit size up
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          objectName: "row"
          spacing: 0
          Item { objectName: "a"; implicitWidth: 20.5; implicitHeight: 10.25 }
          Item { objectName: "b"; implicitWidth: 10; implicitHeight: 10 }
        }
        """
      When 100 milliseconds pass
      Then the property "width" of "a" is 21
      And the property "height" of "a" is 11
      And the property "x" of "b" is 21
      And the property "implicitWidth" of "row" is 31
      And the property "implicitHeight" of "row" is 11

    @ljos-only
    Scenario: A Ljós Row whose children have a Layout rounds a child's implicit size up
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        Row {
          objectName: "row"
          Item { objectName: "a"; implicitWidth: 20.5; implicitHeight: 10.25; Layout.alignment: Align.Top }
          Item { objectName: "b"; implicitWidth: 10; implicitHeight: 10; Layout.alignment: Align.Top }
        }
        """
      When 100 milliseconds pass
      Then the property "width" of "a" is 21
      And the property "height" of "a" is 11
      And the property "x" of "b" is 21
      And the property "implicitWidth" of "row" is 31
      And the property "implicitHeight" of "row" is 11

    @ljos-only
    Scenario: A Text in a Ljós Column whose children have a Layout is a whole number of pixels wide
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        Column {
          objectName: "column"
          property bool whole: label.width === Math.ceil(label.implicitWidth) && label.width % 1 === 0
          Text { id: label; text: "Hello wor"; Layout.alignment: Align.Left }
          Rectangle { implicitWidth: 10; implicitHeight: 10; Layout.alignment: Align.Left }
        }
        """
      When 100 milliseconds pass
      Then the property "whole" of "column" is true

    @ljos-only
    Scenario: A child whose content changes is given its new size, rounded up
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        Row {
          objectName: "row"
          Item { objectName: "a"; implicitWidth: 20.5; implicitHeight: 10; Layout.alignment: Align.Top }
          Item { objectName: "b"; implicitWidth: 10; implicitHeight: 10; Layout.alignment: Align.Top }
        }
        """
      When 100 milliseconds pass
      And the property "implicitWidth" of "a" is set to 30.25
      And 100 milliseconds pass
      Then the property "width" of "a" is 31
      And the property "x" of "b" is 31
      And the property "implicitWidth" of "row" is 41

    # A child with no Layout of its own, in a box where a sibling has one, is sized by the box.
    # Given 0 when its content is empty, it is left out, and back once its content grows.
    @ljos-only
    Scenario: A child the box gave no width comes back when its content grows
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        Row {
          objectName: "row"
          Item { objectName: "a"; implicitWidth: 20.5; implicitHeight: 10 }
          Item { objectName: "b"; implicitWidth: 10; implicitHeight: 10; Layout.alignment: Align.Top }
        }
        """
      When 100 milliseconds pass
      And the property "implicitWidth" of "a" is set to 0
      And 100 milliseconds pass
      And the property "implicitWidth" of "a" is set to 30.25
      And 100 milliseconds pass
      Then the property "width" of "a" is 31
      And the property "x" of "b" is 31
      And the property "implicitWidth" of "row" is 41

    # The author gives the width the box gave: the same number, but given, so it holds when
    # the content grows.
    @ljos-only
    Scenario: A child given the size the box gave it keeps that size when its content grows
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        Row {
          objectName: "row"
          Item { objectName: "a"; implicitWidth: 20.5; implicitHeight: 10; Layout.alignment: Align.Top }
          Item { objectName: "b"; implicitWidth: 10; implicitHeight: 10; Layout.alignment: Align.Top }
        }
        """
      When 100 milliseconds pass
      And the property "width" of "a" is set to 21
      And the property "implicitWidth" of "a" is set to 30.25
      And 100 milliseconds pass
      Then the property "width" of "a" is 21
      And the property "x" of "b" is 21
      And the property "implicitWidth" of "row" is 31

    # Qt's RowLayout leaves an Item it was given a width of 20.5 at 20.5 but places the next one
    # at 21 (qml6). Ljós's box never rounds a size the author gives.
    @ljos-only @qt-diverges
    Scenario: A size the author gives is not rounded
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        Row {
          objectName: "row"
          Item { objectName: "a"; width: 20.5; height: 10; Layout.alignment: Align.Top }
          Item { objectName: "b"; Layout.preferredWidth: 7.5; implicitHeight: 10 }
          Item { objectName: "c"; implicitWidth: 10; implicitHeight: 10; Layout.alignment: Align.Top }
        }
        """
      When 100 milliseconds pass
      Then the property "width" of "a" is 20.5
      And the property "x" of "b" is 20.5
      And the property "width" of "b" is 7.5
      And the property "x" of "c" is 28
      And the property "implicitWidth" of "row" is 38

    @ljos-only
    Scenario: A box whose children have no Layout places a size from their content as it is, as a positioner does
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        Row {
          objectName: "row"
          Item { objectName: "a"; implicitWidth: 20.5; implicitHeight: 10.25 }
          Item { objectName: "b"; implicitWidth: 10; implicitHeight: 10 }
        }
        """
      When 100 milliseconds pass
      Then the property "width" of "a" is 20.5
      And the property "x" of "b" is 20.5
      And the property "implicitWidth" of "row" is 30.5
      And the property "implicitHeight" of "row" is 10.25

  Rule: A box in a box is as small as its children let it be

    # Measured with qml6: a RowLayout that fills a ColumnLayout 50 wide, whose two children fill and
    # take at least 60 and 30, reads a `Layout.minimumWidth` of 90 and is 90 wide. Ljós's boxes
    # agree.
    @ljos-only
    Scenario: A Row in a Column reads its children's minimum along it, and is held to it
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        Column {
          objectName: "column"
          width: 50
          property real innerMinimum: inner.Layout.minimumWidth
          Row {
            id: inner
            objectName: "inner"
            spacing: 4
            padding: 2
            Layout.fillWidth: true
            Rectangle { Layout.minimumWidth: 60; Layout.fillWidth: true; implicitHeight: 10 }
            Rectangle { Layout.minimumWidth: 30; Layout.fillWidth: true; implicitHeight: 10 }
          }
        }
        """
      When 100 milliseconds pass
      Then the property "innerMinimum" of "column" is 98
      And the property "width" of "inner" is 98

    # Measured with qml6: a ColumnLayout that fills a RowLayout, of children at least 40 and 70
    # wide, reads a `Layout.minimumWidth` of 70, and the RowLayout's is 70 too.
    @ljos-only
    Scenario: A Column in a Row reads the largest of its children's minimum across it
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        Row {
          objectName: "row"
          height: 50
          property real innerMinimum: inner.Layout.minimumWidth
          Column {
            id: inner
            objectName: "inner"
            Layout.fillHeight: true
            Rectangle { Layout.minimumWidth: 40; implicitHeight: 10; Layout.fillHeight: true }
            Rectangle { Layout.minimumWidth: 70; implicitHeight: 10 }
          }
        }
        """
      When 100 milliseconds pass
      Then the property "innerMinimum" of "row" is 70
      And the property "width" of "inner" is 70
      And the property "implicitWidth" of "row" is 70

  Rule: A Box turns

    @ljos-only
    Scenario: Flipping the direction lays the same children out the other way
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Layout
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
        import Ljos.Layout
        Row {
          Rectangle { anchors.left: parent.left; width: 10; height: 10 }
          Item { width: 10; height: 10; anchors { verticalCenter: parent.verticalCenter } }
          Item { Item { anchors.fill: parent } }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Bar.qml:4:23: error: `anchors.left` on a child of a `Row` is overridden by it; use `Layout.alignment` or move it out of the Row [layout-anchored]
        Bar.qml:5:51: error: `anchors.verticalCenter` on a child of a `Row` is overridden by it; use `Layout.alignment` or move it out of the Row [layout-anchored]
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

    # Qt's Row honours an anchor across it (top, bottom, verticalCenter, baseline) and overrides one
    # along it. Measured with qml6: a child 7 high in a Row 50 high, anchored at its vertical centre,
    # is at y 21; Ljós's Row places one with `Layout.alignment: Align.VCenter` at y 22, so the pass
    # cannot write the one for the other.
    @ljos-only @headless
    Scenario: A Row positioner whose child is anchored across it is kept, as Qt honours the anchor
      Given the file "Main.qml":
        """qml
        import QtQuick
        Row {
          height: 50
          Rectangle { width: 10; height: 10 }
          Text { text: "Home"; anchors.verticalCenter: parent.verticalCenter }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:5:24: `anchors.verticalCenter` places a child across Qt's `Row`, which honours it; Ljós's `Row` places it across with `Layout.alignment`
        1 files, 0 lowered, 1 kept
        """

    # A Text's implicit width is a fraction of a pixel. Qt's RowLayout and ColumnLayout round it up,
    # and so does a Ljós Row or Column that is a layout, so the lowered file places each child where
    # the written one did.
    @ljos-only @headless
    Scenario: A RowLayout and a ColumnLayout of Texts are lowered, and their Texts keep their places
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Layouts
        Rectangle {
          width: 320; height: 120; color: "white"
          RowLayout {
            x: 10; y: 10; spacing: 3
            Text { text: "Name"; font.pixelSize: 13 }
            Text { text: "Surname"; font.pixelSize: 13 }
            Rectangle { implicitWidth: 7; implicitHeight: 7; color: "#c03030" }
          }
          ColumnLayout {
            x: 10; y: 40; spacing: 1
            Text { text: "Hello world"; font.pixelSize: 11 }
            Rectangle { implicitWidth: 7; implicitHeight: 7; color: "#30a050" }
            Text { text: "Small"; font.pixelSize: 9 }
          }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        would lower ./Main.qml (16 edits)
        1 files, 1 lowered, 0 kept
        """
      When the author runs "ljos render . --out kept.png --settle 0 --compat keep"
      And the author runs "ljos render . --out lowered.png --settle 0 --compat lower"
      Then the pictures "kept.png" and "lowered.png" have the same pixels

    # Measured with qml6: a RowLayout in a ColumnLayout 200 wide grows across it only where a child of
    # it fills with no maximum. The lowered Row is told to fill there, and the Column in a Row is told
    # to fill its height.
    @ljos-only @headless
    Scenario: A layout in a layout is lowered with a fill where it grows
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Layouts
        Rectangle {
          width: 320; height: 160; color: "white"
          ColumnLayout {
            x: 10; y: 10; width: 200
            RowLayout {
              Rectangle { implicitWidth: 30; implicitHeight: 20; color: "#c03030" }
              Rectangle { Layout.fillWidth: true; implicitHeight: 20; color: "#30a050" }
            }
            RowLayout {
              Rectangle { implicitWidth: 30; implicitHeight: 20; color: "#3030c0" }
            }
          }
          RowLayout {
            x: 220; y: 10; height: 140
            ColumnLayout {
              Rectangle { implicitWidth: 20; Layout.fillHeight: true; color: "#c0c030" }
            }
          }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        would lower ./Main.qml (23 edits)
        1 files, 1 lowered, 0 kept
        """
      When the author runs "ljos render . --out kept.png --settle 0 --compat keep"
      And the author runs "ljos render . --out lowered.png --settle 0 --compat lower"
      Then the pictures "kept.png" and "lowered.png" have the same pixels

    # Measured with qml6: told to fill, a RowLayout whose children neither fill nor grow stays as wide
    # as they are, where Ljós's Row told to fill would take the column's width.
    @ljos-only @headless
    Scenario: A layout in a layout told to fill where nothing in it grows is kept
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Layouts
        ColumnLayout {
          width: 200
          RowLayout {
            Layout.fillWidth: true
            Rectangle { implicitWidth: 30; implicitHeight: 20 }
          }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:5:3: a `RowLayout` in a `ColumnLayout` grows only where a child of it fills and is bounded by its children, and Ljós's `Column` reads what this one is given otherwise
        1 files, 0 lowered, 1 kept
        """
