# Sources:
#   docs/DIALECT.md (Ljós's own API: §4.16 the controls, §6 the compat pass)
#   https://doc.qt.io/qt-6/qml-qtquick-controls-textfield.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-textarea.html
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# `Ljos.Controls` is Ljós's own module, and lowering a file to it is Ljós's: those scenarios are
# `@ljos-only`. The file as written for Qt Quick Controls runs on both, and the file in Ljós's API
# is held to its numbers. The shape is that of a form with a name, a search and a note, written
# here as a case of its own.

Feature: Ljós's controls: text fields
  Ljos.Controls has the text field and the text area of Qt Quick
  Controls' Basic style, edited as they are: a field says when it is
  edited, accepted and done with, and an area takes lines. A file of
  Qt's that uses them is lowered to Ljos.Controls with no other edit for
  them.

  Rule: A text field and a text area are Qt's

    @qt-parity
    Scenario: A form as written for Qt
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          width: 300; height: 200
          property int edits: 0
          property int accepts: 0
          property int finished: 0
          TextField {
            objectName: "name"; x: 10; y: 10; width: 200
            placeholderText: "Name"
            onTextEdited: root.edits += 1
            onAccepted: root.accepts += 1
            onEditingFinished: root.finished += 1
          }
          TextArea {
            objectName: "note"; x: 10; y: 70; width: 200
            placeholderText: "Note"; wrapMode: Text.Wrap
          }
        }
        """
      When the user presses the pointer at 50, 30
      And the user releases the pointer at 50, 30
      And the user types "Ada"
      And the user presses "Return"
      Then the property "text" of "name" is "Ada"
      And the property "edits" of "root" is 3
      And the property "accepts" of "root" is 1
      And the property "finished" of "root" is 1
      And the property "implicitHeight" of "name" is 40
      When the user presses the pointer at 50, 90
      And the user releases the pointer at 50, 90
      And the user types "one"
      And the user presses "Return"
      And the user types "two"
      Then the property "length" of "note" is 7
      And the property "lineCount" of "note" is 2
      And the property "finished" of "root" is 2

    @ljos-only
    Scenario: The same in Ljós's API
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          width: 300; height: 200
          property int edits: 0
          property int accepts: 0
          property int finished: 0
          TextField {
            objectName: "name"; x: 10; y: 10; width: 200
            placeholderText: "Name"
            onTextEdited: root.edits += 1
            onAccepted: root.accepts += 1
            onEditingFinished: root.finished += 1
          }
          TextArea {
            objectName: "note"; x: 10; y: 70; width: 200
            placeholderText: "Note"; wrapMode: Text.Wrap
          }
        }
        """
      When the user presses the pointer at 50, 30
      And the user releases the pointer at 50, 30
      And the user types "Ada"
      And the user presses "Return"
      Then the property "text" of "name" is "Ada"
      And the property "edits" of "root" is 3
      And the property "accepts" of "root" is 1
      And the property "finished" of "root" is 1
      And the property "implicitHeight" of "name" is 40
      When the user presses the pointer at 50, 90
      And the user releases the pointer at 50, 90
      And the user types "one"
      And the user presses "Return"
      And the user types "two"
      Then the property "length" of "note" is 7
      And the property "lineCount" of "note" is 2
      And the property "finished" of "root" is 2

    @ljos-only
    Scenario: The form as written for Qt, lowered, does the same
      Given Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          width: 300; height: 200
          property int edits: 0
          property int accepts: 0
          property int finished: 0
          TextField {
            objectName: "name"; x: 10; y: 10; width: 200
            placeholderText: "Name"
            onTextEdited: root.edits += 1
            onAccepted: root.accepts += 1
            onEditingFinished: root.finished += 1
          }
          TextArea {
            objectName: "note"; x: 10; y: 70; width: 200
            placeholderText: "Note"; wrapMode: Text.Wrap
          }
        }
        """
      When the user presses the pointer at 50, 30
      And the user releases the pointer at 50, 30
      And the user types "Ada"
      And the user presses "Return"
      Then the property "text" of "name" is "Ada"
      And the property "edits" of "root" is 3
      And the property "accepts" of "root" is 1
      And the property "finished" of "root" is 1
      And the property "implicitHeight" of "name" is 40
      When the user presses the pointer at 50, 90
      And the user releases the pointer at 50, 90
      And the user types "one"
      And the user presses "Return"
      And the user types "two"
      Then the property "length" of "note" is 7
      And the property "lineCount" of "note" is 2
      And the property "finished" of "root" is 2

  Rule: Qt's text fields are lowered to Ljos.Controls

    @ljos-only @headless
    Scenario: A file of text fields is lowered
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 240; height: 160; color: "white"
          TextField { x: 10; y: 10; width: 200; text: "Ada"; color: "#204080" }
          TextArea { x: 10; y: 60; width: 200; text: "one two"; wrapMode: Text.Wrap }
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
        pragma Scripts: JavaScript; import Ljos.Ui
        import Ljos.Controls
        Rectangle {
          width: 240; height: 160; fill: "white"
          TextField { x: 10; y: 10; width: 200; text: "Ada"; color: "#204080" }
          TextArea { x: 10; y: 60; width: 200; text: "one two"; wrapMode: Text.Wrap }
        }
        """

    @ljos-only @headless
    Scenario: A lowered file of text fields draws what it drew as written
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 240; height: 160; color: "white"
          TextField { x: 10; y: 10; width: 200; placeholderText: "Name" }
          TextArea { x: 10; y: 60; width: 200; text: "one two"; wrapMode: Text.Wrap }
        }
        """
      When the author runs "ljos render . --out kept.png --settle 0 --compat keep"
      And the author runs "ljos render . --out lowered.png --settle 0 --compat lower"
      Then the pictures "kept.png" and "lowered.png" have the same pixels

    @ljos-only @headless
    Scenario: A text area's wrapping named by TextEdit keeps the file
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        TextArea { text: "one two"; wrapMode: TextEdit.Wrap }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:3:39: `TextEdit` is not in Ljos.Ui yet
        1 files, 0 lowered, 1 kept
        """

    @ljos-only @headless
    Scenario: A file of text fields in Ljós's API has nothing to report
      Given the file "Form.qml":
        """qml
        import Ljos.Ui
        import Ljos.Controls
        Item {
          id: form
          property string name: ""
          Label { text: "Name" }
          TextField { y: 20; placeholderText: "Ada"; onAccepted: form.name = text }
          Label { y: 70; text: "Note" }
          TextArea { y: 90; placeholderText: "What to remember" }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """
