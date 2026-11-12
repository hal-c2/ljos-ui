# Sources:
#   docs/DIALECT.md §4.6 (Ljós's one arrangement), §4.14 (what lowers mechanically)
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-gridlayout.html
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-stacklayout.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# Qt's `GridLayout` lowers to Ljós's `Grid`, with what Qt does by default written out: a spacing
# of 5 between columns and rows, and a child centred in its row. `StackLayout` lowers to `Pages`,
# which is the same native under the name of Ljós's API.

Feature: GridLayout lowers to Grid, and StackLayout to Pages
  A Qt file that lays its children out with a `GridLayout` runs, once
  migrated, as a `Grid` of Ljós's API that places and sizes them the same;
  one that shows one child at a time with a `StackLayout` runs as `Pages`.
  Where the pass cannot tell that the two agree, it keeps the file and says
  why.

  Rule: A Grid sizes its children as Qt's GridLayout does where their rules agree

    @qt-parity
    Scenario Outline: A GridLayout of two columns that fills
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        GridLayout {
          objectName: "grid"
          columns: 2
          width: 200; height: 100
          Rectangle { objectName: "a"; implicitWidth: 30; implicitHeight: 10; Layout.fillWidth: true }
          Rectangle { objectName: "b"; implicitWidth: 50; implicitHeight: 20 }
          Rectangle { objectName: "c"; implicitWidth: 30; implicitHeight: 10; Layout.fillHeight: true }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "y" of "<item>" is <y>
      And the property "width" of "<item>" is <width>
      And the property "height" of "<item>" is <height>
      And the property "implicitWidth" of "grid" is 85
      And the property "implicitHeight" of "grid" is 35

      Examples:
        | item | x   | y  | width | height |
        | a    | 0   | 5  | 145   | 10     |
        | b    | 150 | 0  | 50    | 20     |
        | c    | 0   | 25 | 30    | 75     |

    @ljos-only
    Scenario Outline: The same children in a Ljós Grid, as the pass writes it
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        Grid { columnSpacing: 5; rowSpacing: 5;
          objectName: "grid"
          columns: 2
          width: 200; height: 100
          Rectangle { Layout.alignment: Qt.AlignVCenter; objectName: "a"; implicitWidth: 30; implicitHeight: 10; Layout.fillWidth: true }
          Rectangle { Layout.alignment: Qt.AlignVCenter; objectName: "b"; implicitWidth: 50; implicitHeight: 20 }
          Rectangle { Layout.alignment: Qt.AlignVCenter; objectName: "c"; implicitWidth: 30; implicitHeight: 10; Layout.fillHeight: true }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "y" of "<item>" is <y>
      And the property "width" of "<item>" is <width>
      And the property "height" of "<item>" is <height>
      And the property "implicitWidth" of "grid" is 85
      And the property "implicitHeight" of "grid" is 35

      Examples:
        | item | x   | y  | width | height |
        | a    | 0   | 5  | 145   | 10     |
        | b    | 150 | 0  | 50    | 20     |
        | c    | 0   | 25 | 30    | 75     |

    # Two children that fill share what is left by the sizes they ask for (qml6: 73 and 122).
    @qt-parity
    Scenario Outline: Two children of a GridLayout fill a row between them
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        GridLayout {
          columns: 2
          width: 200
          Rectangle { objectName: "a"; implicitWidth: 30; implicitHeight: 10; Layout.fillWidth: true }
          Rectangle { objectName: "b"; implicitWidth: 50; implicitHeight: 20; Layout.fillWidth: true }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "width" of "<item>" is <width>

      Examples:
        | item | x  | width |
        | a    | 0  | 73    |
        | b    | 78 | 122   |

    @ljos-only
    Scenario Outline: Two children of a Ljós Grid fill a row between them
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        Grid { columnSpacing: 5; rowSpacing: 5;
          columns: 2
          width: 200
          Rectangle { Layout.alignment: Qt.AlignVCenter; objectName: "a"; implicitWidth: 30; implicitHeight: 10; Layout.fillWidth: true }
          Rectangle { Layout.alignment: Qt.AlignVCenter; objectName: "b"; implicitWidth: 50; implicitHeight: 20; Layout.fillWidth: true }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "width" of "<item>" is <width>

      Examples:
        | item | x  | width |
        | a    | 0  | 73    |
        | b    | 78 | 122   |

    # A top-to-bottom GridLayout reads `rows`, the count along its flow: a and b share the
    # first column, c starts the second.
    @qt-parity
    Scenario Outline: A top-to-bottom GridLayout of two rows that fills
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        GridLayout {
          objectName: "grid"
          flow: GridLayout.TopToBottom
          rows: 2
          width: 200; height: 100
          Rectangle { objectName: "a"; implicitWidth: 30; implicitHeight: 10; Layout.fillWidth: true }
          Rectangle { objectName: "b"; implicitWidth: 50; implicitHeight: 20 }
          Rectangle { objectName: "c"; implicitWidth: 30; implicitHeight: 10; Layout.fillHeight: true }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "y" of "<item>" is <y>
      And the property "width" of "<item>" is <width>
      And the property "height" of "<item>" is <height>
      And the property "implicitWidth" of "grid" is 85
      And the property "implicitHeight" of "grid" is 35

      Examples:
        | item | x   | y  | width | height |
        | a    | 0   | 33 | 165   | 10     |
        | b    | 0   | 80 | 50    | 20     |
        | c    | 170 | 0  | 30    | 75     |

    @ljos-only
    Scenario Outline: The same children in a top-to-bottom Ljós Grid, as the pass writes it
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        Grid { columnSpacing: 5; rowSpacing: 5;
          objectName: "grid"
          flow: Grid.TopToBottom
          rows: 2
          width: 200; height: 100
          Rectangle { Layout.alignment: Qt.AlignVCenter; objectName: "a"; implicitWidth: 30; implicitHeight: 10; Layout.fillWidth: true }
          Rectangle { Layout.alignment: Qt.AlignVCenter; objectName: "b"; implicitWidth: 50; implicitHeight: 20 }
          Rectangle { Layout.alignment: Qt.AlignVCenter; objectName: "c"; implicitWidth: 30; implicitHeight: 10; Layout.fillHeight: true }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "y" of "<item>" is <y>
      And the property "width" of "<item>" is <width>
      And the property "height" of "<item>" is <height>
      And the property "implicitWidth" of "grid" is 85
      And the property "implicitHeight" of "grid" is 35

      Examples:
        | item | x   | y  | width | height |
        | a    | 0   | 33 | 165   | 10     |
        | b    | 0   | 80 | 50    | 20     |
        | c    | 170 | 0  | 30    | 75     |

    # The count across a top-to-bottom flow is ignored: `columns: 2` still puts three children
    # in one column. The pass keeps such a file (DIALECT.md §4.14), so this has no Ljós Grid twin.
    @qt-parity
    Scenario Outline: A top-to-bottom GridLayout ignores its columns
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        GridLayout {
          flow: GridLayout.TopToBottom
          columns: 2
          Rectangle { objectName: "a"; implicitWidth: 30; implicitHeight: 10 }
          Rectangle { objectName: "b"; implicitWidth: 30; implicitHeight: 10 }
          Rectangle { objectName: "c"; implicitWidth: 30; implicitHeight: 10 }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<item>" is <x>
      And the property "y" of "<item>" is <y>

      Examples:
        | item | x | y  |
        | a    | 0 | 0  |
        | b    | 0 | 15 |
        | c    | 0 | 30 |

  Rule: Pages shows one child at a time, as StackLayout does

    # qml6: the hidden child keeps its implicit size; the shown one fills the layout.
    @qt-parity
    Scenario Outline: A StackLayout shows the child at its current index, at its size
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        StackLayout {
          objectName: "pages"
          width: 100; height: 50
          currentIndex: 1
          Rectangle { objectName: "a"; implicitWidth: 30; implicitHeight: 10 }
          Rectangle { objectName: "b"; implicitWidth: 40; implicitHeight: 20 }
        }
        """
      When 100 milliseconds pass
      Then the property "visible" of "<item>" is <visible>
      And the property "width" of "<item>" is <width>
      And the property "height" of "<item>" is <height>
      And the property "count" of "pages" is 2

      Examples:
        | item | visible | width | height |
        | a    | false   | 30    | 10     |
        | b    | true    | 100   | 50     |

    @ljos-only
    Scenario Outline: Pages shows the child at its current index, at its size
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        Pages {
          objectName: "pages"
          width: 100; height: 50
          currentIndex: 1
          Rectangle { objectName: "a"; implicitWidth: 30; implicitHeight: 10 }
          Rectangle { objectName: "b"; implicitWidth: 40; implicitHeight: 20 }
        }
        """
      When 100 milliseconds pass
      Then the property "visible" of "<item>" is <visible>
      And the property "width" of "<item>" is <width>
      And the property "height" of "<item>" is <height>
      And the property "count" of "pages" is 2

      Examples:
        | item | visible | width | height |
        | a    | false   | 30    | 10     |
        | b    | true    | 100   | 50     |

  Rule: GridLayout and StackLayout are lowered where Ljós's types agree with them

    @ljos-only @headless
    Scenario: A file with a GridLayout and a StackLayout would be lowered, and draws the same
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Layouts
        Rectangle {
          width: 200; height: 120; color: "white"
          GridLayout {
            x: 10; y: 10; columns: 2
            Rectangle { implicitWidth: 40; implicitHeight: 20; color: "#c03030" }
            Rectangle { implicitWidth: 30; implicitHeight: 30; color: "#30c030" }
            Rectangle { implicitWidth: 20; implicitHeight: 20; color: "#3030c0" }
          }
          StackLayout {
            x: 110; y: 10; width: 80; height: 60; currentIndex: 1
            Rectangle { color: "#c0c030" }
            Rectangle { color: "#30c0c0" }
          }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        would lower ./Main.qml (14 edits)
        1 files, 1 lowered, 0 kept
        """
      When the author runs "ljos render . --out kept.png --settle 0 --compat keep"
      And the author runs "ljos render . --out lowered.png --settle 0 --compat lower"
      Then the pictures "kept.png" and "lowered.png" have the same pixels

    @ljos-only @headless
    Scenario: A migrated GridLayout has Qt's spacing and centring written out
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Layouts
        Item {
          GridLayout {
            columns: 2
            Rectangle { implicitWidth: 10; implicitHeight: 10 }
            Rectangle { implicitWidth: 10; implicitHeight: 20; Layout.alignment: Qt.AlignRight }
          }
        }
        """
      When the author runs "ljos migrate . --write"
      Then the file "Main.qml" is:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        Item {
          Grid { columnSpacing: 5; rowSpacing: 5;
            columns: 2
            Rectangle { Layout.alignment: Qt.AlignVCenter; implicitWidth: 10; implicitHeight: 10 }
            Rectangle { implicitWidth: 10; implicitHeight: 20; Layout.alignment: Qt.AlignRight | Qt.AlignVCenter }
          }
        }
        """

    @ljos-only @headless
    Scenario: A migrated StackLayout is Pages
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Layouts
        StackLayout {
          currentIndex: 1
          Item { }
          Item { }
        }
        """
      When the author runs "ljos migrate . --write"
      Then the file "Main.qml" is:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        Pages {
          currentIndex: 1
          Item { }
          Item { }
        }
        """

    # qml6: a GridLayout of `rows: 2` and the default left-to-right flow puts three children in
    # one row; Ljós's Grid would make two columns of them.
    @ljos-only @headless
    Scenario: A GridLayout whose rows its flow ignores is kept
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Layouts
        GridLayout {
          rows: 2
          Rectangle { implicitWidth: 30; implicitHeight: 10 }
          Rectangle { implicitWidth: 30; implicitHeight: 10 }
          Rectangle { implicitWidth: 30; implicitHeight: 10 }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:4:3: `rows` of a `GridLayout` is ignored along a left-to-right flow, and Ljós's `Grid` would not ignore it
        1 files, 0 lowered, 1 kept
        """

    @ljos-only @headless
    Scenario: A GridLayout whose child is put in a cell of its own is kept
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Layouts
        GridLayout {
          columns: 2
          Rectangle { implicitWidth: 30; implicitHeight: 10; Layout.row: 1 }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:5:54: `Layout.row` puts a child of a `GridLayout` where the pass does not yet follow Qt
        1 files, 0 lowered, 1 kept
        """

    @ljos-only @headless
    Scenario: A StackLayout whose attached properties a child reads is kept
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Layouts
        StackLayout {
          Rectangle { color: StackLayout.isCurrentItem ? "red" : "blue" }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:4:22: `StackLayout`'s attached properties are not in Ljos.Layout yet
        1 files, 0 lowered, 1 kept
        """
