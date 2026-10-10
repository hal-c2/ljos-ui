# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#application-prop
#   https://doc.qt.io/qt-6/qml-qtquick-application.html#layoutDirection-prop
#   https://doc.qt.io/qt-6/qtquick-positioning-righttoleft.html
#   https://doc.qt.io/qt-6/qml-qtquick-layoutmirroring.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#uiLanguage-prop

# Qt works the application's layout direction out when the application starts (from the system
# language, or from a catalogue loaded then) and assigning Qt.uiLanguage later leaves it as it was.
# In Ljós it is the direction of the UI language at every moment, so an application that offers a
# language menu mirrors when the user picks Arabic.
@qt-diverges
Feature: The layout direction follows the UI language
  Qt.application.layoutDirection is Qt.RightToLeft while the UI language is one that is written
  from right to left. Nothing is mirrored by itself: an application opts in by binding
  LayoutMirroring.enabled, or a layoutDirection, to it.

  Scenario Outline: The direction of a language
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property bool rightToLeft: Qt.application.layoutDirection === Qt.RightToLeft
      }
      """
    When the UI language is set to "<language>"
    Then the property "rightToLeft" of "root" is <rightToLeft>

    Examples:
      | language | rightToLeft |
      | ar       | true        |
      | ar_EG    | true        |
      | he       | true        |
      | fa_IR    | true        |
      | ur       | true        |
      | is_IS    | false       |
      | de       | false       |
      | ja       | false       |

  Scenario: The direction goes back when the language does
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int direction: Qt.application.layoutDirection
      }
      """
    When the UI language is set to "he"
    And the UI language is set to "en"
    Then the property "direction" of "root" is 0

  Scenario: Items are not mirrored unless the application asks
    Given the QML:
      """qml
      import QtQuick
      Row {
        objectName: "root"
        width: 100
        Item { objectName: "first"; width: 20; height: 20 }
        Item { objectName: "second"; width: 30; height: 20 }
      }
      """
    When the UI language is set to "ar"
    Then the property "x" of "first" is 0
    And the property "x" of "second" is 20

  Scenario: An application mirrors itself by binding LayoutMirroring to the direction
    Given the QML:
      """qml
      import QtQuick
      Row {
        objectName: "root"
        width: 100
        LayoutMirroring.enabled: Qt.application.layoutDirection === Qt.RightToLeft
        Item { objectName: "first"; width: 20; height: 20 }
        Item { objectName: "second"; width: 30; height: 20 }
      }
      """
    When the UI language is set to "ar"
    Then the property "x" of "first" is 80
    And the property "x" of "second" is 50
