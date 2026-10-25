# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-imports.html
#   https://doc.qt.io/qt-6/qmlvaluetypes.html
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html
#   docs/ARCHITECTURE.md sections 5.5 (the type description feeds the compiler) and 19.3, and decision D1
# `ljos check` is Ljós's own program: Qt has nothing to compare it with.
# A build reads QML against a table of types. Where a table of Qt's types was generated from a
# local Qt it is that one; a build without Qt reads against what Ljós's own types declare. What is
# not tagged here holds of both builds. `@no-qt-table` is what only the second says, and a build
# with Qt's table does not run it; `@qt-table` is what only the first says. A member Qt's table does
# not list is an error, as Qt fails to load the file; one Ljós's own table does not list is a
# warning, as that table may lack what Qt has.
# A reported column counts the characters of the line as it is in the file, and a file given here is
# written with the eight spaces it is indented by: the first visible column is 9.

@ljos-only @headless
Feature: Checking files on a machine that has no Qt
  Ljós reads QML by what its own types declare, so `ljos check` knows
  the types Ljós implements, their properties, signals and enums on a
  machine where Qt was never installed. What it cannot know there is
  whether a name it has never heard of is a type of Qt's, and it says so.

  Rule: The types Ljós implements are known with their members

    Scenario: A rectangle with a text in it has nothing to report
      Given the file "Main.qml":
        """qml
        import QtQuick
        Rectangle {
          width: 200; height: 100
          color: "steelblue"
          border.width: 2
          Text {
            anchors.centerIn: parent
            text: "Halló"
            font.bold: true
            horizontalAlignment: Text.AlignHCenter
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: Controls in a layout have nothing to report
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls
        import QtQuick.Layouts
        ColumnLayout {
          spacing: 4
          Label { text: "Name" }
          TextField { id: name; Layout.fillWidth: true; placeholderText: "Jón" }
          Button {
            text: "Greet"
            Layout.alignment: Qt.AlignRight
            onClicked: console.log(name.text)
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: A model with its rows and a handler of another object's signal have nothing to report
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          Timer { id: tick; interval: 1000; running: true }
          Connections { target: tick; function onTriggered() { console.log(rows.count) } }
          ListModel {
            id: rows
            ListElement { name: "Anna"; age: 31 }
          }
          Repeater { model: rows; Text { text: name } }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: What a state changes is looked up on the object it changes
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          id: root
          property real sliderWidth: 1
          Rectangle { id: box }
          states: State {
            name: "wide"
            PropertyChanges { target: root; sliderWidth: 5; z: 3 }
            PropertyChanges { target: box; color: "red"; restoreEntryValues: false }
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    @no-qt-table
    Scenario: What a state changes that its object does not have is a warning of that object's
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          Rectangle { id: box }
          states: State {
            name: "wide"
            PropertyChanges { target: box; colr: "red" }
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Main.qml:6:44: warning: `colr` is not a property of `Rectangle`; did you mean `color`? [unknown-property]
        1 files, 0 errors, 1 warnings
        """

    @qt-table
    Scenario: A misspelt property of a button is an error where Qt's table lists its members
      Given the file "Main.qml":
        """qml
        import QtQuick.Controls
        Button {
          txt: "Greet"
        }
        """
      When the files are checked
      Then the check reports:
        """
        Main.qml:3:11: error: `txt` is not a property of `Button`; did you mean `text`? [unknown-property]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

    @no-qt-table
    Scenario: A misspelt property of a button is a warning where Ljós's own table describes it
      Given the file "Main.qml":
        """qml
        import QtQuick.Controls
        Button {
          txt: "Greet"
        }
        """
      When the files are checked
      Then the check reports:
        """
        Main.qml:3:11: warning: `txt` is not a property of `Button`; did you mean `text`? [unknown-property]
        1 files, 0 errors, 1 warnings
        """

    @qt-table
    Scenario: A part a font does not have is an error where Qt's table lists its members
      Given the file "Main.qml":
        """qml
        import QtQuick
        Text {
          font.sparkle: true
        }
        """
      When the files are checked
      Then the check reports:
        """
        Main.qml:3:11: error: `sparkle` is not a property of `font` [unknown-property]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

    @no-qt-table
    Scenario: A part a font does not have is a warning where Ljós's own table describes it
      Given the file "Main.qml":
        """qml
        import QtQuick
        Text {
          font.sparkle: true
        }
        """
      When the files are checked
      Then the check reports:
        """
        Main.qml:3:11: warning: `sparkle` is not a property of `font` [unknown-property]
        1 files, 0 errors, 1 warnings
        """

    Scenario: A mouse area nobody named is reported as it is with Qt's types
      Given the file "Main.qml":
        """qml
        import QtQuick
        Rectangle {
          MouseArea { anchors.fill: parent; onClicked: parent.color = "tomato" }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Main.qml:3:11: warning: `MouseArea` reacts to clicks but neither it nor its parent has `Accessible.name`, so assistive technology cannot tell what it does; set `Accessible.role` and `Accessible.name`, or `Accessible.ignored: true` if it is only decoration [a11y-unnamed-interactive]
        1 files, 0 errors, 1 warnings
        """

    Scenario: A type of a module the file does not import is no type of the file
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          Button { }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Main.qml:3:11: error: `Button` is not a type of anything the file imports [unknown-type]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

  Rule: A name no type has may be a type of Qt's that this build has not heard of

    @no-qt-table
    Scenario: A type no module has is reported with what the build cannot know
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          Sparkle { }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Main.qml:3:11: error: `Sparkle` is not a type of anything the file imports (this build knows Ljós's own types only: with a table of Qt's types it could tell whether Qt has it) [unknown-type]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

    @no-qt-table
    Scenario: A type of a module of Qt's that Ljós does not implement is reported the same way
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtCharts
        Item {
          ChartView { }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Main.qml:4:11: error: `ChartView` is not a type of anything the file imports (this build knows Ljós's own types only: with a table of Qt's types it could tell whether Qt has it) [unknown-type]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

  Rule: A type of a module Ljós does not build stands in, and the check says so

    Scenario: The import of a module whose types stand in is a warning
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtLocation
        Item {
          Map { }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Main.qml:2:9: warning: module "QtLocation" is not part of Ljós: its types stand in as empty Items, and what only they have is left out [stand-in]
        1 files, 0 errors, 1 warnings
        """
