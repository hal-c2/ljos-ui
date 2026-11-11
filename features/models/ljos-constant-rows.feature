# Sources:
#   docs/DIALECT.md (§4.4 typed properties, §4.8 models and delegates, §4.14, §6 the compat pass)
#   https://doc.qt.io/qt-6/qml-qtqml-models-listmodel.html
#   https://doc.qt.io/qt-6/qtquick-modelviewsdata-modelview.html (required properties, the model object)
# Verified on Qt 6.11.2 (qml6, offscreen): a role an element of a ListModel leaves out reads as its
# type's default ("", 0, false), and a delegate's required properties are given an array's
# objects' members as they are given a ListModel's roles.
# The Qt file of a pair is the oracle, and the Ljós file is held to its numbers. The shape is
# that of a list of cities and their hours, as corpus/qtdoc/examples/demos/clocks/Main.qml has it,
# written here as a case of its own.

Feature: A ListModel of constants is a typed list
  Most ListModels are tables written once: constant elements that nothing
  edits. In Ljós's API such a table is a `readonly property list<T>` of
  objects, with its row type `T` declared in a `.ts` module, so what a
  delegate reads of a row has a type. `ljos migrate` writes a constant
  ListModel so and writes its row type in `<File>.rows.ts` beside the
  file. A ListModel that a script edits, or whose rows the file reads in
  ways a list does not give, stays a ListModel.

  Rule: A list of objects reads as the table did

    @qt-parity
    Scenario: A table of constants in Qt
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 200; height: 100
          ListModel {
            id: zones
            ListElement { city: "Reykjavík"; shift: 0; home: true }
            ListElement { city: "Tokyo"; shift: 9 }
          }
          ListView {
            width: 200; height: 60; model: zones
            delegate: Item {
              objectName: "r" + index; width: 200; height: 20
              property string shown: model.city + "/" + model.shift + "/" + model.home
            }
          }
          Repeater {
            model: zones
            Item {
              required property string city; required property real shift
              required property int index
              objectName: "c" + index
              property string shown: city + shift
            }
          }
        }
        """
      Then the property "shown" of "r0" is "Reykjavík/0/true"
      And the property "shown" of "r1" is "Tokyo/9/false"
      And the property "shown" of "c1" is "Tokyo9"

    @ljos-only
    Scenario: The same table as a typed list in Ljós's API
      Given the file "Main.rows.ts":
        """ts
        export interface ZonesRow {
          readonly city: string;
          readonly shift: number;
          readonly home: boolean;
        }
        """
      And the QML:
        """qml
        import Ljos.Ui
        import "Main.rows.ts" as Rows
        Item {
          width: 200; height: 100
          readonly property list<Rows.ZonesRow> zones: [
            { city: "Reykjavík", shift: 0, home: true },
            { city: "Tokyo", shift: 9, home: false }
          ]
          ListView {
            width: 200; height: 60; model: zones
            delegate: Item {
              objectName: "r" + index; width: 200; height: 20
              property string shown: modelData.city + "/" + modelData.shift + "/" + modelData.home
            }
          }
          Repeater {
            model: zones
            Item {
              required property string city; required property real shift
              required property int index
              objectName: "c" + index
              property string shown: city + shift
            }
          }
        }
        """
      Then the property "shown" of "r0" is "Reykjavík/0/true"
      And the property "shown" of "r1" is "Tokyo/9/false"
      And the property "shown" of "c1" is "Tokyo9"

  Rule: Qt's constant ListModels are lowered to typed lists

    # The table's `id:` line stays, empty: an edit never adds or removes a line, so what is said
    # of a line of the lowered file is said of the same line of the written one. A role the
    # second element sets and the first leaves out is written in the first as Qt reads it there.
    @ljos-only @headless
    Scenario: A table with an id is a list the views are given, with its row type beside it
      Given the file "Main.qml":
        """qml
        import QtQuick
        Rectangle {
          width: 120; height: 60; color: "white"
          ListModel {
            id: zones
            ListElement { shade: "#c03030"; wide: 30 }
            ListElement { shade: "#3030c0"; wide: 50; tall: true }
          }
          ListView {
            y: 30; width: 120; height: 30; orientation: ListView.Horizontal
            model: zones
            delegate: Rectangle { width: model.wide; height: model.tall ? 30 : 20; color: model.shade }
          }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        would lower ./Main.qml (16 edits)
        would write ./Main.rows.ts
        1 files, 1 lowered, 0 kept
        """
      When the author runs "ljos render . --out kept.png --settle 0 --compat keep"
      And the author runs "ljos render . --out lowered.png --settle 0 --compat lower"
      Then the pictures "kept.png" and "lowered.png" have the same pixels
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Main.qml (16 edits)
        wrote ./Main.rows.ts
        1 files, 1 lowered, 0 kept
        """
      And the file "Main.qml" is:
        """qml
        import Ljos.Ui
        import "Main.rows.ts" as Rows
        Rectangle {
          width: 120; height: 60; fill: "white"
          readonly property list<Rows.ZonesRow> zones: [

            { shade: "#c03030", wide: 30, tall: false },
            { shade: "#3030c0", wide: 50, tall: true }
          ]
          ListView {
            y: 30; width: 120; height: 30; orientation: ListView.Horizontal
            model: zones
            delegate: Rectangle { width: modelData.wide; height: modelData.tall ? 30 : 20; fill: modelData.shade }
          }
        }
        """
      And the file "Main.rows.ts" is:
        """ts
        // The rows of the constant ListModels of Main.qml (docs/DIALECT.md §4.8).
        export interface ZonesRow {
          readonly shade: string;
          readonly wide: number;
          readonly tall: boolean;
        }
        """
      When the author runs "ljos render . --out written.png --settle 0 --compat keep"
      Then the pictures "kept.png" and "written.png" have the same pixels

    # A file put back to its Qt text (a `git checkout` of it) leaves beside it the rows module
    # migrate wrote, which is not tracked. That module is the one migrate would write, to the
    # byte, so the file is lowered again as it was the first time, and the module is not written
    # again.
    @ljos-only @headless
    Scenario: A rows module that is the one migrate would write lowers the file again
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          width: 120; height: 40
          ListModel {
            id: zones
            ListElement { city: "Oslo"; shift: 1 }
          }
          Repeater { model: zones; Text { text: model.city } }
        }
        """
      And the file "Main.rows.ts":
        """ts
        // The rows of the constant ListModels of Main.qml (docs/DIALECT.md §4.8).
        export interface ZonesRow {
          readonly city: string;
          readonly shift: number;
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Main.qml (7 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "Main.qml" is:
        """qml
        import Ljos.Ui
        import "Main.rows.ts" as Rows
        Item {
          width: 120; height: 40
          readonly property list<Rows.ZonesRow> zones: [

            { city: "Oslo", shift: 1 }
          ]
          Repeater { model: zones; Text { text: modelData.city } }
        }
        """
      And the file "Main.rows.ts" is:
        """ts
        // The rows of the constant ListModels of Main.qml (docs/DIALECT.md §4.8).
        export interface ZonesRow {
          readonly city: string;
          readonly shift: number;
        }
        """

    # A script of that path with other text is the author's: it does not declare the rows the
    # lowered file would import, so the file is kept and neither is written.
    @ljos-only @headless
    Scenario: A rows module of the author's keeps the file as it is
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          width: 120; height: 40
          ListModel {
            id: zones
            ListElement { city: "Oslo"; shift: 1 }
          }
          Repeater { model: zones; Text { text: model.city } }
        }
        """
      And the file "Main.rows.ts":
        """ts
        export interface ZonesRow {
          readonly city: string;
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        kept ./Main.qml:1:1: `Main.rows.ts` is a script of the project's, not the module of this file's rows
        1 files, 0 lowered, 1 kept
        """
      And the file "Main.qml" is:
        """qml
        import QtQuick
        Item {
          width: 120; height: 40
          ListModel {
            id: zones
            ListElement { city: "Oslo"; shift: 1 }
          }
          Repeater { model: zones; Text { text: model.city } }
        }
        """
      And the file "Main.rows.ts" is:
        """ts
        export interface ZonesRow {
          readonly city: string;
        }
        """

    # A table written in a view's `model:` becomes a list the view holds. A delegate whose root
    # declares required properties is given them by the list as by the table, so they stay.
    @ljos-only @headless
    Scenario: A table written in a view is a list the view holds
      Given the file "Clock.qml":
        """qml
        import QtQuick
        Rectangle { property string city; property real shift; width: 20 + shift * 10; height: 20; color: city === "Tokyo" ? "red" : "blue" }
        """
      And the file "Main.qml":
        """qml
        import QtQuick
        Item {
          width: 200; height: 60
          ListView {
            width: 200; height: 60
            delegate: Clock { required city; required shift }
            model: ListModel {
              ListElement { city: "Reykjavík"; shift: 0 }
              ListElement { city: "Tokyo"; shift: 9 }
            }
          }
        }
        """
      When the author runs "ljos render . --out kept.png --settle 0 --compat keep"
      And the author runs "ljos render . --out lowered.png --settle 0 --compat lower"
      Then the pictures "kept.png" and "lowered.png" have the same pixels
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Clock.qml (2 edits)
        lowered ./Main.qml (8 edits)
        wrote ./Main.rows.ts
        2 files, 2 lowered, 0 kept
        """
      And the file "Main.qml" is:
        """qml
        import Ljos.Ui
        import "Main.rows.ts" as Rows
        Item {
          width: 200; height: 60
          ListView {
            width: 200; height: 60
            delegate: Clock { required city; required shift }
            readonly property list<Rows.Row> rows: [
              { city: "Reykjavík", shift: 0 },
              { city: "Tokyo", shift: 9 }
            ]; model: rows
          }
        }
        """
      And the file "Main.rows.ts" is:
        """ts
        // The rows of the constant ListModels of Main.qml (docs/DIALECT.md §4.8).
        export interface Row {
          readonly city: string;
          readonly shift: number;
        }
        """

    # Each of these ListModels stays one, and the file is still lowered: a script appends to the
    # first, the second's value is a translation made as the file runs, a delegate reads the
    # third's role by its bare name (which a list does not give), and one writes the fourth's.
    @ljos-only @headless
    Scenario: A table that is edited, made as the file runs or read by a role's name stays a ListModel
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          ListModel { id: edited; ListElement { city: "Reykjavík"; shift: 0 } }
          Component.onCompleted: edited.append({ city: "Tokyo", shift: 9 })
          Repeater { model: edited; Text { text: model.city } }
          Repeater {
            model: ListModel { ListElement { city: qsTr("Reykjavík"); shift: 0 } }
            Text { text: model.city }
          }
          Repeater {
            model: ListModel { ListElement { city: "Reykjavík"; shift: 0 } }
            Text { text: city }
          }
          Repeater {
            model: ListModel { ListElement { city: "Reykjavík"; shift: 0 } }
            Item { Component.onCompleted: model.shift = 1 }
          }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Main.qml (4 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "Main.qml" is:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Models
        Item {
          ListModel { id: edited; ListElement { city: "Reykjavík"; shift: 0 } }
          Component.onCompleted: edited.append({ city: "Tokyo", shift: 9 })
          Repeater { model: edited; Text { text: modelData.city } }
          Repeater {
            model: ListModel { ListElement { city: qsTr("Reykjavík"); shift: 0 } }
            Text { text: modelData.city }
          }
          Repeater {
            model: ListModel { ListElement { city: "Reykjavík"; shift: 0 } }
            Text { text: city }
          }
          Repeater {
            model: ListModel { ListElement { city: "Reykjavík"; shift: 0 } }
            Item { Component.onCompleted: modelData.shift = 1 }
          }
        }
        """
