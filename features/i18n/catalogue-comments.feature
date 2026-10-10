# Sources:
#   https://doc.qt.io/qt-6/i18n-source-translation.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#qsTr-method
#   https://doc.qt.io/qt-6/linguist-ts-file-format.html
#   https://doc.qt.io/qt-6/qtranslator.html#translate

# Qt's compiled catalogue keeps of each entry only as much as tells it from the others, so a text
# that has a single entry is found whatever comment it is asked with: asked without a comment it
# gets the translation of the entry that has one. Ljós reads the Linguist file itself, where the
# comment is still there, and shows the source text: the translator sees an untranslated string,
# not a meaning nobody chose.
@qt-diverges
Feature: A translation written for one meaning is not used for another
  The comment of qsTr tells two meanings of one text apart. A translation the translator wrote
  for a meaning serves that meaning only.

  Background:
    Given the translation file "app_is.ts":
      """xml
      <?xml version="1.0" encoding="utf-8"?>
      <!DOCTYPE TS>
      <TS version="2.1" language="is_IS">
      <context><name>Scenario</name>
        <message><source>Close</source><comment>a door</comment>
          <translation>Loka</translation></message>
      </context>
      </TS>
      """

  Scenario: A text asked for without a comment does not take a translation that has one
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string shown: qsTr("Close")
        property string door: qsTr("Close", "a door")
      }
      """
    When the UI language is set to "is_IS"
    Then the property "shown" of "root" is "Close"
    And the property "door" of "root" is "Loka"

  Scenario: A text asked for with another comment does not take it either
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string shown: qsTr("Close", "nearby")
      }
      """
    When the UI language is set to "is_IS"
    Then the property "shown" of "root" is "Close"
