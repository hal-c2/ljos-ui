# Sources:
#   https://doc.qt.io/qt-6/qml-qt-labs-folderlistmodel-folderlistmodel.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: FolderListModel
  A FolderListModel makes a read-only model of the files and folders in a
  folder, one row each, with the file's name, path, address, size and
  times as roles. The folder is read in the background: the model says
  Loading, then Ready when its rows are there. Names starting with a dot
  are hidden, name filters pick files and not folders, and the rows are
  sorted by name unless the model is told otherwise.

  Background:
    Given the text file "d/A.jpg":
      """
      12345
      """
    And the text file "d/b.png":
      """
      abc
      """
    And the text file "d/c.txt":
      """
      x
      """
    And the text file "d/a.tar.gz":
      """
      q
      """
    And the text file "d/.secret.png":
      """
      h
      """
    And the text file "d/sub/inner.txt":
      """
      i
      """
    And the text file "d/Zdir/z.txt":
      """
      z
      """

  @api-Qt.labs.folderlistmodel.FolderListModel.sortField
  @api-Qt.labs.folderlistmodel.FolderListModel.status
  Scenario Outline: FolderListModel.<name> is <value>
    Given the QML:
      """qml
      import QtQuick
      import Qt.labs.folderlistmodel
      Item { objectName: "o"; property var v: FolderListModel.<name> }
      """
    Then the property "v" of "o" is <value>

    Examples:
      | name     | value |
      | Unsorted | 0     |
      | Name     | 1     |
      | Time     | 2     |
      | Size     | 3     |
      | Type     | 4     |
      | Null     | 0     |
      | Ready    | 1     |
      | Loading  | 2     |

  @api-Qt.labs.folderlistmodel.FolderListModel.folder
  @api-Qt.labs.folderlistmodel.FolderListModel.count
  Scenario: The rows are the folder's files and folders, by name
    Given the QML:
      """qml
      import QtQuick
      import Qt.labs.folderlistmodel
      Item {
        id: root; objectName: "o"
        property string seen: ""
        property string names: ""
        FolderListModel {
          id: m; folder: Qt.resolvedUrl("d")
          onStatusChanged: root.seen += status + ";"
          onCountChanged: {
            let r = []
            for (let i = 0; i < count; i++) r.push(get(i, "fileName"))
            root.names = r.join(",")
          }
        }
        Repeater {
          id: shown; model: m
          delegate: Item { required property string fileName; objectName: "row-" + fileName }
        }
        property int made: shown.count
      }
      """
    When 100 milliseconds pass
    Then the property "seen" of "o" is "2;1;"
    And the property "names" of "o" is "A.jpg,Zdir,a.tar.gz,b.png,c.txt,sub"
    And the property "made" of "o" is 6

  @api-Qt.labs.folderlistmodel.FolderListModel.get
  Scenario: A row's roles describe its file
    Given the QML:
      """qml
      import QtQuick
      import Qt.labs.folderlistmodel
      Item {
        objectName: "o"
        readonly property string here: String(Qt.resolvedUrl("x")).replace(/x$/, "")
        FolderListModel { id: m; folder: Qt.resolvedUrl("d") }
        readonly property int row: m.count > 2 ? 2 : -1
        property string name: m.get(row, "fileName") ?? ""
        property string base: m.get(row, "fileBaseName") ?? ""
        property string suffix: m.get(row, "fileSuffix") ?? ""
        property string url: String(m.get(row, "fileUrl")).replace(here, "@/")
        property bool pathIsUrls: "file://" + m.get(row, "filePath") === String(m.get(row, "fileUrl"))
        property bool dated: m.get(row, "fileModified") instanceof Date
        property bool fileIsDir: m.get(row, "fileIsDir") ?? true
        property bool dirIsDir: m.get(row - 1, "fileIsDir") ?? false
        property bool folder: row > 0 && m.isFolder(row - 1)
        property bool missing: m.get(row, "nothing") === undefined
        property bool past: m.count > 0 && m.get(99, "fileName") === undefined
      }
      """
    When 100 milliseconds pass
    Then the property "name" of "o" is "a.tar.gz"
    And the property "base" of "o" is "a"
    And the property "suffix" of "o" is "tar.gz"
    And the property "url" of "o" is "@/d/a.tar.gz"
    And the property "pathIsUrls" of "o" is true
    And the property "dated" of "o" is true
    And the property "fileIsDir" of "o" is false
    And the property "dirIsDir" of "o" is true
    And the property "folder" of "o" is true
    And the property "missing" of "o" is true
    And the property "past" of "o" is true

  @api-Qt.labs.folderlistmodel.FolderListModel.indexOf
  Scenario: indexOf finds a file by its address
    Given the QML:
      """qml
      import QtQuick
      import Qt.labs.folderlistmodel
      Item {
        objectName: "o"
        FolderListModel { id: m; folder: Qt.resolvedUrl("d") }
        property int found: m.count, m.indexOf(Qt.resolvedUrl("d/c.txt"))
        property int relative: m.count, m.indexOf("c.txt")
      }
      """
    When 100 milliseconds pass
    Then the property "found" of "o" is 4
    And the property "relative" of "o" is -1

  Scenario Outline: With <setting> the rows are <names>
    Given the QML:
      """qml
      import QtQuick
      import Qt.labs.folderlistmodel
      Item {
        id: root; objectName: "o"
        property string names: ""
        FolderListModel {
          id: m; folder: Qt.resolvedUrl("d"); <setting>
          onStatusChanged: {
            let r = []
            for (let i = 0; i < count; i++) r.push(get(i, "fileName"))
            root.names = r.join(",")
          }
        }
      }
      """
    When 100 milliseconds pass
    Then the property "names" of "o" is "<names>"

    Examples:
      | setting                                       | names                                             |
      | sortCaseSensitive: false                      | A.jpg,a.tar.gz,b.png,c.txt,sub,Zdir               |
      | showDirsFirst: true                           | Zdir,sub,A.jpg,a.tar.gz,b.png,c.txt               |
      | sortReversed: true                            | sub,c.txt,b.png,a.tar.gz,Zdir,A.jpg               |
      | sortReversed: true; showDirsFirst: true       | sub,Zdir,c.txt,b.png,a.tar.gz,A.jpg               |
      | sortField: FolderListModel.Size               | Zdir,sub,A.jpg,b.png,a.tar.gz,c.txt               |
      | sortField: FolderListModel.Type               | Zdir,sub,a.tar.gz,A.jpg,b.png,c.txt               |
      | showHidden: true                              | .secret.png,A.jpg,Zdir,a.tar.gz,b.png,c.txt,sub   |
      | showDotAndDotDot: true                        | .,..,A.jpg,Zdir,a.tar.gz,b.png,c.txt,sub          |
      | showDirs: false                               | A.jpg,a.tar.gz,b.png,c.txt                        |
      | showFiles: false                              | Zdir,sub                                          |
      | nameFilters: ["*.png", "*.JPG"]               | Zdir,b.png,sub                                    |
      | nameFilters: ["*.JPG"]; caseSensitive: false  | A.jpg,Zdir,sub                                    |
      | nameFilters: ["?.*"]                          | A.jpg,Zdir,a.tar.gz,b.png,c.txt,sub               |

  Scenario: Name filters bound to another object's list pick the first rows
    Given the QML:
      """qml
      import QtQuick
      import Qt.labs.folderlistmodel
      Item {
        id: root; objectName: "o"
        property var filters: ["*.png", "*.jpg"]
        property string names: ""
        FolderListModel {
          id: m; folder: Qt.resolvedUrl("d"); showDirs: false
          nameFilters: root.filters
          onStatusChanged: {
            let r = []
            for (let i = 0; i < count; i++) r.push(get(i, "fileName"))
            root.names = r.join(",")
          }
        }
      }
      """
    When 100 milliseconds pass
    Then the property "names" of "o" is "A.jpg,b.png"

  @api-Qt.labs.folderlistmodel.FolderListModel.parentFolder
  Scenario: The parent folder is the one the folder is in
    Given the QML:
      """qml
      import QtQuick
      import Qt.labs.folderlistmodel
      Item {
        objectName: "o"
        readonly property string here: String(Qt.resolvedUrl("x")).replace(/x$/, "")
        FolderListModel { id: m; folder: Qt.resolvedUrl("d/sub") }
        FolderListModel { id: top; folder: "file:///" }
        property string up: String(m.parentFolder).replace(here, "@/")
        property string none: String(top.parentFolder)
      }
      """
    When 100 milliseconds pass
    Then the property "up" of "o" is "@/d"
    And the property "none" of "o" is ""

  Scenario: Another folder empties the model, then lists its own files
    Given the QML:
      """qml
      import QtQuick
      import Qt.labs.folderlistmodel
      Item {
        id: root; objectName: "o"
        property string seen: ""
        function go(f) { m.folder = Qt.resolvedUrl(f) }
        // Between the two lists Qt may or may not say the count is 0.
        readonly property bool listedTwice:
          seen.startsWith("s2;c6;s1;") && seen.endsWith("s2;c1;s1;")
        FolderListModel {
          id: m; objectName: "m"; folder: Qt.resolvedUrl("d")
          onStatusChanged: root.seen += "s" + status + ";"
          onCountChanged: root.seen += "c" + count + ";"
        }
      }
      """
    When 100 milliseconds pass
    And the method "go" of "o" is called with "d/sub"
    And 100 milliseconds pass
    Then the property "listedTwice" of "o" is true
    And the property "count" of "m" is 1

  Scenario: A folder that is not there empties the model
    Given the QML:
      """qml
      import QtQuick
      import Qt.labs.folderlistmodel
      Item {
        objectName: "o"
        FolderListModel { id: m; objectName: "m"; folder: Qt.resolvedUrl("d") }
        function go(f) { m.folder = Qt.resolvedUrl(f) }
      }
      """
    When 100 milliseconds pass
    And the method "go" of "o" is called with "nothere"
    And 100 milliseconds pass
    Then the property "count" of "m" is 0
    And the property "status" of "m" is 0
