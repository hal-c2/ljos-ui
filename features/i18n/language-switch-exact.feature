# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#uiLanguage-prop
#   https://doc.qt.io/qt-6/qtquick-internationalization.html
#   https://doc.qt.io/qt-6/qqmlengine.html#retranslate
#   https://doc.qt.io/qt-6/qtranslator.html#load-1

# Two things Qt 6.11.2 (qml6) does differently when Qt.uiLanguage is assigned, each kept out of
# Ljós on purpose; the comment above each scenario says what Qt does.
@qt-diverges
Feature: A language change touches exactly what the language decides
  The UI language is what the application shows, and a change of it works out again the bindings
  whose last value came from a translation.

  Background:
    Given the translation file "app_is.ts":
      """xml
      <?xml version="1.0" encoding="utf-8"?>
      <!DOCTYPE TS>
      <TS version="2.1" language="is_IS">
      <context><name>Scenario</name>
        <message><source>Open</source><translation>Opna</translation></message>
      </context>
      </TS>
      """

  # Qt works out again every binding that has qsTr written in it, also one whose last run went
  # another way and asked for no translation (three runs here). Ljós follows what a binding read,
  # as it does for every other dependency, so a binding the language cannot change stays asleep.
  Scenario: A binding that stopped translating is left alone
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property var runs: ({ title: 0 })
        property bool translate: true
        property string title: { runs.title++; return translate ? qsTr("Open") : "Open" }
        property int titleRuns
        function report() { titleRuns = runs.title }
      }
      """
    When the property "translate" of "root" is set to false
    And the UI language is set to "is_IS"
    And the method "report" of "root" is called
    Then the property "title" of "root" is "Open"
    And the property "titleRuns" of "root" is 2

  Scenario: It translates again when it is asked to
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property bool translate: true
        property string title: translate ? qsTr("Open") : "Open"
      }
      """
    When the property "translate" of "root" is set to false
    And the UI language is set to "is_IS"
    And the property "translate" of "root" is set to true
    Then the property "title" of "root" is "Opna"

  # Qt looks for a catalogue of the new language and, finding none, keeps the one it had: after
  # "is_IS" the texts stay Icelandic whatever is assigned next (only "" takes them back). Ljós
  # shows each language as itself, and without a catalogue that is the text as written.
  Scenario Outline: Changing to a language without a catalogue shows the text as written
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string shown: qsTr("Open")
      }
      """
    When the UI language is set to "is_IS"
    And the UI language is set to "<language>"
    Then the property "shown" of "root" is "Open"

    Examples:
      | language |
      | en       |
      | fr_FR    |
      | nonsense |
