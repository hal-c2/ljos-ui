# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-dialogs-filedialog.html
#   https://doc.qt.io/qt-6/qml-qtquick-dialogs-folderdialog.html
#   https://developer.android.com/training/data-storage/shared/documents-files
#   https://developer.android.com/reference/android/content/Intent#ACTION_OPEN_DOCUMENT
#   https://developer.android.com/reference/android/content/Intent#ACTION_CREATE_DOCUMENT
#   https://developer.android.com/reference/android/content/Intent#ACTION_OPEN_DOCUMENT_TREE
#   docs/ARCHITECTURE.md (§10.1: the platform's services)
#   https://github.com/hal-c2/ljos-ui/issues/616
# On a phone a file or folder dialog is the system's document picker, and
# what the user picks is a document of some provider (the phone's storage,
# a cloud drive), not a path. A document opened is copied into the
# application's cache, so whatever reads a file reads it; one to save, and
# a folder, are the provider's own URL. features/platform/dialogs.feature
# pins the dialogs anywhere, with a stand-in system.
@ljos-only @android
Feature: A file dialog on a phone is the system's document picker
  The application asks the system, and the system shows its picker, which
  takes the dialog's name filters as the kinds of document to offer.

  Background:
    Given the application may read local files through XMLHttpRequest
    And the phone's Downloads hold the file "notes.txt" with the text "hello from the phone"
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Dialogs
      Window {
        id: root; objectName: "root"; visible: true
        property string text: ""; property string log: ""
        FileDialog { objectName: "open"
          readonly property string scheme: String(selectedFile).split(":")[0]
          nameFilters: ["Text (*.txt)"]
          onAccepted: {
            const request = new XMLHttpRequest()
            request.open("GET", selectedFile, false); request.send()
            root.text = request.responseText
          }
          onRejected: root.log += "rejected"
        }
        FileDialog { objectName: "save"
          readonly property string scheme: String(selectedFile).split(":")[0]
          fileMode: FileDialog.SaveFile; currentFile: "file:///report.txt"
        }
        FolderDialog { objectName: "folder"
          readonly property string scheme: String(selectedFolder).split(":")[0]
        }
      }
      """

  Scenario: The document the user opens is read as a file
    When the method "open" of "open" is called
    Then the system shows an "open file" dialog titled ""
    When the user chooses "notes.txt" in the system dialog
    Then the property "scheme" of "open" is "file"
    And the property "text" of "root" is "hello from the phone"

  Scenario: Leaving the picker rejects the dialog
    When the method "open" of "open" is called
    And the user dismisses the system dialog
    Then the property "log" of "root" is "rejected"
    And the property "selectedFile" of "open" is ""

  Scenario: The document the user saves to is the provider's
    When the method "open" of "save" is called
    Then the system shows a "save file" dialog titled ""
    When the user chooses "report.txt" in the system dialog
    Then the property "scheme" of "save" is "content"
    And the phone's Downloads hold the file "report.txt"

  # Android lets no application take the whole of Downloads as a folder,
  # only a folder in it.
  Scenario: The folder the user picks is the provider's
    Given the phone's Downloads hold the file "letters/one.txt" with the text "a"
    When the method "open" of "folder" is called
    And the user chooses "letters" in the system dialog
    Then the property "scheme" of "folder" is "content"
