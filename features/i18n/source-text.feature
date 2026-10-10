# Sources:
#   https://doc.qt.io/qt-6/qtquick-internationalization.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#qsTr-method
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#qsTranslate-method
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#qsTrId-method
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#qsTrNoOp-method
#   https://doc.qt.io/qt-6/i18n-source-translation.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Text marked for translation, when there is no translation
  An application runs before anyone has translated it. Every function that marks a text for
  translation then gives the text as it is written in the source.

  Scenario Outline: The source text is shown
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string label: "Save"
        property string shown: <expression>
      }
      """
    Then the property "shown" of "root" is "<shown>"

    Examples:
      | expression                          | shown     |
      | qsTr("Hello")                       | Hello     |
      | qsTr("Open", "verb")                | Open      |
      | qsTr(label)                         | Save      |
      | qsTr("")                            |           |
      | qsTranslate("Dialog", "Cancel")     | Cancel    |
      | qsTranslate("Dialog", "Open", "verb") | Open    |
      | qsTrId("app-title")                 | app-title |
      | "Total: " + qsTr("Sum")             | Total: Sum |

  Scenario Outline: A marker function gives back its text and translates nothing
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string shown: <expression>
      }
      """
    Then the property "shown" of "root" is "<shown>"

    Examples:
      | expression                          | shown  |
      | QT_TR_NOOP("Later")                 | Later  |
      | QT_TRANSLATE_NOOP("Dialog", "Later") | Later |
      | QT_TRID_NOOP("later-id")            | later-id |
      | qsTr(QT_TR_NOOP("Later"))           | Later  |

  Rule: "%n" stands for the count

    Scenario Outline: The count replaces "%n" in the source text
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int count: 3
          property string shown: <expression>
        }
        """
      Then the property "shown" of "root" is "<shown>"

      Examples:
        | expression                           | shown            |
        | qsTr("%n file(s)", "", 3)            | 3 file(s)        |
        | qsTr("%n file(s)", "", count)        | 3 file(s)        |
        | qsTr("%n of %n", "", 2)              | 2 of 2           |
        | qsTr("%n file(s)", "", 0)            | 0 file(s)        |
        | qsTr("%n file(s)", "", 2.7)          | 2 file(s)        |
        | qsTranslate("Dialog", "%n file(s)", "", 4) | 4 file(s)  |
        | qsTrId("%n things", 3)               | 3 things         |
        | qsTr("%Ln file(s)", "", 12345)       | 12,345 file(s)   |
        | qsTr("%n file(s)", "", 12345)        | 12345 file(s)    |

    Scenario Outline: Without a count "%n" stays in the text
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string shown: <expression>
        }
        """
      Then the property "shown" of "root" is "<shown>"

      Examples:
        | expression                   | shown      |
        | qsTr("%n file(s)")           | %n file(s) |
        | qsTr("%n file(s)", "")       | %n file(s) |
        | qsTr("%n file(s)", "", -1)   | %n file(s) |

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
      When the property "count" of "root" is set to 7
      Then the property "shown" of "root" is "7 file(s)"

  Rule: A call that cannot be a translation is an error

    Scenario Outline: The wrong kind of argument is reported
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string shown: "kept"
          Component.onCompleted: shown = <expression>
        }
        """
      Then the property "shown" of "root" is "kept"
      And the log has a warning matching "<warning>"

      Examples:
        | expression            | warning                                                   |
        | qsTr()                | qsTr.. requires at least one argument                     |
        | qsTr(5)               | qsTr..: first argument .sourceText. must be a string      |
        | qsTr("Open", 5)       | qsTr..: second argument .disambiguation. must be a string |
        | qsTr("Open", "", "x") | qsTr..: third argument .n. must be a number               |
        | qsTranslate("Dialog") | qsTranslate.. requires at least two arguments             |
        | qsTrId()              | qsTrId.. requires at least one argument                   |
        | qsTrId(5)             | qsTrId..: first argument .id. must be a string            |
