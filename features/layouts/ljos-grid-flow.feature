# Sources:
#   docs/DIALECT.md §4.6 (Ljós's one arrangement), §4.14 (what lowers mechanically)
#   https://doc.qt.io/qt-6/qml-qtquick-grid.html
#   https://doc.qt.io/qt-6/qml-qtquick-flow.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# `Ljos.Layout`'s `Grid` is Ljós's own: the arrangement of `Row`, `Column` and `Box` on two axes.
# Where its rule and Qt's `Grid` positioner agree, the Qt file as written is the oracle and the
# Ljós file is held to its numbers. Ljós's `Flow` is Qt's, exported from `Ljos.Layout`.

Feature: Ljós's Grid and Flow
  A Grid of Ljós's API lays its children out in cells, row after row, or
  column after column when its `flow` says so; its `columns` or `rows`
  bound the line. A child keeps its own size unless it asks for another
  through its `Layout`, as in a Row; it can fill its cell, span cells, and
  be put in a cell of its own. A Flow lays its children out in lines that
  wrap at its width, as Qt's does.

  Rule: A Grid agrees with Qt's Grid positioner where their rules agree

    @qt-parity
    Scenario Outline: A Grid positioner of three columns
      Given the QML:
        """qml
        import QtQuick
        Grid {
          objectName: "grid"
          columns: 3
          spacing: 4
          padding: 2
          Rectangle { objectName: "a"; width: 20; height: 10 }
          Rectangle { objectName: "b"; width: 30; height: 15 }
          Rectangle { objectName: "c"; width: 10; height: 10 }
          Rectangle { objectName: "d"; width: 25; height: 20 }
          Rectangle { objectName: "e"; width: 5; height: 5 }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "y" of "<item>" is <y>
      And the property "implicitWidth" of "grid" is 77
      And the property "implicitHeight" of "grid" is 43

      Examples:
        | item | x  | y  |
        | a    | 2  | 2  |
        | b    | 31 | 2  |
        | c    | 65 | 2  |
        | d    | 2  | 21 |
        | e    | 31 | 21 |

    @ljos-only
    Scenario Outline: The same children in a Ljós Grid
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        Grid {
          objectName: "grid"
          columns: 3
          spacing: 4
          padding: 2
          Rectangle { objectName: "a"; width: 20; height: 10 }
          Rectangle { objectName: "b"; width: 30; height: 15 }
          Rectangle { objectName: "c"; width: 10; height: 10 }
          Rectangle { objectName: "d"; width: 25; height: 20 }
          Rectangle { objectName: "e"; width: 5; height: 5 }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "y" of "<item>" is <y>
      And the property "implicitWidth" of "grid" is 77
      And the property "implicitHeight" of "grid" is 43

      Examples:
        | item | x  | y  |
        | a    | 2  | 2  |
        | b    | 31 | 2  |
        | c    | 65 | 2  |
        | d    | 2  | 21 |
        | e    | 31 | 21 |

    @qt-parity
    Scenario Outline: A Grid positioner that fills two rows column after column
      Given the QML:
        """qml
        import QtQuick
        Grid {
          objectName: "grid"
          rows: 2
          flow: Grid.TopToBottom
          spacing: 3
          Rectangle { objectName: "a"; width: 20; height: 10 }
          Rectangle { objectName: "b"; width: 30; height: 15 }
          Rectangle { objectName: "c"; width: 10; height: 12 }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "y" of "<item>" is <y>
      And the property "implicitWidth" of "grid" is 43
      And the property "implicitHeight" of "grid" is 30

      Examples:
        | item | x  | y  |
        | a    | 0  | 0  |
        | b    | 0  | 15 |
        | c    | 33 | 0  |

    @ljos-only
    Scenario Outline: The same children in a Ljós Grid that fills two rows column after column
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        Grid {
          objectName: "grid"
          rows: 2
          flow: Grid.TopToBottom
          spacing: 3
          Rectangle { objectName: "a"; width: 20; height: 10 }
          Rectangle { objectName: "b"; width: 30; height: 15 }
          Rectangle { objectName: "c"; width: 10; height: 12 }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "y" of "<item>" is <y>
      And the property "implicitWidth" of "grid" is 43
      And the property "implicitHeight" of "grid" is 30

      Examples:
        | item | x  | y  |
        | a    | 0  | 0  |
        | b    | 0  | 15 |
        | c    | 33 | 0  |

  Rule: A Grid's child asks for its size and its cells with its Layout

    # Qt's Grid positioner reads no `Layout`; its GridLayout spreads room no child fills over the
    # columns. Ljós's Grid has the one rule of its Row: a child that fills takes the room left,
    # and room nobody takes is left at the end.
    @ljos-only @qt-diverges
    Scenario: A child that fills its column, and one that spans both
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        Grid {
          width: 200
          columns: 2
          columnSpacing: 10
          rowSpacing: 4
          Rectangle { objectName: "a"; implicitWidth: 50; implicitHeight: 20 }
          Rectangle { objectName: "b"; Layout.fillWidth: true; implicitHeight: 20 }
          Rectangle { objectName: "c"; Layout.columnSpan: 2; Layout.fillWidth: true; implicitHeight: 10 }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "b" is 60
      And the property "width" of "b" is 140
      And the property "x" of "c" is 0
      And the property "y" of "c" is 24
      And the property "width" of "c" is 200

    @ljos-only
    Scenario: A Grid with no count is one line along its flow
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        Grid {
          objectName: "grid"
          spacing: 5
          Rectangle { objectName: "a"; width: 20; height: 10 }
          Rectangle { objectName: "b"; width: 30; height: 15 }
          Rectangle { objectName: "c"; width: 10; height: 10 }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "c" is 60
      And the property "y" of "c" is 0
      And the property "implicitWidth" of "grid" is 70
      And the property "implicitHeight" of "grid" is 15

  Rule: Ljós's Flow is Qt's

    @qt-parity
    Scenario Outline: A Flow wraps its children at its width
      Given the QML:
        """qml
        import QtQuick
        Flow {
          objectName: "flow"
          width: 100
          spacing: 5
          padding: 1
          Rectangle { objectName: "a"; width: 40; height: 10 }
          Rectangle { objectName: "b"; width: 40; height: 20 }
          Rectangle { objectName: "c"; width: 40; height: 10 }
          Rectangle { objectName: "d"; width: 10; height: 10 }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "y" of "<item>" is <y>
      And the property "implicitHeight" of "flow" is 37

      Examples:
        | item | x  | y  |
        | a    | 1  | 1  |
        | b    | 46 | 1  |
        | c    | 1  | 26 |
        | d    | 46 | 26 |

    @ljos-only
    Scenario Outline: The same children in a Ljós Flow
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        Flow {
          objectName: "flow"
          width: 100
          spacing: 5
          padding: 1
          Rectangle { objectName: "a"; width: 40; height: 10 }
          Rectangle { objectName: "b"; width: 40; height: 20 }
          Rectangle { objectName: "c"; width: 40; height: 10 }
          Rectangle { objectName: "d"; width: 10; height: 10 }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "y" of "<item>" is <y>
      And the property "implicitHeight" of "flow" is 37

      Examples:
        | item | x  | y  |
        | a    | 1  | 1  |
        | b    | 46 | 1  |
        | c    | 1  | 26 |
        | d    | 46 | 26 |

  Rule: A Grid's or a Flow's child is not anchored

    @ljos-only @headless
    Scenario: An anchor on a child of a Grid or a Flow is reported
      Given the file "Tiles.qml":
        """qml
        import Ljos.Ui
        import Ljos.Layout
        Column {
          Grid { Rectangle { anchors.right: parent.right; width: 10; height: 10 } }
          Flow { Rectangle { anchors.top: parent.top; width: 10; height: 10 } }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Tiles.qml:4:30: error: `anchors.right` on a child of a `Grid` is overridden by it; use `Layout.alignment` or move it out of the Grid [layout-anchored]
        Tiles.qml:5:30: error: `anchors.top` on a child of a `Flow` is overridden by it; wrap it in an `Item` or move it out of the Flow [layout-anchored]
        1 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

  Rule: Qt's Grid and Flow are lowered to Ljós's where they agree

    @ljos-only @headless
    Scenario: A file with a Grid and a Flow would be lowered, and draws the same
      Given the file "Main.qml":
        """qml
        import QtQuick
        Rectangle {
          width: 200; height: 120; color: "white"
          Grid {
            x: 10; y: 10; columns: 2; spacing: 4
            Rectangle { width: 40; height: 20; color: "#c03030" }
            Rectangle { width: 30; height: 30; color: "#30c030" }
            Rectangle { width: 20; height: 20; color: "#3030c0" }
          }
          Flow {
            x: 100; y: 10; width: 90; spacing: 5
            Rectangle { width: 40; height: 20; color: "#c0c030" }
            Rectangle { width: 40; height: 20; color: "#30c0c0" }
            Rectangle { width: 40; height: 20; color: "#c030c0" }
          }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        would lower ./Main.qml (8 edits)
        1 files, 1 lowered, 0 kept
        """
      When the author runs "ljos render . --out kept.png --settle 0 --compat keep"
      And the author runs "ljos render . --out lowered.png --settle 0 --compat lower"
      Then the pictures "kept.png" and "lowered.png" have the same pixels

    @ljos-only @headless
    Scenario: A migrated Grid imports Ljos.Layout, and has Qt's four columns written out
      Given the file "Main.qml":
        """qml
        import QtQuick
        Grid {
          Rectangle { width: 10; height: 10 }
          Rectangle { width: 10; height: 10 }
          Rectangle { width: 10; height: 10 }
          Rectangle { width: 10; height: 10 }
        }
        """
      When the author runs "ljos migrate . --write"
      Then the file "Main.qml" is:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        Grid {
          columns: 4
          Rectangle { width: 10; height: 10 }
          Rectangle { width: 10; height: 10 }
          Rectangle { width: 10; height: 10 }
          Rectangle { width: 10; height: 10 }
        }
        """

    # qml6: a count of 0 is no count, so `columns: 0` is Qt's four columns.
    @ljos-only @headless
    Scenario: A Grid of no columns is migrated to Qt's four, said once
      Given the file "Main.qml":
        """qml
        import QtQuick
        Rectangle {
          width: 80; height: 40; color: "white"
          Grid {
            columns: 0
            Rectangle { width: 10; height: 10; color: "#c03030" }
            Rectangle { width: 10; height: 10; color: "#30c030" }
            Rectangle { width: 10; height: 10; color: "#3030c0" }
            Rectangle { width: 10; height: 10; color: "#c0c030" }
            Rectangle { width: 10; height: 10; color: "#30c0c0" }
          }
        }
        """
      When the author runs "ljos render . --out kept.png --settle 0 --compat keep"
      And the author runs "ljos render . --out lowered.png --settle 0 --compat lower"
      Then the pictures "kept.png" and "lowered.png" have the same pixels
      When the author runs "ljos migrate . --write"
      Then the file "Main.qml" is:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        Rectangle {
          width: 80; height: 40; fill: "white"
          Grid {
            columns: 4
            Rectangle { width: 10; height: 10; fill: "#c03030" }
            Rectangle { width: 10; height: 10; fill: "#30c030" }
            Rectangle { width: 10; height: 10; fill: "#3030c0" }
            Rectangle { width: 10; height: 10; fill: "#c0c030" }
            Rectangle { width: 10; height: 10; fill: "#30c0c0" }
          }
        }
        """

    @ljos-only @headless
    Scenario: A Flow with a transition is kept, and says why
      Given the file "Main.qml":
        """qml
        import QtQuick
        Flow {
          add: Transition { }
          Rectangle { width: 10; height: 10 }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:3:3: `add` keeps a `Flow` as Qt's until the Box rules reach a wrapping line
        1 files, 0 lowered, 1 kept
        """

    # Qt's Grid counts the spacing after the last child of a line it does not fill: two children
    # 20 and 30 wide in its default four columns, spaced 5, make it 60 wide, not 55 (qml6).
    @ljos-only @headless
    Scenario: A Grid whose children do not fill its line is kept
      Given the file "Main.qml":
        """qml
        import QtQuick
        Grid {
          columns: 3
          spacing: 5
          Rectangle { width: 20; height: 10 }
          Rectangle { width: 30; height: 15 }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:2:1: Qt's `Grid` counts the spacing after a row of children it does not fill, and Ljós's `Grid` does not
        1 files, 0 lowered, 1 kept
        """

    @ljos-only @headless
    Scenario: A Flow whose child asks a layout to fill it is kept
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Layouts
        Flow {
          width: 200
          Rectangle { width: 20; height: 20 }
          Rectangle { Layout.fillWidth: true; height: 20 }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:6:15: `Layout.fillWidth` is ignored by Qt's `Flow` and would be honoured by Ljós's
        1 files, 0 lowered, 1 kept
        """
