# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#locale-method
#   https://doc.qt.io/qt-6/qml-qtqml-locale.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#uiLanguage-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Qt.locale describes how a language writes numbers and text
  Qt.locale(name) gives the conventions of the locale with that name; Qt.locale() those of the
  application's own locale. The application's locale is not its UI language: translated texts
  follow Qt.uiLanguage, number formats follow the locale.

  Scenario Outline: A locale is found by a more or less complete name
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string name: Qt.locale("<asked>").name
      }
      """
    Then the property "name" of "root" is "<name>"

    Examples:
      | asked    | name  |
      | is_IS    | is_IS |
      | is       | is_IS |
      | is-IS    | is_IS |
      | de       | de_DE |
      | de_AT    | de_AT |
      | en_GB    | en_GB |
      | pt       | pt_BR |
      | zh       | zh_CN |
      | ar       | ar_EG |
      | he       | he_IL |
      | nonsense | C     |
      | C        | C     |

  Scenario Outline: The signs a locale writes numbers with
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        readonly property var locale: Qt.locale("<name>")
        property string point: locale.decimalPoint
        property string group: locale.groupSeparator
        property string zero: locale.zeroDigit
        property string plus: locale.positiveSign
        property string minus: locale.negativeSign
      }
      """
    Then the property "point" of "root" is "<point>"
    And the property "group" of "root" is "<group>"
    And the property "zero" of "root" is "<zero>"
    And the property "plus" of "root" is "+"
    And the property "minus" of "root" is "-"

    Examples:
      | name  | point | group | zero |
      | en_US | .     | ,     | 0    |
      | is_IS | ,     | .     | 0    |
      | de_DE | ,     | .     | 0    |
      | de_CH | .     | '     | 0    |
      | C     | .     | ,     | 0    |

  Scenario: Arabic has its own digits and signs
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        readonly property var locale: Qt.locale("ar_EG")
        property string point: locale.decimalPoint
        property string group: locale.groupSeparator
        property string zero: locale.zeroDigit
      }
      """
    Then the property "point" of "root" is "٫"
    And the property "group" of "root" is "٬"
    And the property "zero" of "root" is "٠"

  Scenario Outline: The direction a locale's text runs in
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property bool rightToLeft: Qt.locale("<name>").textDirection === Qt.RightToLeft
      }
      """
    Then the property "rightToLeft" of "root" is <rightToLeft>

    Examples:
      | name  | rightToLeft |
      | en_US | false       |
      | is_IS | false       |
      | ar_EG | true        |
      | he_IL | true        |
      | fa_IR | true        |
      | ur_PK | true        |
      | C     | false       |

  Scenario Outline: The languages to look for translations in, most exact first
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string languages: Qt.locale("<name>").uiLanguages.join(" ")
      }
      """
    Then the property "languages" of "root" is "<languages>"

    Examples:
      | name  | languages                    |
      | is_IS | is-Latn-IS is-IS is-Latn is  |
      | de_AT | de-Latn-AT de-AT de-Latn de  |
      | ar_EG | ar-Arab-EG ar-EG ar-Arab ar  |

  Rule: The UI language and the locale are two things

    Scenario: Qt.uiLanguage holds what was assigned
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string language: Qt.uiLanguage
        }
        """
      When the UI language is set to "is_IS"
      Then the property "language" of "root" is "is_IS"

    Scenario: The locale does not follow the UI language
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string name: Qt.locale().name
          property string amount: "%L1".arg(1234.5)
          property string files: qsTr("%Ln file(s)", "", 12345)
        }
        """
      When the UI language is set to "is_IS"
      Then the property "name" of "root" is "en_US"
      And the property "amount" of "root" is "1,234.5"
      And the property "files" of "root" is "12,345 file(s)"
