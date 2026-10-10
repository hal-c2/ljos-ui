# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#uiLanguage-prop
#   https://doc.qt.io/qt-6/qtquick-internationalization.html
#   https://doc.qt.io/qt-6/qqmlengine.html#retranslate
#   https://doc.qt.io/qt-6/qtranslator.html#load-1
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Changing the language while the application runs
  Assigning Qt.uiLanguage changes the language of every translated text on screen. Nothing is
  reloaded: the bindings that asked for a translation are worked out again, and only those.

  Background:
    Given the translation file "app_is.ts":
      """xml
      <?xml version="1.0" encoding="utf-8"?>
      <!DOCTYPE TS>
      <TS version="2.1" language="is_IS">
      <context><name>Scenario</name>
        <message><source>Open</source><translation>Opna</translation></message>
        <message><source>Close</source><translation>Loka</translation></message>
        <message numerus="yes"><source>%n file(s)</source><translation>
          <numerusform>%n skrá</numerusform>
          <numerusform>%n skrár</numerusform></translation></message>
      </context>
      </TS>
      """
    And the translation file "app_de.ts":
      """xml
      <?xml version="1.0" encoding="utf-8"?>
      <!DOCTYPE TS>
      <TS version="2.1" language="de">
      <context><name>Scenario</name>
        <message><source>Open</source><translation>Öffnen</translation></message>
      </context>
      </TS>
      """

  Scenario: A catalogue changes nothing until its language is the UI language
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string shown: qsTr("Open")
      }
      """
    Then the property "shown" of "root" is "Open"

  Scenario: Translated bindings follow the UI language
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int count: 3
        property string open: qsTr("Open")
        property string both: qsTr("Open") + " / " + qsTr("Close")
        property string files: qsTr("%n file(s)", "", count)
      }
      """
    When the UI language is set to "is_IS"
    Then the property "open" of "root" is "Opna"
    And the property "both" of "root" is "Opna / Loka"
    And the property "files" of "root" is "3 skrár"

  Scenario: The language can change more than once
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string shown: qsTr("Open")
      }
      """
    When the UI language is set to "is_IS"
    And the UI language is set to "de"
    Then the property "shown" of "root" is "Öffnen"

  Scenario: QML changes the language by assigning Qt.uiLanguage
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string shown: qsTr("Open")
        property string language: Qt.uiLanguage
        function icelandic() { Qt.uiLanguage = "is_IS" }
      }
      """
    When the method "icelandic" of "root" is called
    Then the property "shown" of "root" is "Opna"
    And the property "language" of "root" is "is_IS"

  Scenario: Objects made after the change are in the new language
    Given the file "Late.qml":
      """qml
      import QtQuick
      Item {
        objectName: "late"
        property string shown: qsTranslate("Scenario", "Close")
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item { objectName: "root" }
      """
    When the UI language is set to "is_IS"
    And "Late.qml" is loaded
    Then the property "shown" of "late" is "Loka"

  Scenario: Only the bindings that translate are worked out again
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property var runs: ({ title: 0, size: 0, shout: 0 })
        property int side: 10
        property string title: { runs.title++; return qsTr("Open") }
        property int size: { runs.size++; return side * 2 }
        property string shout: { runs.shout++; return title.toUpperCase() }
        property int titleRuns
        property int sizeRuns
        property int shoutRuns
        function report() {
          titleRuns = runs.title; sizeRuns = runs.size; shoutRuns = runs.shout
        }
      }
      """
    When the UI language is set to "is_IS"
    And the method "report" of "root" is called
    Then the property "shout" of "root" is "OPNA"
    And the property "titleRuns" of "root" is 2
    And the property "shoutRuns" of "root" is 2
    And the property "sizeRuns" of "root" is 1

  Scenario: Setting the language it already has works nothing out again
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property var runs: ({ title: 0 })
        property string title: { runs.title++; return qsTr("Open") }
        property int titleRuns
        function report() { titleRuns = runs.title }
      }
      """
    When the UI language is set to "is_IS"
    And the UI language is set to "is_IS"
    And the method "report" of "root" is called
    Then the property "titleRuns" of "root" is 2

  Rule: A language finds the catalogue of its nearest relative

    Scenario Outline: The language names a catalogue more or less exactly
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string shown: qsTr("Open")
        }
        """
      When the UI language is set to "<language>"
      Then the property "shown" of "root" is "<shown>"

      Examples:
        | language | shown  |
        | is_IS    | Opna   |
        | is-IS    | Opna   |
        | is       | Opna   |
        | de       | Öffnen |
        | de_DE    | Öffnen |
        | de_AT    | Öffnen |
        | de-CH    | Öffnen |

  Rule: What a language does not have is shown as written

    Scenario: The empty language is no language
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string shown: qsTr("Open")
        }
        """
      When the UI language is set to "is_IS"
      And the UI language is set to ""
      Then the property "shown" of "root" is "Open"

    Scenario: A text one language lacks is not borrowed from the language before
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string shown: qsTr("Close")
        }
        """
      When the UI language is set to "is_IS"
      And the UI language is set to "de"
      Then the property "shown" of "root" is "Close"
