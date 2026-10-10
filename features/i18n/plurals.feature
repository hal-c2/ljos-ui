# Sources:
#   https://doc.qt.io/qt-6/i18n-source-translation.html
#   https://doc.qt.io/qt-6/i18n-plural-rules.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#qsTr-method
#   https://doc.qt.io/qt-6/linguist-ts-file-format.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Plural forms
  The third argument of qsTr is a count. A catalogue holds one form of the text for each plural
  form its language has, and the count picks the form. Languages count differently: Icelandic says
  "21 skrá" as it says "1 skrá", Polish has a form for 2 to 4 that English never needs.

  Rule: Icelandic has two forms, and the first is for counts that end in 1 but not in 11

    Background:
      Given the translation file "app_is.ts":
        """xml
        <?xml version="1.0" encoding="utf-8"?>
        <!DOCTYPE TS>
        <TS version="2.1" language="is_IS">
        <context><name>Scenario</name>
          <message numerus="yes"><source>%n file(s)</source><translation>
            <numerusform>%n skrá</numerusform>
            <numerusform>%n skrár</numerusform></translation></message>
          <message numerus="yes"><source>%Ln byte(s)</source><translation>
            <numerusform>%Ln bæti</numerusform>
            <numerusform>%Ln bæti alls</numerusform></translation></message>
          <message><source>Page %n</source><translation>Síða %n</translation></message>
        </context>
        <context><name></name>
          <message id="unread" numerus="yes"><source>%n unread</source><translation>
            <numerusform>%n ólesið</numerusform>
            <numerusform>%n ólesin</numerusform></translation></message>
        </context>
        </TS>
        """

    Scenario Outline: The count picks the form
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string shown: qsTr("%n file(s)", "", <count>)
        }
        """
      When the UI language is set to "is_IS"
      Then the property "shown" of "root" is "<shown>"

      Examples:
        | count | shown      |
        | 1     | 1 skrá     |
        | 0     | 0 skrár    |
        | 2     | 2 skrár    |
        | 5     | 5 skrár    |
        | 11    | 11 skrár   |
        | 21    | 21 skrá    |
        | 101   | 101 skrá   |
        | 111   | 111 skrár  |
        | 1001  | 1001 skrá  |

    Scenario: The text follows the count
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int count: 1
          property string shown: qsTr("%n file(s)", "", count)
        }
        """
      When the UI language is set to "is_IS"
      And the property "count" of "root" is set to 4
      Then the property "shown" of "root" is "4 skrár"

    Scenario: A text looked up by id has plural forms too
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string one: qsTrId("unread", 31)
          property string many: qsTrId("unread", 12)
        }
        """
      When the UI language is set to "is_IS"
      Then the property "one" of "root" is "31 ólesið"
      And the property "many" of "root" is "12 ólesin"

    Scenario: "%Ln" writes the count with the locale's digit grouping
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string shown: qsTr("%Ln byte(s)", "", 20481)
        }
        """
      When the UI language is set to "is_IS"
      Then the property "shown" of "root" is "20,481 bæti"

    # The translator was given one form to fill in, so that is the only form there is: a count
    # that needs the second one has nothing to show but the text as written.
    Scenario Outline: A translation that was not written for a count serves only its first form
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string shown: qsTr("Page %n", "", <count>)
        }
        """
      When the UI language is set to "is_IS"
      Then the property "shown" of "root" is "<shown>"

      Examples:
        | count | shown   |
        | 1     | Síða 1  |
        | 21    | Síða 21 |
        | 3     | Page 3  |
        | 11    | Page 11 |

  Rule: Polish has three forms

    Background:
      Given the translation file "app_pl.ts":
        """xml
        <?xml version="1.0" encoding="utf-8"?>
        <!DOCTYPE TS>
        <TS version="2.1" language="pl_PL">
        <context><name>Scenario</name>
          <message numerus="yes"><source>%n file(s)</source><translation>
            <numerusform>%n plik</numerusform>
            <numerusform>%n pliki</numerusform>
            <numerusform>%n plików</numerusform></translation></message>
        </context>
        </TS>
        """

    Scenario Outline: The count picks one of three
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string shown: qsTr("%n file(s)", "", <count>)
        }
        """
      When the UI language is set to "pl_PL"
      Then the property "shown" of "root" is "<shown>"

      Examples:
        | count | shown      |
        | 1     | 1 plik     |
        | 2     | 2 pliki    |
        | 4     | 4 pliki    |
        | 5     | 5 plików   |
        | 0     | 0 plików   |
        | 12    | 12 plików  |
        | 22    | 22 pliki   |
        | 25    | 25 plików  |
        | 112   | 112 plików |

  Rule: Arabic has six forms

    Background:
      Given the translation file "app_ar.ts":
        """xml
        <?xml version="1.0" encoding="utf-8"?>
        <!DOCTYPE TS>
        <TS version="2.1" language="ar_EG">
        <context><name>Scenario</name>
          <message numerus="yes"><source>%n file(s)</source><translation>
            <numerusform>zero %n</numerusform>
            <numerusform>one %n</numerusform>
            <numerusform>two %n</numerusform>
            <numerusform>few %n</numerusform>
            <numerusform>many %n</numerusform>
            <numerusform>other %n</numerusform></translation></message>
        </context>
        </TS>
        """

    Scenario Outline: The count picks one of six
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string shown: qsTr("%n file(s)", "", <count>)
        }
        """
      When the UI language is set to "ar_EG"
      Then the property "shown" of "root" is "<shown>"

      Examples:
        | count | shown     |
        | 0     | zero 0    |
        | 1     | one 1     |
        | 2     | two 2     |
        | 3     | few 3     |
        | 10    | few 10    |
        | 11    | many 11   |
        | 99    | many 99   |
        | 100   | other 100 |
        | 102   | other 102 |
        | 103   | few 103   |
