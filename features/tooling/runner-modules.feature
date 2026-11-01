# Sources:
#   https://doc.qt.io/qt-6/qtquick-localstorage-qmlmodule.html
#   https://doc.qt.io/qt-6/qtqml-xmllistmodel-qmlmodule.html
#   https://doc.qt.io/qt-6/qml-qt-labs-folderlistmodel-folderlistmodel.html
# Verified on Qt 6.11.2: `qml6` loads a file that imports any of these
# modules. A shipped Ljós application registers the ones it uses; `ljos
# run`, `ljos render` and `ljos test` stand where `qml6` and
# `qmltestrunner` stand, so they have every one of them. The test runner's
# side is checked against Qt in qttest-suites.feature.

@ljos-only @headless
Feature: The modules the ljos command has
  A project run or rendered with the `ljos` command can import every
  module Ljós has, not only QtQuick and its controls, as a project run
  with `qml6` can import every module Qt installed.

  Scenario: A rendered project imports the database, XML and folder modules
    Given the file "Main.qml":
      """qml
      import QtQuick
      import QtQuick.LocalStorage
      import QtQml.XmlListModel
      import Qt.labs.folderlistmodel
      Item {
        XmlListModel { id: feed }
        FolderListModel { id: folder }
        Component.onCompleted: {
          let db = LocalStorage.openDatabaseSync("Notes", "1.0", "notes")
          console.warn("made " + (feed !== null) + " " + (folder !== null) + " " + db.version)
        }
      }
      """
    When the author runs "ljos render . --out shot.png --settle 0"
    Then the command's output mentions "made true true 1.0"
    And the command's output does not mention "is not installed"

  Scenario: A rendered project makes an object from text
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item {
        Component.onCompleted: {
          let made = Qt.createQmlObject("import QtQuick; Rectangle { width: 7 }", this)
          console.warn("made " + made.width)
        }
      }
      """
    When the author runs "ljos render . --out shot.png --settle 0"
    Then the command's output mentions "made 7"
    And the command's output does not mention "has no compiler"

  Scenario: A file URL with no slash is relative to the working directory
    Given the file "Main.qml":
      """qml
      import QtQuick
      ShaderEffect {
        width: 40; height: 40
        fragmentShader: "file:shaders/plain.frag"
        Component.onCompleted: console.warn("shader status " + status)
      }
      """
    And the file "shaders/plain.frag":
      """
      #version 440
      layout(location = 0) out vec4 fragColor;
      void main() { fragColor = vec4(1.0); }
      """
    When the author runs "ljos render . --out shot.png --settle 0"
    Then the command's output does not mention "Failed to find shader"
    And the command's output does not mention "preparation failed"
