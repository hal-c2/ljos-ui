# Sources:
#   https://doc.qt.io/qt-6/qtquickdialogs-index.html
#   https://doc.qt.io/qt-6/qml-qtquick-dialogs-filedialog.html
#   https://doc.qt.io/qt-6/qml-qtquick-dialogs-folderdialog.html
#   https://doc.qt.io/qt-6/qml-qtquick-dialogs-colordialog.html
#   https://doc.qt.io/qt-6/qml-qtquick-dialogs-fontdialog.html
#   https://doc.qt.io/qt-6/qml-qtquick-dialogs-messagedialog.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: The dialogs of QtQuick.Dialogs
  A file, folder, colour, font or message dialog is declared in QML and
  opened with open(). The system shows it when it has a dialog of that kind;
  otherwise the application draws its own. Either way the dialog ends
  accepted or rejected, with what the user chose in its properties.

  Rule: What a dialog starts as

    @qt-parity
    Scenario Outline: A <type> starts with <property> <value>
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Dialogs
        Item { <type> { objectName: "d" } }
        """
      Then the property "<property>" of "d" is <value>

      Examples:
        | type          | property       | value             |
        | FileDialog    | fileMode       | 0                 |
        | FileDialog    | nameFilters    | ["All Files (*)"] |
        | FileDialog    | selectedFile   | ""                |
        | FileDialog    | options        | 0                 |
        | FileDialog    | defaultSuffix  | ""                |
        | FileDialog    | visible        | false             |
        | FileDialog    | result         | 0                 |
        | FileDialog    | modality       | 1                 |
        | FolderDialog  | selectedFolder | ""                |
        | FolderDialog  | acceptLabel    | ""                |
        | ColorDialog   | selectedColor  | "#ffffff"         |
        | ColorDialog   | options        | 0                 |
        | FontDialog    | options        | 0                 |
        | MessageDialog | buttons        | 1024              |
        | MessageDialog | text           | ""                |
        | MessageDialog | informativeText | ""               |

    @qt-parity
    Scenario Outline: <name> is <value>
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Dialogs
        Item { objectName: "o"; property var v: <name> }
        """
      Then the property "v" of "o" is <value>

      Examples:
        | name                             | value   |
        | FileDialog.OpenFile              | 0       |
        | FileDialog.OpenFiles             | 1       |
        | FileDialog.SaveFile              | 2       |
        | FileDialog.DontUseNativeDialog   | 8       |
        | FileDialog.ReadOnly              | 16      |
        | FolderDialog.ShowDirsOnly        | 1       |
        | ColorDialog.ShowAlphaChannel     | 1       |
        | ColorDialog.DontUseNativeDialog  | 4       |
        | FontDialog.MonospacedFonts       | 16      |
        | FontDialog.DontUseNativeDialog   | 2       |
        | MessageDialog.Ok                 | 1024    |
        | MessageDialog.Cancel             | 4194304 |
        | MessageDialog.AcceptRole         | 0       |
        | MessageDialog.DestructiveRole    | 2       |
        | FileDialog.Accepted              | 1       |
        | FileDialog.Rejected              | 0       |

  Rule: A dialog opens and ends from QML

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Dialogs
        Item {
          id: root
          objectName: "root"
          property string log: ""
          FolderDialog {
            objectName: "d"
            onAccepted: root.log += "accepted;"
            onRejected: root.log += "rejected;"
          }
        }
        """

    @qt-parity @api-QtQuick.Dialogs.FolderDialog.open
    Scenario: open() shows the dialog
      When the method "open" of "d" is called
      Then the property "visible" of "d" is true

    @qt-parity @api-QtQuick.Dialogs.FolderDialog.accept
    Scenario: accept() closes the dialog accepted and tells
      When the method "open" of "d" is called
      And the method "accept" of "d" is called
      Then the property "visible" of "d" is false
      And the property "result" of "d" is 1
      And the property "log" of "root" is "accepted;"

    @qt-parity @api-QtQuick.Dialogs.FolderDialog.reject
    Scenario: reject() closes the dialog rejected and tells
      When the method "open" of "d" is called
      And the method "reject" of "d" is called
      Then the property "visible" of "d" is false
      And the property "result" of "d" is 0
      And the property "log" of "root" is "rejected;"

    # The documentation does not say that close() rejects; Qt 6.11.2 does,
    # for an open dialog of every kind, and does nothing for a closed one.
    @qt-parity @api-QtQuick.Dialogs.FolderDialog.close
    Scenario: close() closes an open dialog rejected
      When the method "open" of "d" is called
      And the method "close" of "d" is called
      Then the property "visible" of "d" is false
      And the property "result" of "d" is 0
      And the property "log" of "root" is "rejected;"

    @qt-parity @api-QtQuick.Dialogs.FolderDialog.close
    Scenario: close() of a closed dialog does nothing
      When the method "close" of "d" is called
      Then the property "visible" of "d" is false
      And the property "log" of "root" is ""

    @qt-parity @api-QtQuick.Dialogs.FolderDialog.done
    Scenario: done() with Accepted ends the dialog as accept() does
      When the method "open" of "d" is called
      And the method "done" of "d" is called with 1
      Then the property "visible" of "d" is false
      And the property "result" of "d" is 1
      And the property "log" of "root" is "accepted;"

  # The oracle cannot answer a dialog the system shows; Ljós's tests stand in
  # for the system and answer it.
  @ljos-only
  Rule: The system shows a file or folder dialog

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Dialogs
        Item {
          id: root
          objectName: "root"
          property string log: ""
          FileDialog {
            objectName: "d"
            title: "Open notes"
            onAccepted: root.log += "accepted " + selectedFile
            onRejected: root.log += "rejected"
          }
          FolderDialog { objectName: "folder"; title: "Where" }
        }
        """

    Scenario: Opening a file dialog asks the system for one
      When the method "open" of "d" is called
      Then the system shows an "open file" dialog titled "Open notes"

    Scenario: The file the user chooses is the selected file
      When the method "open" of "d" is called
      And the user chooses "/home/user/notes.txt" in the system dialog
      Then the property "selectedFile" of "d" is "file:///home/user/notes.txt"
      And the property "visible" of "d" is false
      And the property "result" of "d" is 1
      And the property "log" of "root" is "accepted file:///home/user/notes.txt"
      And the system shows no dialog

    Scenario: Dismissing the system's dialog rejects
      When the method "open" of "d" is called
      And the user dismisses the system dialog
      Then the property "visible" of "d" is false
      And the property "log" of "root" is "rejected"
      And the property "selectedFile" of "d" is ""

    Scenario Outline: A file dialog in the mode <mode> asks for <kind>
      When the property "fileMode" of "d" is set to <mode>
      And the method "open" of "d" is called
      Then the system shows a "<kind>" dialog titled "Open notes"

      Examples:
        | mode | kind       |
        | 1    | open files |
        | 2    | save file  |

    Scenario: Every file the user chooses is among the selected files
      When the property "fileMode" of "d" is set to 1
      And the method "open" of "d" is called
      And the user chooses the files "/a.txt, /b c.txt" in the system dialog
      # A URL reads back as Qt writes it for a person, the space kept.
      Then the property "selectedFiles" of "d" is ["file:///a.txt", "file:///b c.txt"]
      And the property "selectedFile" of "d" is "file:///a.txt"

    Scenario: Closing the dialog from QML takes the system's dialog away
      When the method "open" of "d" is called
      And the method "close" of "d" is called
      Then the system shows no dialog
      And the property "log" of "root" is "rejected"

    # rfd cannot take away a dialog it showed. QML sees what Qt's close()
    # does all the same; the system's dialog stays on screen until the user
    # ends it, and that answer goes nowhere.
    Scenario: A dialog the system cannot take away is closed for QML all the same
      Given the system cannot take its dialogs away
      When the method "open" of "d" is called
      And the method "close" of "d" is called
      Then the property "visible" of "d" is false
      And the property "result" of "d" is 0
      And the property "log" of "root" is "rejected"
      And the system shows an "open file" dialog titled "Open notes"

    Scenario: The late answer of a dialog closed from QML is not heard
      Given the system cannot take its dialogs away
      When the method "open" of "d" is called
      And the method "close" of "d" is called
      And the user chooses "/home/user/notes.txt" in the system dialog
      Then the property "log" of "root" is "rejected"
      And the property "selectedFile" of "d" is ""
      And the property "visible" of "d" is false
      And the system shows no dialog

    Scenario: Opened again, the dialog hears the system's new dialog
      Given the system cannot take its dialogs away
      When the method "open" of "d" is called
      And the method "close" of "d" is called
      And the method "open" of "d" is called
      And the user chooses "/home/user/notes.txt" in the system dialog
      Then the property "log" of "root" is "rejectedaccepted file:///home/user/notes.txt"
      And the property "visible" of "d" is false

    Scenario: A folder dialog asks for a folder and selects it
      When the method "open" of "folder" is called
      And the user chooses "/home/user/Music" in the system dialog
      Then the property "selectedFolder" of "folder" is "file:///home/user/Music"

    Scenario: A dialog told not to be the system's is drawn by the application
      When the property "options" of "d" is set to 8
      And the method "open" of "d" is called
      Then the system shows no dialog
      And the accessibility tree has a "dialog" named "Open notes"

  @ljos-only
  Rule: The system shows a message with buttons it knows

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Dialogs
        Item {
          id: root
          objectName: "root"
          property string log: ""
          MessageDialog {
            objectName: "m"
            title: "Delete"
            text: "Delete the file?"
            buttons: MessageDialog.Ok | MessageDialog.Cancel
            onButtonClicked: (button, role) => root.log += button + "/" + role + ";"
            onAccepted: root.log += "accepted;"
            onRejected: root.log += "rejected;"
          }
        }
        """

    Scenario: A message the system can show is shown by the system
      When the method "open" of "m" is called
      Then the system shows a "message" dialog titled "Delete"

    @api-QtQuick.Dialogs.MessageDialog.buttonClicked
    Scenario: The button the user presses is told with its role
      When the method "open" of "m" is called
      And the user answers "OK" in the system dialog
      Then the property "log" of "root" is "1024/0;accepted;"
      And the property "visible" of "m" is false

    Scenario: The button that rejects ends the dialog rejected
      When the method "open" of "m" is called
      And the user answers "Cancel" in the system dialog
      Then the property "log" of "root" is "4194304/1;rejected;"

  # Qt's own non-native dialogs are not the ones Ljós draws, and the oracle
  # cannot read an accessibility tree.
  @ljos-only @a11y
  Rule: The application draws a colour dialog

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Dialogs
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string log: ""
          ColorDialog {
            objectName: "d"
            title: "Pick"
            selectedColor: "#336699"
            onAccepted: root.log += "accepted"
            onRejected: root.log += "rejected"
          }
        }
        """
      And an assistive technology is listening

    Scenario: A colour dialog is not in the tree until it opens
      Then the accessibility tree has no "dialog" named "Pick"
      And the system shows no dialog

    @api-QtQuick.Dialogs.ColorDialog.title
    Scenario: An open colour dialog is a dialog named by its title
      When the method "open" of "d" is called
      Then the accessibility tree has a "dialog" named "Pick"
      And the "button" named "OK" is inside the "dialog" named "Pick"

    Scenario: The keyboard is inside the open dialog, on the colour's text
      When the method "open" of "d" is called
      Then the accessibility tree's focus is the "textinput" named "Hex"
      And the "textinput" named "Hex" has the value "#336699"

    Scenario: Escape closes the dialog rejected
      When the method "open" of "d" is called
      And the user presses "Escape"
      Then the property "visible" of "d" is false
      And the property "log" of "root" is "rejected"
      And the accessibility tree has no "dialog" named "Pick"

    @api-QtQuick.Dialogs.ColorDialog.selectedColor
    Scenario: OK accepts the colour the user typed
      When the method "open" of "d" is called
      And the user presses "Ctrl+A"
      And the user types "#ff8000"
      And the user clicks the "button" "OK"
      Then the property "selectedColor" of "d" is "#ff8000"
      And the property "log" of "root" is "accepted"

    Scenario: Cancel keeps the colour selected before
      When the method "open" of "d" is called
      And the user presses "Ctrl+A"
      And the user types "#ff8000"
      And the user clicks the "button" "Cancel"
      Then the property "selectedColor" of "d" is "#336699"
      And the property "log" of "root" is "rejected"

  @ljos-only @a11y
  Rule: The application draws a font dialog

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Dialogs
        Item {
          width: 400; height: 400
          FontDialog { objectName: "d"; title: "Font"; selectedFont.family: "Liberation Sans" }
        }
        """
      And an assistive technology is listening

    @api-QtQuick.Dialogs.FontDialog.title
    Scenario: An open font dialog is a dialog named by its title
      When the method "open" of "d" is called
      Then the accessibility tree has a "dialog" named "Font"
      And the "checkbox" named "Bold" is inside the "dialog" named "Font"

    @api-QtQuick.Dialogs.FontDialog.selectedFont
    Scenario: The style the user picks is in the selected font
      When the method "open" of "d" is called
      And the user clicks the "checkbox" "Bold"
      And the user clicks the "button" "OK"
      Then the property "selectedFont.bold" of "d" is true
      And the property "selectedFont.family" of "d" is "Liberation Sans"

  @ljos-only @a11y
  Rule: The application draws a message the system cannot show

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Dialogs
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string log: ""
          MessageDialog {
            objectName: "m"
            title: "Unsaved"
            text: "Save the changes?"
            buttons: MessageDialog.Save | MessageDialog.Discard | MessageDialog.Cancel
            onButtonClicked: (button, role) => root.log += button + "/" + role + ";"
            onAccepted: root.log += "accepted;"
          }
        }
        """
      And an assistive technology is listening

    Scenario: The message and its buttons are inside the dialog
      When the method "open" of "m" is called
      Then the system shows no dialog
      And the "label" named "Save the changes?" is inside the "dialog" named "Unsaved"
      And the "button" named "Close without Saving" is inside the "dialog" named "Unsaved"

    Scenario: A button of the drawn message is told with its role
      When the method "open" of "m" is called
      And the user clicks the "button" "Save"
      Then the property "log" of "root" is "2048/0;accepted;"
      And the property "visible" of "m" is false
