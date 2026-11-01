# Sources:
#   https://doc.qt.io/qt-6/qfileselector.html
#   https://doc.qt.io/qt-6/qqmlfileselector.html
#   https://doc.qt.io/qt-6/qtquickcontrols-styles.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A file in a +selector directory stands in for the file beside it
  A file of a directory named after the running Controls style, or after the
  platform, is what a program gets when it asks for the file of that name one
  level up. Any other such directory is left alone. The style counts for a
  program that imports the Controls.

  Background:
    Given the file "Badge.qml":
      """qml
      import QtQuick
      Item { property string from: "plain" }
      """
    And the file "+Material/Badge.qml":
      """qml
      import QtQuick
      Item { property string from: "material" }
      """
    And the file "+Universal/Badge.qml":
      """qml
      import QtQuick
      Item { property string from: "universal" }
      """

  Scenario Outline: The running style chooses the file
    Given the application starts with the controls style "<style>"
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item { Badge { objectName: "badge" } }
      """
    Then the property "from" of "badge" is "<from>"

    Examples:
      | style     | from      |
      | Material  | material  |
      | Universal | universal |
      | Basic     | plain     |

  Scenario: A directory that names nothing running is left alone
    Given the file "Tag.qml":
      """qml
      import QtQuick
      Item { property string from: "plain" }
      """
    And the file "+nothing_of_the_kind/Tag.qml":
      """qml
      import QtQuick
      Item { property string from: "other" }
      """
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item { Tag { objectName: "tag" } }
      """
    Then the property "from" of "tag" is "plain"

  Scenario: The platform chooses a file too
    Given the file "Tag.qml":
      """qml
      import QtQuick
      Item { property string from: "plain" }
      """
    And the file "+unix/Tag.qml":
      """qml
      import QtQuick
      Item { property string from: "unix" }
      """
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item { Tag { objectName: "tag" } }
      """
    Then the property "from" of "tag" is "unix"

  Scenario: The style comes before the platform
    Given the application starts with the controls style "Material"
    And the file "Tag.qml":
      """qml
      import QtQuick
      Item { property string from: "plain" }
      """
    And the file "+unix/Tag.qml":
      """qml
      import QtQuick
      Item { property string from: "unix" }
      """
    And the file "+Material/Tag.qml":
      """qml
      import QtQuick
      Item { property string from: "material" }
      """
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item { Tag { objectName: "tag" } }
      """
    Then the property "from" of "tag" is "material"

  Scenario: A module's type is chosen, and what it uses of its module is there
    Given the application starts with the controls style "Material"
    And the file "kit/qmldir":
      """
      module kit
      Panel Panel.qml
      Part Part.qml
      """
    And the file "kit/Part.qml":
      """qml
      import QtQuick
      Item { property string name: "part" }
      """
    And the file "kit/Panel.qml":
      """qml
      import QtQuick
      Item { property string from: "plain" }
      """
    And the file "kit/+Material/Panel.qml":
      """qml
      import QtQuick
      import kit
      Item {
        property string from: "material " + part.name
        Part { id: part }
      }
      """
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      import kit
      Item { Panel { objectName: "panel" } }
      """
    Then the property "from" of "panel" is "material part"
