# Sources:
#   https://doc.qt.io/qt-6/qtquick-internationalization.html
#   https://doc.qt.io/qt-6/i18n-source-translation.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#qsTr-method
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#qsTranslate-method
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#qsTrId-method
#   https://doc.qt.io/qt-6/qtqml-documents-structure.html
#   https://doc.qt.io/qt-6/linguist-ts-file-format.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Translations come from a catalogue
  A translator hands over a Qt Linguist file. A text is looked up by where it is written (its
  context, the name of the QML file), the source text, and the comment that tells two meanings of
  one text apart.

  Background:
    Given the translation file "app_is.ts":
      """xml
      <?xml version="1.0" encoding="utf-8"?>
      <!DOCTYPE TS>
      <TS version="2.1" language="is_IS">
      <context><name>Scenario</name>
        <message><source>Open</source><translation>Opna</translation></message>
        <message><source>Open</source><comment>not closed</comment>
          <translation>Opið</translation></message>
        <message><source>Draft</source>
          <translation type="unfinished">Uppkast</translation></message>
        <message><source>Empty</source><translation type="unfinished"></translation></message>
        <message><source>Old</source><translation type="obsolete">Gamalt</translation></message>
        <message><source>Gone</source><translation type="vanished">Farið</translation></message>
        <message><source>%1 of %2</source><translation>%2 alls, þar af %1</translation></message>
      </context>
      <context><name>Dialog</name>
        <message><source>Open</source><translation>Opna glugga</translation></message>
      </context>
      <context><name></name>
        <message id="app-title"><source>Reader</source><translation>Lesari</translation></message>
      </context>
      </TS>
      """

  Scenario Outline: A text is shown in the UI language
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string shown: <expression>
      }
      """
    When the UI language is set to "is_IS"
    Then the property "shown" of "root" is "<shown>"

    Examples:
      | expression                       | shown            |
      | qsTr("Open")                     | Opna             |
      | qsTr("Open", "not closed")       | Opið             |
      | qsTranslate("Dialog", "Open")    | Opna glugga      |
      | qsTrId("app-title")              | Lesari           |
      | "[" + qsTr("Open") + "]"         | [Opna]           |
      | qsTr("%1 of %2").arg(3).arg(10)  | 10 alls, þar af 3 |

  Scenario Outline: What the catalogue does not have is shown as written
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string shown: <expression>
      }
      """
    When the UI language is set to "is_IS"
    Then the property "shown" of "root" is "<shown>"

    Examples:
      | expression                       | shown   |
      | qsTr("Close")                    | Close   |
      | qsTranslate("Dialog", "Draft")   | Draft   |
      | qsTranslate("Nowhere", "Open")   | Open    |
      | qsTrId("no-such-id")             | no-such-id |
      | qsTr("open")                     | open    |

  Scenario: A comment the catalogue does not know falls back to the text without a comment
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string shown: qsTr("Open", "a door")
      }
      """
    When the UI language is set to "is_IS"
    Then the property "shown" of "root" is "Opna"

  Scenario Outline: Only translations a translator can still see are used
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string shown: qsTr("<source>")
      }
      """
    When the UI language is set to "is_IS"
    Then the property "shown" of "root" is "<shown>"

    Examples:
      | source | shown   |
      | Draft  | Uppkast |
      | Empty  | Empty   |
      | Old    | Old     |
      | Gone   | Gone    |

  Scenario: A marker function translates nothing, a later qsTr does
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string marked: QT_TR_NOOP("Open")
        property string shown: qsTr(marked)
      }
      """
    When the UI language is set to "is_IS"
    Then the property "marked" of "root" is "Open"
    And the property "shown" of "root" is "Opna"

  Scenario: A function is translated like a binding
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string shown
        function show() { shown = qsTr("Open") }
      }
      """
    When the UI language is set to "is_IS"
    And the method "show" of "root" is called
    Then the property "shown" of "root" is "Opna"

  Rule: The context is the name of the file the text is written in

    Scenario: The same text in two files has two translations
      Given the file "Dialog.qml":
        """qml
        import QtQuick
        Item {
          property string title: qsTr("Open")
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string here: qsTr("Open")
          property string there: dialog.title
          Dialog { id: dialog }
        }
        """
      When the UI language is set to "is_IS"
      Then the property "here" of "root" is "Opna"
      And the property "there" of "root" is "Opna glugga"

    Scenario: A delegate and an inline component belong to the file they are written in
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          component Label: Item { property string title: qsTr("Open") }
          Label { objectName: "inline" }
          Repeater {
            model: 1
            Item {
              objectName: "row"
              property string title: qsTr("Open", "not closed")
            }
          }
        }
        """
      When the UI language is set to "is_IS"
      Then the property "title" of "inline" is "Opna"
      And the property "title" of "row" is "Opið"

    Scenario: pragma Translator names the context instead
      Given the QML:
        """qml
        pragma Translator: Dialog
        import QtQuick
        Item {
          objectName: "root"
          property string shown: qsTr("Open")
          property string made
          function make() { made = qsTr("Open") }
        }
        """
      When the UI language is set to "is_IS"
      And the method "make" of "root" is called
      Then the property "shown" of "root" is "Opna glugga"
      And the property "made" of "root" is "Opna glugga"
