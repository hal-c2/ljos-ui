# Sources:
#   docs/DIALECT.md (Ljós's own API: §4.16 the controls, §6 the compat pass)
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menu.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menuitem.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menuseparator.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-dialog.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-itemdelegate.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# `Ljos.Controls` is Ljós's own module, and lowering a file to it is Ljós's: those scenarios are
# `@ljos-only`. The file as written for Qt Quick Controls runs on both, and the file in Ljós's API
# is held to its numbers. The shapes are those of a list with a context menu and of a confirmation
# dialog, written here as cases of their own.

Feature: Ljós's controls: menus, dialogs and item delegates
  Ljos.Controls has the menu, its items and separators, the dialog and
  the item delegate of Qt Quick Controls' Basic style, opened, chosen
  from and closed as they are. A file of Qt's that uses them is lowered
  to Ljos.Controls with no other edit for them.

  Rule: A menu and an item delegate are Qt's

    @qt-parity
    Scenario: A row with a menu as written for Qt
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          width: 300; height: 240
          property string chosen: ""
          property int clicks: 0
          ItemDelegate {
            objectName: "row"; x: 10; y: 10; width: 200; text: "Notes.txt"
            onClicked: { root.clicks += 1; menu.open() }
          }
          Menu {
            id: menu; objectName: "menu"; x: 20; y: 60
            MenuItem { text: "Open"; onTriggered: root.chosen = text }
            MenuSeparator { }
            MenuItem { text: "Delete"; onTriggered: root.chosen = text }
          }
        }
        """
      When the user presses the pointer at 50, 30
      And the user releases the pointer at 50, 30
      And 300 milliseconds pass
      Then the property "clicks" of "root" is 1
      And the property "visible" of "menu" is true
      And the property "padding" of "row" is 12
      And the property "count" of "menu" is 3
      When the user presses "Down"
      And the user presses "Return"
      And 300 milliseconds pass
      Then the property "chosen" of "root" is "Open"
      And the property "visible" of "menu" is false

    @ljos-only
    Scenario: The same in Ljós's API
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          width: 300; height: 240
          property string chosen: ""
          property int clicks: 0
          ItemDelegate {
            objectName: "row"; x: 10; y: 10; width: 200; text: "Notes.txt"
            onClicked: { root.clicks += 1; menu.open() }
          }
          Menu {
            id: menu; objectName: "menu"; x: 20; y: 60
            MenuItem { text: "Open"; onTriggered: root.chosen = text }
            MenuSeparator { }
            MenuItem { text: "Delete"; onTriggered: root.chosen = text }
          }
        }
        """
      When the user presses the pointer at 50, 30
      And the user releases the pointer at 50, 30
      And 300 milliseconds pass
      Then the property "clicks" of "root" is 1
      And the property "visible" of "menu" is true
      And the property "padding" of "row" is 12
      And the property "count" of "menu" is 3
      When the user presses "Down"
      And the user presses "Return"
      And 300 milliseconds pass
      Then the property "chosen" of "root" is "Open"
      And the property "visible" of "menu" is false

    @ljos-only
    Scenario: The file as written for Qt, lowered, does the same
      Given Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          width: 300; height: 240
          property string chosen: ""
          property int clicks: 0
          ItemDelegate {
            objectName: "row"; x: 10; y: 10; width: 200; text: "Notes.txt"
            onClicked: { root.clicks += 1; menu.open() }
          }
          Menu {
            id: menu; objectName: "menu"; x: 20; y: 60
            MenuItem { text: "Open"; onTriggered: root.chosen = text }
            MenuSeparator { }
            MenuItem { text: "Delete"; onTriggered: root.chosen = text }
          }
        }
        """
      When the user presses the pointer at 50, 30
      And the user releases the pointer at 50, 30
      And 300 milliseconds pass
      Then the property "clicks" of "root" is 1
      And the property "visible" of "menu" is true
      And the property "padding" of "row" is 12
      And the property "count" of "menu" is 3
      When the user presses "Down"
      And the user presses "Return"
      And 300 milliseconds pass
      Then the property "chosen" of "root" is "Open"
      And the property "visible" of "menu" is false

  Rule: A dialog is Qt's

    @qt-parity
    Scenario: A dialog as written for Qt
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          width: 300; height: 240
          property int accepts: 0
          property int rejects: 0
          Dialog {
            id: dialog; objectName: "dialog"; x: 20; y: 20; width: 240
            title: "Delete the file?"; modal: true
            standardButtons: Dialog.Ok | Dialog.Cancel
            onAccepted: root.accepts += 1
            onRejected: root.rejects += 1
          }
        }
        """
      When the method "open" of "dialog" is called
      And 300 milliseconds pass
      Then the property "visible" of "dialog" is true
      When the method "accept" of "dialog" is called
      And 300 milliseconds pass
      Then the property "accepts" of "root" is 1
      And the property "visible" of "dialog" is false
      When the method "open" of "dialog" is called
      And 300 milliseconds pass
      And the user presses "Escape"
      And 300 milliseconds pass
      Then the property "rejects" of "root" is 1
      And the property "visible" of "dialog" is false

    @ljos-only
    Scenario: The same in Ljós's API
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          width: 300; height: 240
          property int accepts: 0
          property int rejects: 0
          Dialog {
            id: dialog; objectName: "dialog"; x: 20; y: 20; width: 240
            title: "Delete the file?"; modal: true
            standardButtons: Dialog.Ok | Dialog.Cancel
            onAccepted: root.accepts += 1
            onRejected: root.rejects += 1
          }
        }
        """
      When the method "open" of "dialog" is called
      And 300 milliseconds pass
      Then the property "visible" of "dialog" is true
      When the method "accept" of "dialog" is called
      And 300 milliseconds pass
      Then the property "accepts" of "root" is 1
      And the property "visible" of "dialog" is false
      When the method "open" of "dialog" is called
      And 300 milliseconds pass
      And the user presses "Escape"
      And 300 milliseconds pass
      Then the property "rejects" of "root" is 1
      And the property "visible" of "dialog" is false

    @ljos-only
    Scenario: The file as written for Qt, lowered, does the same
      Given Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          width: 300; height: 240
          property int accepts: 0
          property int rejects: 0
          Dialog {
            id: dialog; objectName: "dialog"; x: 20; y: 20; width: 240
            title: "Delete the file?"; modal: true
            standardButtons: Dialog.Ok | Dialog.Cancel
            onAccepted: root.accepts += 1
            onRejected: root.rejects += 1
          }
        }
        """
      When the method "open" of "dialog" is called
      And 300 milliseconds pass
      Then the property "visible" of "dialog" is true
      When the method "accept" of "dialog" is called
      And 300 milliseconds pass
      Then the property "accepts" of "root" is 1
      And the property "visible" of "dialog" is false
      When the method "open" of "dialog" is called
      And 300 milliseconds pass
      And the user presses "Escape"
      And 300 milliseconds pass
      Then the property "rejects" of "root" is 1
      And the property "visible" of "dialog" is false

  Rule: Qt's menus and dialogs are lowered to Ljos.Controls

    @ljos-only @headless
    Scenario: A file of menus and dialogs is lowered
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 240; height: 160; color: "white"
          ItemDelegate { width: 200; text: "Notes.txt"; onClicked: menu.open() }
          Menu { id: menu; MenuItem { text: "Open" } MenuSeparator { } }
          Dialog { title: "Delete?"; standardButtons: Dialog.Ok | Dialog.Cancel }
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
          ItemDelegate { width: 200; text: "Notes.txt"; onClicked: menu.open() }
          Menu { id: menu; MenuItem { text: "Open" } MenuSeparator { } }
          Dialog { title: "Delete?"; standardButtons: Dialog.Ok | Dialog.Cancel }
        }
        """

    @ljos-only @headless
    Scenario: A lowered file of an open dialog draws what it drew as written
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 240; height: 160; color: "white"
          ItemDelegate { width: 200; text: "Notes.txt" }
          Dialog {
            x: 20; y: 40; width: 200; visible: true
            title: "Delete?"; standardButtons: Dialog.Ok | Dialog.Cancel
          }
        }
        """
      When the author runs "ljos render . --out kept.png --settle 0 --compat keep"
      And the author runs "ljos render . --out lowered.png --settle 0 --compat lower"
      Then the pictures "kept.png" and "lowered.png" have the same pixels

    @ljos-only @headless
    Scenario: A file of menus and dialogs in Ljós's API has nothing to report
      Given the file "List.qml":
        """qml
        import Ljos.Ui
        import Ljos.Controls
        Item {
          id: list
          property string chosen: ""
          ItemDelegate { text: "Notes.txt"; onClicked: menu.open() }
          Menu { id: menu; MenuItem { text: "Open"; onTriggered: list.chosen = text } }
          Dialog { title: "Delete?"; standardButtons: Dialog.Ok | Dialog.Cancel }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """
