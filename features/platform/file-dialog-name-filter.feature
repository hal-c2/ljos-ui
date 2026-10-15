# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-dialogs-filedialog.html#selectedNameFilter.index-prop
#   https://doc.qt.io/qt-6/qml-qtquick-dialogs-filedialog.html#nameFilters-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: FileDialog.selectedNameFilter
  The selected name filter is one of the dialog's nameFilters, chosen by
  its index. It says that filter's name (the text before its parentheses),
  the extensions its patterns name and the patterns themselves.

  Scenario: No filter is selected until an index is set
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Dialogs
      Item {
        FileDialog {
          objectName: "d"
          nameFilters: ["Images (*.png *.jpg)", "Text files (*.txt)"]
          readonly property string text: [selectedNameFilter.index, selectedNameFilter.name,
              selectedNameFilter.extensions.join(","), selectedNameFilter.globs.join(",")].join("|")
        }
      }
      """
    Then the property "text" of "d" is "-1|||"

  Scenario Outline: An index set where the dialog is declared selects that filter
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Dialogs
      Item {
        FileDialog {
          objectName: "d"
          nameFilters: <filters>
          selectedNameFilter.index: <index>
          readonly property string text: [selectedNameFilter.index, selectedNameFilter.name,
              selectedNameFilter.extensions.join(","), selectedNameFilter.globs.join(",")].join("|")
        }
      }
      """
    Then the property "text" of "d" is "<text>"

    Examples:
      | filters                                          | index | text                                |
      | ["Images (*.png *.jpg)", "Text files (*.txt)"]   | 0     | 0\|Images\|png,jpg\|*.png,*.jpg    |
      | ["Images (*.png *.jpg)", "Text files (*.txt)"]   | 1     | 1\|Text files\|txt\|*.txt           |
      | ["Images (*.png *.jpg)", "All files (*)"]        | 1     | 1\|All files\|*\|*                  |
      | ["*.a", "plain"]                                 | 0     | 0\|*.a\|\|                          |
      | ["*.a", "plain"]                                 | 1     | 1\|plain\|\|                        |

  Scenario: An index beyond the filters selects the first
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Dialogs
      Item {
        FileDialog {
          objectName: "d"
          nameFilters: ["Images (*.png *.jpg)", "Text files (*.txt)"]
          selectedNameFilter.index: 7
          readonly property string text: [selectedNameFilter.name,
              selectedNameFilter.globs.join(",")].join("|")
        }
      }
      """
    Then the property "text" of "d" is "Images|*.png,*.jpg"

  # Qt fills name, extensions and globs when the index is declared and
  # leaves them as they were when a script sets it later ("1|||" on the
  # offscreen platform); Ljós keeps them following the index.
  @qt-diverges
  Scenario: The members follow an index written later
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Dialogs
      Item {
        FileDialog {
          id: d
          objectName: "d"
          nameFilters: ["Images (*.png *.jpg)", "Text files (*.txt)"]
          readonly property string text: [selectedNameFilter.index, selectedNameFilter.name,
              selectedNameFilter.extensions.join(","), selectedNameFilter.globs.join(",")].join("|")
          function pick() { selectedNameFilter.index = 1 }
        }
      }
      """
    When the method "pick" of "d" is called
    Then the property "text" of "d" is "1|Text files|txt|*.txt"
