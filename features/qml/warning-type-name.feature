# Sources:
#   https://doc.qt.io/qt-6/qtqml-documents-definetypes.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: An object's own warning names the type its file declares
  A file declares a type named after the file. When an object of that type warns about itself,
  the warning names that type after "QML ", not the built-in type the file is made of. The rest
  of the message still names the built-in type.

  Scenario: A positioner a file declares warns under the file's name
    Given the file "MyCol.qml":
      """qml
      import QtQuick
      Column { Rectangle { width: 5; height: 5; anchors.top: parent.top } }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        MyCol {}
      }
      """
    Then the log has a warning matching "QML MyCol: Cannot specify .* for items inside Column. Column will not function.$"

  Scenario: An image a file declares warns under the file's name
    Given the file "MyImg.qml":
      """qml
      import QtQuick
      Image { source: "nothere.png" }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        MyImg {}
      }
      """
    Then the log has a warning matching "QML MyImg: Cannot open: .*nothere.png$"

  Scenario: The root of the main file warns under the main file's name
    Given the QML:
      """qml
      import QtQuick
      Column { Rectangle { width: 5; height: 5; anchors.top: parent.top } }
      """
    Then the log has a warning matching "QML Scenario: Cannot specify .* for items inside Column. Column will not function.$"

  Scenario: An object declared in place keeps the built-in name
    Given the QML:
      """qml
      import QtQuick
      Item {
        Column { property int extra; Rectangle { width: 5; height: 5; anchors.top: parent.top } }
      }
      """
    Then the log has a warning matching "QML Column: Cannot specify .* for items inside Column. Column will not function.$"
