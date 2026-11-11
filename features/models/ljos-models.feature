# Sources:
#   docs/DIALECT.md (§4.8 models and delegates, §4.14, §6 the compat pass)
#   https://doc.qt.io/qt-6/qtquick-modelviewsdata-modelview.html (required properties, the model object)
#   https://doc.qt.io/qt-6/qml-qtqml-models-listmodel.html
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html
#   https://doc.qt.io/qt-6/qml-qtqml-instantiator.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# `Ljos.Models` and the views of `Ljos.Ui` are Ljós's own API: the Qt file of a pair is the oracle,
# and the Ljós file is held to its numbers. The shape is that of a list of cities and their
# hours, as corpus/qtdoc/examples/demos/clocks/Main.qml has it, written here as a case of its own.

Feature: The views of Ljós's API and the models of Ljos.Models
  `Repeater` and `ListView` are in `Ljos.Ui`, and `ListModel`,
  `ListElement` and `Instantiator` in `Ljos.Models`, as Qt has them. A
  delegate reads its row as `modelData`: the value of a list of values,
  the value of a model's one role, and of a model of several roles the
  row itself, each role a member of it. Qt's `model.<role>` is
  `modelData.<role>`, and the compat pass writes it so where the file
  shows what the model is.

  Rule: A delegate's `modelData` is its row

    @qt-parity
    Scenario: A required modelData of a model of several roles is its row
      Given the QML:
        """qml
        import QtQuick
        ListView {
          width: 200; height: 100
          model: ListModel {
            ListElement { city: "Reykjavík"; shift: 0 }
            ListElement { city: "Tokyo"; shift: 9 }
          }
          delegate: Item {
            required property var modelData
            required property int index
            objectName: "r" + index
            width: 200; height: 20
            property string shown: modelData.city + "/" + modelData.shift
          }
        }
        """
      Then the property "shown" of "r1" is "Tokyo/9"

    @qt-parity
    Scenario: The modelData of a model of one role is that role's value
      Given the QML:
        """qml
        import QtQuick
        ListView {
          width: 200; height: 100
          model: ListModel {
            ListElement { city: "Reykjavík" }
            ListElement { city: "Tokyo" }
          }
          delegate: Item {
            objectName: "r" + index
            width: 200; height: 20
            property string shown: modelData
          }
        }
        """
      Then the property "shown" of "r1" is "Tokyo"

    # Qt 6.11 gives the row as `modelData` only to a delegate that declares it required, and
    # leaves it undefined in the delegate's context (features/models/delegate-context.feature).
    # A delegate written in Ljós's API reads the one row either way, so `modelData.city` reads
    # the same in every delegate; a Qt file keeps Qt's undefined.
    @ljos-only
    Scenario: In Ljós's API, modelData of a model of several roles is its row without required properties too
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Models
        ListView {
          width: 200; height: 100
          model: ListModel {
            ListElement { city: "Reykjavík"; shift: 0 }
            ListElement { city: "Tokyo"; shift: 9 }
          }
          delegate: Item {
            objectName: "r" + index
            width: 200; height: 20
            property string shown: modelData.city + "/" + modelData.shift
          }
        }
        """
      Then the property "shown" of "r1" is "Tokyo/9"

    # Qt refuses a write to a member of a required `modelData` ("read-only object"). Ljós's
    # row is the `model` a delegate writes a role through in Qt, so `model.shift = 1` lowered to
    # `modelData.shift = 1` writes the model as it did.
    @ljos-only
    Scenario: In Ljós's API, a role written through modelData is written to the model
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Models
        Item {
          ListModel {
            id: zones
            ListElement { city: "Reykjavík"; shift: 0 }
            ListElement { city: "Tokyo"; shift: 9 }
          }
          Repeater {
            model: zones
            Item { Component.onCompleted: if (index === 0) modelData.shift = 1 }
          }
          Repeater {
            model: zones
            Item { objectName: "s" + index; property int read: modelData.shift }
          }
        }
        """
      Then the property "read" of "s0" is 1
      And the property "read" of "s1" is 9

  Rule: Views and models are Ljós's API as they are Qt's

    @qt-parity
    Scenario: A ListView of a ListModel, a Repeater and an Instantiator in Qt
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          width: 200; height: 100
          property int made: 0
          ListView {
            objectName: "list"
            width: 200; height: 100
            model: ListModel {
              ListElement { city: "Reykjavík"; shift: 0 }
              ListElement { city: "Tokyo"; shift: 9 }
            }
            delegate: Item {
              objectName: "r" + index
              width: ListView.view.width; height: 20
              property string shown: model.city + "/" + model.shift
              property bool current: ListView.isCurrentItem
            }
          }
          Repeater { model: 3; Item { objectName: "dot" + index; x: index * 10 } }
          Instantiator { model: 2; QtObject { Component.onCompleted: root.made += 1 } }
        }
        """
      Then the property "shown" of "r1" is "Tokyo/9"
      And the property "width" of "r1" is 200
      And the property "current" of "r0" is true
      And the property "current" of "r1" is false
      And the property "x" of "dot2" is 20
      And the property "made" of "root" is 2

    @ljos-only
    Scenario: The same in Ljós's API
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Models
        Item {
          id: root; objectName: "root"
          width: 200; height: 100
          property int made: 0
          ListView {
            objectName: "list"
            width: 200; height: 100
            model: ListModel {
              ListElement { city: "Reykjavík"; shift: 0 }
              ListElement { city: "Tokyo"; shift: 9 }
            }
            delegate: Item {
              objectName: "r" + index
              width: ListView.view.width; height: 20
              property string shown: modelData.city + "/" + modelData.shift
              property bool current: ListView.isCurrentItem
            }
          }
          Repeater { model: 3; Item { objectName: "dot" + index; x: index * 10 } }
          Instantiator { model: 2; QtObject { Component.onCompleted: root.made += 1 } }
        }
        """
      Then the property "shown" of "r1" is "Tokyo/9"
      And the property "width" of "r1" is 200
      And the property "current" of "r0" is true
      And the property "current" of "r1" is false
      And the property "x" of "dot2" is 20
      And the property "made" of "root" is 2

  Rule: Qt's views and models are lowered to them

    @ljos-only @headless
    Scenario: A file of views and a ListModel is lowered, and draws as it did
      Given the file "Main.qml":
        """qml
        import QtQuick
        Rectangle {
          width: 120; height: 60; color: "white"
          ListModel {
            id: zones
            ListElement { shade: "#c03030"; wide: 30 }
            ListElement { shade: "#3030c0"; wide: 50 }
          }
          ListView {
            y: 30; width: 120; height: 30; orientation: ListView.Horizontal
            model: zones
            delegate: Rectangle { width: model.wide; height: 20; color: model.shade }
          }
          Repeater {
            model: zones
            Rectangle { x: model.index * 60; width: 20; height: 20; color: shade }
          }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        would lower ./Main.qml (7 edits)
        1 files, 1 lowered, 0 kept
        """
      When the author runs "ljos render . --out kept.png --settle 0 --compat keep"
      And the author runs "ljos render . --out lowered.png --settle 0 --compat lower"
      Then the pictures "kept.png" and "lowered.png" have the same pixels
      When the author runs "ljos migrate --write ."
      Then the file "Main.qml" is:
        """qml
        import Ljos.Ui
        import Ljos.Models
        Rectangle {
          width: 120; height: 60; fill: "white"
          ListModel {
            id: zones
            ListElement { shade: "#c03030"; wide: 30 }
            ListElement { shade: "#3030c0"; wide: 50 }
          }
          ListView {
            y: 30; width: 120; height: 30; orientation: ListView.Horizontal
            model: zones
            delegate: Rectangle { width: modelData.wide; height: 20; fill: modelData.shade }
          }
          Repeater {
            model: zones
            Rectangle { x: index * 60; width: 20; height: 20; fill: shade }
          }
        }
        """

    @ljos-only @headless
    Scenario: QtQml.Models is lowered to Ljos.Models
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQml.Models
        Item {
          Instantiator { model: ['a', 'b']; QtObject { objectName: model.modelData } }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Main.qml (3 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "Main.qml" is:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Models
        Item {
          Instantiator { model: ['a', 'b']; QtObject { objectName: modelData } }
        }
        """

    @ljos-only @headless
    Scenario: A role read off a model the file does not show keeps the file
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          property var zones: []
          Repeater { model: zones; Text { text: model.city } }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:4:41: `model.city` reads a role of a model the file does not show, which `modelData.city` reads only of a model of several roles; give the delegate `required property var city`
        1 files, 0 lowered, 1 kept
        """

    # The model's value is a translation, so it stays a ListModel and is no constant table
    # (features/models/ljos-constant-rows.feature).
    @ljos-only @headless
    Scenario: A role of a model of one role is that model's modelData
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          Repeater {
            model: ListModel { ListElement { city: qsTr("Reykjavík") } }
            Text { text: model.city }
          }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Main.qml (2 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "Main.qml" is:
        """qml
        import Ljos.Ui
        import Ljos.Models
        Item {
          Repeater {
            model: ListModel { ListElement { city: qsTr("Reykjavík") } }
            Text { text: modelData }
          }
        }
        """

    # A `Component` a view names by id is its delegate as if it were written
    # in the view's `delegate:`, when nothing else in the file names it.
    @ljos-only @headless
    Scenario: A delegate given by id reads the rows of the view it is given to
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          width: 120; height: 60
          ListModel {
            id: zones
            ListElement { shade: "#c03030"; wide: 30 }
            ListElement { shade: "#3030c0"; wide: 50 }
          }
          Component { id: bar; Rectangle { width: model.wide; height: 20; color: model.shade } }
          ListView { width: 120; height: 60; orientation: ListView.Horizontal; model: zones; delegate: bar }
        }
        """
      When the author runs "ljos render . --out kept.png --settle 0 --compat keep"
      And the author runs "ljos render . --out lowered.png --settle 0 --compat lower"
      Then the pictures "kept.png" and "lowered.png" have the same pixels
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
          width: 120; height: 60
          ListModel {
            id: zones
            ListElement { shade: "#c03030"; wide: 30 }
            ListElement { shade: "#3030c0"; wide: 50 }
          }
          Component { id: bar; Rectangle { width: modelData.wide; height: 20; fill: modelData.shade } }
          ListView { width: 120; height: 60; orientation: ListView.Horizontal; model: zones; delegate: bar }
        }
        """

    @ljos-only @headless
    Scenario: A property called model is the file's own, not a row
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          property var model: ({ city: "Reykjavík" })
          Text { text: model.city }
          Repeater { model: 2; Item { property var model: ({ city: "Tokyo" }); width: model.city.length } }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Main.qml (3 edits)
        scripts of ./Main.qml stay JavaScript: typed, ljos check finds 2
        1 files, 1 lowered, 0 kept
        """
      And the file "Main.qml" is:
        """qml
        pragma Scripts: JavaScript
        import Ljos.Ui
        Item {
          property unknown model: ({ city: "Reykjavík" })
          Text { text: model.city }
          Repeater { model: 2; Item { property unknown model: ({ city: "Tokyo" }); width: model.city.length } }
        }
        """
