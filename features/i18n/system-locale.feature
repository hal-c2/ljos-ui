# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#locale-method
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#uiLanguage-prop
#   https://doc.qt.io/qt-6/qml-qtqml-locale.html#formattedDataSize-method
#   https://doc.qt.io/qt-6/qlocale.html#system
#   https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap08.html#tag_08_02
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: An application starts in the system's locale
  A program takes its locale from its environment, category by category, as the system names
  them: LC_ALL first, then the category's own variable (LC_MESSAGES, LC_NUMERIC, LC_TIME), then
  LANG. Qt.locale() is named by the messages category, and so is Qt.uiLanguage; numbers are
  written by LC_NUMERIC and dates and times by LC_TIME.

  Scenario Outline: The locale and the UI language come from the environment
    Given the application starts with the locale environment "<environment>"
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string name: Qt.locale().name
        property string language: Qt.uiLanguage
      }
      """
    Then the property "name" of "root" is "<name>"
    And the property "language" of "root" is "<language>"

    Examples:
      | environment                                    | name  | language |
      | LANG=de_DE.UTF-8                               | de_DE | de       |
      | LANG=pt_BR.UTF-8                               | pt_BR | pt       |
      | LANG=pt_PT.UTF-8                               | pt_PT | pt-PT    |
      | LANG=en_GB.UTF-8                               | en_GB | en-GB    |
      | LANG=zh_TW.UTF-8                               | zh_TW | zh-TW    |
      | LANG=en_US.UTF-8 LC_MESSAGES=de_DE.UTF-8       | de_DE | de       |
      | LANG=en_US.UTF-8 LC_NUMERIC=de_DE.UTF-8        | en_US | en       |
      | LANG=en_US.UTF-8 LC_ALL=fr_FR.UTF-8            | fr_FR | fr       |
      | LANG=en_US.UTF-8 LANGUAGE=de                   | de_DE | de       |

  Scenario Outline: Numbers, dates and times are written by their own categories
    Given the application starts with the locale environment "<environment>"
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string number: (1234.5).toLocaleString(Qt.locale(), "f", 2)
        property string date: Qt.locale().dateFormat(Locale.ShortFormat)
        property string time: Qt.locale().timeFormat(Locale.ShortFormat)
      }
      """
    Then the property "number" of "root" is "<number>"
    And the property "date" of "root" is "<date>"
    And the property "time" of "root" is "<time>"

    Examples:
      | environment                                                       | number   | date       | time  |
      | LANG=de_DE.UTF-8                                                  | 1.234,50 | dd.MM.yy   | HH:mm |
      | LANG=en_US.UTF-8 LC_TIME=de_DE.UTF-8                              | 1,234.50 | dd.MM.yy   | HH:mm |
      | LANG=is LC_NUMERIC=de_DE.UTF-8 LC_TIME=sv_SE.UTF-8                | 1.234,50 | yyyy-MM-dd | HH:mm |
      | LANG=en_US.UTF-8 LC_ALL=fr_FR.UTF-8 LC_NUMERIC=sv_SE.UTF-8        | 1 234,50 | dd/MM/yyyy | HH:mm |

  Scenario: An application that sets its locale writes by it
    Given the application starts with the locale environment "LANG=de_DE.UTF-8"
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string number: (1234.5).toLocaleString(Qt.locale("en_US"), "f", 2)
        property string own: (1234.5).toLocaleString(Qt.locale(), "f", 2)
      }
      """
    Then the property "number" of "root" is "1,234.50"
    And the property "own" of "root" is "1.234,50"

  @api-QtQml.Locale.formattedDataSize
  Scenario Outline: A count of bytes is written in the largest unit there is one of
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string size: Qt.locale("<locale>").formattedDataSize(<bytes>, <precision>, <format>)
      }
      """
    Then the property "size" of "root" is "<size>"

    Examples:
      | locale | bytes   | precision | format                           | size        |
      | en_US  | 0       | 2         | Locale.DataSizeIecFormat         | 0 bytes     |
      | en_US  | 1023    | 2         | Locale.DataSizeIecFormat         | 1,023 bytes |
      | en_US  | 1536    | 2         | Locale.DataSizeIecFormat         | 1.50 KiB    |
      | en_US  | 1234567 | 2         | Locale.DataSizeIecFormat         | 1.18 MiB    |
      | en_US  | 1000    | 1         | Locale.DataSizeSIFormat          | 1.0 kB      |
      | en_US  | 1000    | 0         | Locale.DataSizeTraditionalFormat | 1,000 bytes |
      | en_US  | 1536    | 0         | Locale.DataSizeTraditionalFormat | 2 kB        |
      | en_US  | -2048   | 2         | Locale.DataSizeIecFormat         | -2.00 KiB   |
      | en_US  | 1536    | -1        | Locale.DataSizeIecFormat         | 1.500000 KiB |
      | C      | 1234567 | 1         | Locale.DataSizeSIFormat          | 1.2 MB      |

  # Qt names the units in the locale's language (Bytes, octets, Kio); Ljós has
  # no locale's names for units yet and writes them in English.
  @backlog @api-QtQml.Locale.formattedDataSize
  Scenario Outline: The units of a count of bytes are named in the locale's language
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string size: Qt.locale("<locale>").formattedDataSize(<bytes>)
      }
      """
    Then the property "size" of "root" is "<size>"

    Examples:
      | locale | bytes | size      |
      | de_DE  | 1000  | 1.000 Bytes |
      | fr_FR  | 1536  | 1,50 Kio  |
