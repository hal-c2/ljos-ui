# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-locale.html#toString-method
#   https://doc.qt.io/qt-6/qlocale.html#toString
# Verified on Qt 6.11.2 (qml6, offscreen), with LANG=en_US.UTF-8 and TZ=UTC; the ledger runs Ljós
# in UTC too. Each result is written between brackets, so that an empty one shows. French groups
# digits and English sets "PM" off with U+202F, a narrow no-break space, as CLDR does.

@qt-parity @api-QtQml.Locale.toString
Feature: A locale writes a number or a date as a text
  `Locale.toString` writes a whole number with the locale's digits and group separators, any other
  number in a floating-point format ('g' and six significant digits unless told otherwise), and a
  date in one of the locale's formats (the long one unless told otherwise) or a format string.

  Scenario Outline: <value> written by <locale>
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        readonly property var day: new Date(2026, 0, 2, 14, 5, 7)
        readonly property var locale: Qt.locale("<locale>")
        property string out: {
          try { return "[" + locale.toString(<value>) + "]" }
          catch (e) { return e.name + ": " + e.message }
        }
      }
      """
    Then the property "out" of "root" is "<text>"

    Examples: Whole numbers
      | locale | value       | text            |
      | de_DE  | 1234567     | [1.234.567]     |
      | en_US  | 1234567     | [1,234,567]     |
      | C      | 1234567     | [1234567]       |
      | fr_FR  | 1234567     | [1 234 567]     |
      | de_DE  | -42         | [-42]           |
      | de_DE  | -0          | [0]             |
      | de_DE  | 1234.0      | [1.234]         |
      | de_DE  | 2147483647  | [2.147.483.647] |
      | de_DE  | -2147483648 | [-2.147.483.648] |
      | de_DE  | true        | [1]             |
      | ar_EG  | 1234.5      | [١٬٢٣٤٫٥]       |

    Examples: Other numbers
      | locale | value                   | text           |
      | de_DE  | 1234.5678               | [1.234,57]     |
      | en_US  | 1234.5678               | [1,234.57]     |
      | de_DE  | 0.5                     | [0,5]          |
      | de_DE  | 1e21                    | [1e+21]        |
      | de_DE  | 2147483648              | [2,14748e+09]  |
      | de_DE  | 0.000012345             | [1,2345e-05]   |
      | de_DE  | NaN                     | [nan]          |
      | de_DE  | Infinity                | [inf]          |
      | de_DE  | 1234.5678, 'f'          | [1.234,567800] |
      | de_DE  | 1234.5678, 'f', 2       | [1.234,57]     |
      | de_DE  | 1234.5, 'f', 1.7        | [1.234,5]      |
      | de_DE  | 1234.5, 'f', undefined  | [1.234,500000] |
      | de_DE  | 1234, 'f', 0            | [1.234]        |
      | de_DE  | 3, 'f'                  | [3,000000]     |
      | de_DE  | 1234, 'g'               | [1.234]        |
      | de_DE  | 1234, undefined         | [1.234]        |
      | de_DE  | 1234.5678, 'e', 3       | [1,235e+03]    |
      | de_DE  | 1234.5678, 'E'          | [1,234568E+03] |
      | de_DE  | 1234.5678, 'g', 3       | [1,23e+03]     |
      | de_DE  | 1234.5678, 'G', 2       | [1,2E+03]      |
      | de_DE  | 1234.5678, 'x'          | [1.234,567800] |
      | de_DE  | 1234.5678, 'f2'         | [1.234,567800] |
      | de_DE  | 1234.5678, ''           | []             |

    Examples: Dates
      | locale | value                              | text                                                            |
      | de_DE  | day                                | [Freitag, 2. Januar 2026 14:05:07 Koordinierte Weltzeit]        |
      | de_DE  | day, Locale.LongFormat             | [Freitag, 2. Januar 2026 14:05:07 Koordinierte Weltzeit]        |
      | de_DE  | day, Locale.ShortFormat            | [02.01.26 14:05]                                                |
      | de_DE  | day, Locale.NarrowFormat           | [02.01.26 14:05]                                                |
      | de_DE  | day, 7                             | [02.01.26 14:05]                                                |
      | en_US  | day, Locale.ShortFormat            | [1/2/26 2:05 PM]                                                |
      | de_DE  | day, "dddd d. MMMM yyyy HH:mm"     | [Freitag 2. Januar 2026 14:05]                                  |
      | en_US  | day, "ddd h:mm AP"                 | [Fri 2:05 PM]                                                   |
      | de_DE  | day, "t"                           | [UTC]                                                           |
      | de_DE  | day, ""                            | []                                                              |
      | de_DE  | new Date(NaN)                      | []                                                              |
      | de_DE  | day, undefined                     | [Freitag, 2. Januar 2026 14:05:07 Koordinierte Weltzeit]        |

    Examples: What it does not take
      | locale | value         | text                                                                                                                         |
      | de_DE  | 1, 2, 3, 4    | Error: Locale: toString(): Expected 1-3 arguments, but received 4                                                          |
      | de_DE  | null          | Error: Locale: toString() expects either an int, double, or Date as its first argument                                     |
      | de_DE  | {}            | Error: Locale: toString() expects either an int, double, or Date as its first argument                                     |
      | de_DE  | [1]           | Error: Locale: toString() expects either an int, double, or Date as its first argument                                     |
      | de_DE  | day, null     | Error: Locale: the second argument to the toString overloads whose first argument is a Date should be a string or FormatType |
      | de_DE  | day, "yyyy", 3 | Error: Locale: the toString() overload that takes a Date as its first argument expects 1 or 2 arguments, but received 3    |

  Scenario: The application's locale writes a number as its language does
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string out: Qt.locale().toString(1234.5)
      }
      """
    Then the property "out" of "root" is "1,234.5"
