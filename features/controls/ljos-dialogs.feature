# Sources:
#   docs/DIALECT.md (Ljós's own API: §4.1 the modules, §4.16 the controls, §6 the compat pass)
#   https://doc.qt.io/qt-6/qtquickdialogs-index.html
#   https://doc.qt.io/qt-6/qml-qtquick-dialogs-filedialog.html
#   https://doc.qt.io/qt-6/qml-qtquick-dialogs-folderdialog.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# `Ljos.Controls` and `Ljos.Controls.Basic` are Ljós's own modules, and lowering a file to them is Ljós's: those scenarios are
# `@ljos-only`. The file as written for Qt Quick Dialogs runs on both. Qt draws its own dialog
# offscreen; accepting its file dialog with nothing chosen leaves the file empty, and its folder
# dialog takes a folder of its own, so the Qt file reads the folder only before it opens. Ljós's
# tests stand in for the system's dialog and choose in it.

Feature: Ljós's file and folder dialogs, in Ljos.Controls and Ljos.Controls.Basic
  A file in Ljós's API takes its FileDialog and FolderDialog from
  Ljos.Controls, or from Ljos.Controls.Basic: the two styles have the same
  dialogs. They are the dialogs of QtQuick.Dialogs as they are:
  opened, accepted or rejected, with what the user chose in
  selectedFile or selectedFolder. A file of Qt's that imports
  QtQuick.Dialogs is lowered to Ljos.Controls.Basic with no other edit for
  them; a dialog the module has not taken yet, or the dialogs under
  another name, keeps the file as it is written.

  Rule: A file dialog and a folder dialog are Qt's

    @qt-parity
    Scenario: A file dialog and a folder dialog as written for Qt
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Dialogs
        Item {
          id: root; objectName: "root"
          property string log: ""
          FileDialog {
            objectName: "file"; title: "Open notes"; fileMode: FileDialog.OpenFile
            onAccepted: root.log += "file " + selectedFile + ";"
          }
          FolderDialog {
            objectName: "folder"; title: "Where"
            onAccepted: root.log += "folder;"
          }
        }
        """
      Then the property "selectedFolder" of "folder" is ""
      When the method "open" of "file" is called
      Then the property "visible" of "file" is true
      When the method "accept" of "file" is called
      And the method "open" of "folder" is called
      And the method "accept" of "folder" is called
      Then the property "result" of "file" is 1
      And the property "visible" of "folder" is false
      And the property "result" of "folder" is 1
      And the property "log" of "root" is "file ;folder;"

  @ljos-only
  Rule: The same dialogs in Ljós's API, chosen in the system's dialog

    Scenario: The same in Ljós's API
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          property string log: ""
          FileDialog {
            objectName: "file"; title: "Open notes"; fileMode: FileDialog.OpenFile
            onAccepted: root.log += "file " + selectedFile + ";"
          }
          FolderDialog {
            objectName: "folder"; title: "Where"
            onAccepted: root.log += "folder " + selectedFolder + ";"
          }
        }
        """
      When the method "open" of "file" is called
      Then the system shows an "open file" dialog titled "Open notes"
      When the user chooses "/home/user/notes.txt" in the system dialog
      And the method "open" of "folder" is called
      And the user chooses "/home/user/Music" in the system dialog
      Then the property "result" of "file" is 1
      And the property "selectedFile" of "file" is "file:///home/user/notes.txt"
      And the property "selectedFolder" of "folder" is "file:///home/user/Music"
      And the property "log" of "root" is "file file:///home/user/notes.txt;folder file:///home/user/Music;"

    Scenario: The file as written for Qt, lowered, does the same
      Given Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Dialogs
        Item {
          id: root; objectName: "root"
          property string log: ""
          FileDialog {
            objectName: "file"; title: "Open notes"; fileMode: FileDialog.OpenFile
            onAccepted: root.log += "file " + selectedFile + ";"
          }
          FolderDialog {
            objectName: "folder"; title: "Where"
            onAccepted: root.log += "folder " + selectedFolder + ";"
          }
        }
        """
      When the method "open" of "file" is called
      Then the system shows an "open file" dialog titled "Open notes"
      When the user chooses "/home/user/notes.txt" in the system dialog
      And the method "open" of "folder" is called
      And the user chooses "/home/user/Music" in the system dialog
      Then the property "result" of "file" is 1
      And the property "selectedFile" of "file" is "file:///home/user/notes.txt"
      And the property "selectedFolder" of "folder" is "file:///home/user/Music"
      And the property "log" of "root" is "file file:///home/user/notes.txt;folder file:///home/user/Music;"

  Rule: Qt's dialogs are lowered to Ljos.Controls.Basic

    @ljos-only @headless
    Scenario: A file of the dialogs is lowered, Ljos.Controls.Basic imported on the dialogs' line
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Dialogs
        Item {
          id: root
          property url chosen
          FileDialog { id: picker; fileMode: FileDialog.OpenFiles; onAccepted: root.chosen = selectedFile }
          FolderDialog { onAccepted: root.chosen = selectedFolder }
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
        import Ljos.Controls.Basic
        Item {
          id: root
          property url chosen
          FileDialog { id: picker; fileMode: FileDialog.OpenFiles; onAccepted: root.chosen = selectedFile }
          FolderDialog { onAccepted: root.chosen = selectedFolder }
        }
        """

    @ljos-only @headless
    Scenario: The dialogs' line is left empty where the file imports the controls
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        import QtQuick.Dialogs
        Button {
          text: "Open"
          onClicked: dialog.open()
          FileDialog { id: dialog }
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
        import Ljos.Controls.Basic
        Button {
          text: "Open"
          onClicked: dialog.open()
          FileDialog { id: dialog }
        }
        """

    @ljos-only @headless
    Scenario: A dialog Ljos.Controls.Basic has not taken yet keeps the file
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Dialogs
        Item {
          FileDialog { }
          ColorDialog { }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:5:3: `ColorDialog` is not in Ljos.Controls.Basic yet
        1 files, 0 lowered, 1 kept
        """

    @ljos-only @headless
    Scenario: An enum of a dialog Ljos.Controls.Basic has not taken yet keeps the file
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Dialogs
        Item { property int buttons: MessageDialog.Ok }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:3:30: `MessageDialog` is not in Ljos.Controls.Basic yet
        1 files, 0 lowered, 1 kept
        """

    @ljos-only @headless
    Scenario: The dialogs under another name keep the file
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Dialogs as D
        Item { D.FileDialog { } }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:2:1: it imports `QtQuick.Dialogs` under a name
        1 files, 0 lowered, 1 kept
        """

    # A Qt file finds a name its imports lack among the files beside it; the
    # lowered file imports Ljos.Controls.Basic, and an import is found first.
    @ljos-only @headless
    Scenario: A file of the project Ljos.Controls.Basic has a type of the same name for keeps the file
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Dialogs
        Item {
          Button { }
          FileDialog { }
        }
        """
      And the file "Button.qml":
        """qml
        import QtQuick
        Rectangle { width: 80; height: 30; color: "#336699" }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Button.qml:1:1: it is used by `Main.qml`, which is kept: `Button` is a file of the project, and Ljos.Controls.Basic, which the lowered file imports, has a `Button` that would be found first
        kept ./Main.qml:4:3: `Button` is a file of the project, and Ljos.Controls.Basic, which the lowered file imports, has a `Button` that would be found first
        2 files, 0 lowered, 2 kept
        """

    # A qualifier is found before a type of an import, in Qt and in Ljós,
    # so a script's `Box` stays the file's own beside Ljos.Ui's `Box`.
    @ljos-only @headless
    Scenario: An import qualifier named as a Ljós type is the file's own, and the file lowers
      Given the file "box.js":
        """
        .pragma library
        function helper() { return 7 }
        """
      And the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Dialogs
        import "box.js" as Box
        Item {
          property int d: Box.helper()
          FileDialog { }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Main.qml (2 edits)
        scripts of ./Main.qml stay JavaScript: typed, ljos check finds 1
        1 files, 1 lowered, 0 kept
        """
      And the file "Main.qml" is:
        """qml
        pragma Scripts: JavaScript
        import Ljos.Ui
        import Ljos.Controls.Basic
        import "box.js" as Box
        Item {
          property int d: Box.helper()
          FileDialog { }
        }
        """

    # Qt finds a name in the last import that has it. The lowered file has
    # one `import Ljos.Controls.Basic`, on the line of the first of the dialogs and
    # the controls: a module of the project imported between that line and
    # the controls' would be found before Ljós's `Label`, where Qt finds
    # Basic's.
    @ljos-only @headless
    Scenario: A module of the project imported between the dialogs and the controls keeps the file
      Given the file "app/Gauges/qmldir":
        """
        module Gauges
        Label 1.0 Label.qml
        """
      And the file "app/Gauges/Label.qml":
        """qml
        import QtQuick
        Rectangle { width: 60; height: 20 }
        """
      And the file "app/Main.qml":
        """qml
        import QtQuick
        import QtQuick.Dialogs
        import Gauges
        import QtQuick.Controls.Basic
        Item {
          Label { color: "red" }
          FileDialog { }
        }
        """
      When the author runs "ljos migrate app"
      Then the command prints:
        """
        kept app/Gauges/Label.qml:1:1: it is used by `Main.qml`, which is kept: `Label` is Qt's, but in the lowered file the project's `Label`, imported after the line Ljos.Controls.Basic takes, would be found first
        kept app/Main.qml:6:3: `Label` is Qt's, but in the lowered file the project's `Label`, imported after the line Ljos.Controls.Basic takes, would be found first
        2 files, 0 lowered, 2 kept
        """

    # A name a script declares is the script's in Qt and in Ljós, so a local
    # `Box` is not Ljos.Ui's `Box`.
    @ljos-only @headless
    Scenario: A local a script names as a Ljós type is the script's own, and the file lowers
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Dialogs
        Item {
          function size() { var Box = { w: 4 }; return Box.w }
          FileDialog { }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Main.qml (2 edits)
        scripts of ./Main.qml stay JavaScript: typed, ljos check finds 1
        1 files, 1 lowered, 0 kept
        """
      And the file "Main.qml" is:
        """qml
        pragma Scripts: JavaScript
        import Ljos.Ui
        import Ljos.Controls.Basic
        Item {
          function size() { var Box = { w: 4 }; return Box.w }
          FileDialog { }
        }
        """
