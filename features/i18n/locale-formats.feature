# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-locale.html
#   https://doc.qt.io/qt-6/qml-qtqml-date.html
#   https://doc.qt.io/qt-6/qml-qtqml-number.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#formatDateTime-method
#   https://doc.qt.io/qt-6/qlocale.html#toString-13
# Verified on Qt 6.11.2 (qml6, offscreen).
# The oracle runs with LANG=en_US.UTF-8 and TZ=UTC, so Qt.locale() is en_US and local time is UTC;
# the ledger runs Ljós in UTC too. Every date below is made in local time.
# A locale's own time formats put U+202F (a narrow no-break space) before "AM" and "PM", as
# CLDR does; a format string written in a scenario uses a plain space.

@qt-parity
Feature: Dates, times and numbers written and read the way a locale does
  A locale knows how its language writes a date and a time, long or short. Date.toLocaleDateString,
  toLocaleTimeString and toLocaleString write a date with a locale and either one of its formats
  or a format string; Date.fromLocaleDateString and its family read one back. Number.toLocaleString
  writes a number with the locale's digits and separators.

  @api-QtQml.Locale.FormatType @api-QtQml.Locale.DayOfWeek @api-QtQml.Locale.MeasurementSystem
  Scenario Outline: The Locale type names its enums
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int value: Locale.<key>
      }
      """
    Then the property "value" of "root" is <value>

    Examples:
      | key                   | value |
      | LongFormat            | 0     |
      | ShortFormat           | 1     |
      | NarrowFormat          | 2     |
      | Sunday                | 0     |
      | Monday                | 1     |
      | Saturday              | 6     |
      | MetricSystem          | 0     |
      | ImperialUSSystem      | 1     |
      | ImperialUKSystem      | 2     |
      | ImperialSystem        | 1     |
      | CurrencyIsoCode       | 0     |
      | CurrencySymbol        | 1     |
      | CurrencyDisplayName   | 2     |
      | DefaultNumberOptions  | 0     |
      | OmitGroupSeparator    | 1     |
      | RejectGroupSeparator  | 2     |

  @api-QtQml.Locale.dateFormat @api-QtQml.Locale.timeFormat @api-QtQml.Locale.dateTimeFormat
  Scenario Outline: A locale's formats are format strings
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        readonly property var locale: Qt.locale("<name>")
        property string date: locale.dateFormat(Locale.<format>)
        property string time: locale.timeFormat(Locale.<format>)
        property string both: locale.dateTimeFormat(Locale.<format>)
      }
      """
    Then the property "date" of "root" is "<date>"
    And the property "time" of "root" is "<time>"
    And the property "both" of "root" is "<date> <time>"

    Examples:
      | name  | format       | date               | time             |
      | en_US | LongFormat   | dddd, MMMM d, yyyy | h:mm:ss Ap tttt  |
      | en_US | ShortFormat  | M/d/yy             | h:mm Ap          |
      | en_US | NarrowFormat | M/d/yy             | h:mm Ap          |
      | de_DE | LongFormat   | dddd, d. MMMM yyyy | HH:mm:ss tttt    |
      | de_DE | ShortFormat  | dd.MM.yy           | HH:mm            |
      | en_GB | ShortFormat  | dd/MM/yyyy         | HH:mm            |
      | fr_FR | LongFormat   | dddd d MMMM yyyy   | HH:mm:ss tttt    |
      | is_IS | ShortFormat  | d.M.yyyy           | HH:mm            |
      | C     | LongFormat   | dddd, d MMMM yyyy  | HH:mm:ss t       |
      | C     | ShortFormat  | d MM yyyy          | HH:mm            |

  @api-QtQml.Locale.dateFormat
  Scenario: Without a format a locale gives its long one
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string date: Qt.locale().dateFormat()
        property string time: Qt.locale().timeFormat()
      }
      """
    Then the property "date" of "root" is "dddd, MMMM d, yyyy"
    And the property "time" of "root" is "h:mm:ss Ap tttt"

  @api-QtQml.Date.toLocaleDateString @api-QtQml.Date.toLocaleTimeString @api-QtQml.Date.toLocaleString
  Scenario Outline: A date written in a locale's format
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        readonly property var day: new Date(2024, 0, 5, 14, 7, 9)
        property string text: <expression>
      }
      """
    Then the property "text" of "root" is "<text>"

    Examples:
      | expression                                                       | text                                  |
      | day.toLocaleDateString(Qt.locale(), Locale.LongFormat)           | Friday, January 5, 2024               |
      | day.toLocaleDateString(Qt.locale(), Locale.ShortFormat)          | 1/5/24                                |
      | day.toLocaleDateString(Qt.locale("de_DE"), Locale.LongFormat)    | Freitag, 5. Januar 2024               |
      | day.toLocaleDateString(Qt.locale("de_DE"), Locale.ShortFormat)   | 05.01.24                              |
      | day.toLocaleDateString(Qt.locale("is_IS"), Locale.LongFormat)    | föstudagur, 5. janúar 2024            |
      | day.toLocaleDateString(Qt.locale("C"), Locale.ShortFormat)       | 5 01 2024                             |
      | day.toLocaleTimeString(Qt.locale(), Locale.ShortFormat)          | 2:07 PM                               |
      | day.toLocaleTimeString(Qt.locale(), Locale.LongFormat)           | 2:07:09 PM Coordinated Universal Time |
      | day.toLocaleTimeString(Qt.locale("de_DE"), Locale.ShortFormat)   | 14:07                                 |
      | day.toLocaleString(Qt.locale(), Locale.ShortFormat)              | 1/5/24 2:07 PM                        |
      | day.toLocaleString(Qt.locale("C"), Locale.ShortFormat)           | 5 01 2024 14:07                       |

  @api-QtQml.Date.toLocaleDateString @api-QtQml.Date.toLocaleTimeString @api-QtQml.Date.toLocaleString
  Scenario Outline: What a locale writes without a format, and with something that is not a locale
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        readonly property var day: new Date(2024, 0, 5, 14, 7, 9)
        property string text: <expression>
      }
      """
    Then the property "text" of "root" is "<text>"

    Examples:
      | expression                                          | text                    |
      | day.toLocaleDateString()                            | Friday, January 5, 2024 |
      | day.toLocaleDateString(Qt.locale("de_DE"))          | Freitag, 5. Januar 2024 |
      | day.toLocaleDateString(Locale.ShortFormat)          | 1/5/24                  |
      | day.toLocaleDateString(Locale.LongFormat)           | 1/5/24                  |
      | day.toLocaleDateString("dd MMMM yyyy")              | 1/5/24                  |
      | day.toLocaleDateString(null, Locale.LongFormat)     | 1/5/24                  |
      | day.toLocaleTimeString(Locale.ShortFormat)          | 2:07 PM                 |
      | day.toLocaleString(Locale.ShortFormat)              | 1/5/24 2:07 PM          |
      | new Date(NaN).toLocaleDateString(Qt.locale(), Locale.ShortFormat) |        |

  @api-QtQml.Date.toLocaleDateString @api-QtQml.Date.toLocaleTimeString
  Scenario Outline: A date written with a format string
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        readonly property var day: new Date(<date>)
        property string text: day.toLocaleString(Qt.locale("<name>"), "<format>")
      }
      """
    Then the property "text" of "root" is "<text>"

    Examples:
      | date                      | name  | format                     | text                     |
      | 2024, 0, 5, 14, 7, 9      | en_US | dd MMMM yyyy               | 05 January 2024          |
      | 2024, 0, 5, 14, 7, 9      | en_US | h:mm AP                    | 2:07 PM                  |
      | 2024, 0, 5, 14, 7, 9      | en_US | hh:mm:ss.zzz ap A a        | 02:07:09.000 pm PM pm    |
      | 2024, 2, 5, 0, 7, 9, 45   | en_US | h hh H HH h AP z zzz       | 12 12 0 00 12 AM 045 045 |
      | 2024, 0, 5, 14, 7, 9      | en_US | y yy yyy yyyy              | y 24 24y 2024            |
      | 2024, 0, 5, 14, 7, 9      | en_US | 'Day' d 'of' MMMM, ''yy''  | Day 5 of January, '24'   |
      | 2024, 0, 5, 14, 7, 9      | en_US | ddd MMM d                  | Fri Jan 5                |
      | 2024, 0, 5, 14, 7, 9      | de_DE | dddd ddd MMMM MMM          | Freitag Fr. Januar Jan.  |
      | 2024, 0, 5, 14, 7, 9      | en_US | t tt ttt                   | UTC +0000 +00:00         |

  @api-QtQml.Date.toLocaleDateString @api-QtQml.Date.toLocaleTimeString @api-QtQml.Qt.formatTime
  Scenario Outline: A date writes only the letters of a date, a time only those of a time
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        readonly property var day: new Date(2024, 0, 5, 14, 7, 9)
        property string text: <expression>
      }
      """
    Then the property "text" of "root" is "<text>"

    Examples:
      | expression                                                     | text                    |
      | day.toLocaleTimeString(Qt.locale(), "dd MMMM yyyy h:mm t")     | dd MMMM yyyy 14:07 UTC  |
      | day.toLocaleDateString(Qt.locale(), "dd MMM h:mm:ss ap zzz t") | 05 Jan h:mm:ss ap zzz t |
      | Qt.formatTime(day, "yyyy h")                                   | yyyy 14                 |
      | Qt.formatDate(day, "yyyy h")                                   | 2024 h                  |

  @api-QtQml.Locale.monthName @api-QtQml.Locale.standaloneMonthName @api-QtQml.Locale.dayName @api-QtQml.Locale.standaloneDayName
  Scenario Outline: A name on its own can differ from the name inside a date
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        readonly property var locale: Qt.locale("<name>")
        property string month: locale.monthName(0, Locale.<format>)
        property string aloneMonth: locale.standaloneMonthName(0, Locale.<format>)
        property string day: locale.dayName(5, Locale.<format>)
        property string aloneDay: locale.standaloneDayName(5, Locale.<format>)
      }
      """
    Then the property "month" of "root" is "<month>"
    And the property "aloneMonth" of "root" is "<aloneMonth>"
    And the property "day" of "root" is "<day>"
    And the property "aloneDay" of "root" is "<aloneDay>"

    Examples:
      | name  | format       | month  | aloneMonth | day     | aloneDay |
      | en_US | ShortFormat  | Jan    | Jan        | Fri     | Fri      |
      | de_DE | LongFormat   | Januar | Januar     | Freitag | Freitag  |
      | de_DE | ShortFormat  | Jan.   | Jan        | Fr.     | Fr       |
      | C     | NarrowFormat | 1      | J          | 5       | F        |

  @api-QtQml.Locale.standaloneMonthName @api-QtQml.Date.toLocaleDateString
  Scenario: A locale kept in a property is still a locale
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property var locale: Qt.locale("de_DE")
        property string month: root.locale.standaloneMonthName(0, Locale.ShortFormat)
        property string format: root.locale.dateFormat(Locale.ShortFormat)
        property string date: new Date(2024, 0, 5).toLocaleDateString(root.locale, Locale.ShortFormat)
        property string name: root.locale.name
      }
      """
    Then the property "month" of "root" is "Jan"
    And the property "format" of "root" is "dd.MM.yy"
    And the property "date" of "root" is "05.01.24"
    And the property "name" of "root" is "de_DE"

  @api-QtQml.Locale.amText @api-QtQml.Locale.pmText @api-QtQml.Locale.measurementSystem @api-QtQml.Locale.weekDays
  Scenario Outline: What else a locale knows about days and measures
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        readonly property var locale: Qt.locale("<name>")
        property string am: locale.amText + " " + locale.pmText
        property int measures: locale.measurementSystem
        property string weekDays: locale.weekDays.join(",")
      }
      """
    Then the property "am" of "root" is "<am>"
    And the property "measures" of "root" is <measures>
    And the property "weekDays" of "root" is "<weekDays>"

    Examples:
      | name  | am        | measures | weekDays  |
      | en_US | AM PM     | 1        | 1,2,3,4,5 |
      | en_GB | am pm     | 2        | 1,2,3,4,5 |
      | de_DE | AM PM     | 0        | 1,2,3,4,5 |
      | is_IS | f.h. e.h. | 0        | 1,2,3,4,5 |
      | ar_EG | ص م       | 0        | 1,2,3,4,0 |
      | C     | AM PM     | 0        | 1,2,3,4,5 |

  Rule: Reading a date back

    @api-QtQml.Date.fromLocaleDateString @api-QtQml.Date.fromLocaleTimeString @api-QtQml.Date.fromLocaleString
    Scenario Outline: A text in a locale's format is read as a date
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          readonly property var read: Date.<function>(Qt.locale("<name>"), "<text>", <format>)
          property string date: read.getFullYear() + "-" + (read.getMonth() + 1) + "-" + read.getDate()
          property string time: read.getHours() + ":" + read.getMinutes() + ":" + read.getSeconds()
        }
        """
      Then the property "date" of "root" is "<date>"
      And the property "time" of "root" is "<time>"

      Examples:
        | function             | name  | text                    | format                      | date     | time    |
        | fromLocaleDateString | en_US | Friday, January 5, 2024 | Locale.LongFormat           | 2024-1-5 | 0:0:0   |
        | fromLocaleDateString | en_US | 1/5/24                  | Locale.ShortFormat          | 1924-1-5 | 0:0:0   |
        | fromLocaleDateString | en_US | 01/05/2024              | "M/d/yyyy"                  | 2024-1-5 | 0:0:0   |
        | fromLocaleDateString | en_US | 05 january 2024         | "dd MMMM yyyy"              | 2024-1-5 | 0:0:0   |
        | fromLocaleDateString | en_US | 05 Jan 2024             | "dd MMM yyyy"               | 2024-1-5 | 0:0:0   |
        | fromLocaleDateString | de_DE | Freitag, 5. Januar 2024 | Locale.LongFormat           | 2024-1-5 | 0:0:0   |
        | fromLocaleString     | en_US | 2024-01-05 14:07        | "yyyy-MM-dd HH:mm"          | 2024-1-5 | 14:7:0  |
        | fromLocaleString     | en_US | 1/5/24 2:07 PM          | Locale.ShortFormat          | 1924-1-5 | 14:7:0  |
        | fromLocaleString     | en_US | 05.01.2024 9:30:15 am   | "dd.MM.yyyy h:mm:ss ap"     | 2024-1-5 | 9:30:15 |

    @api-QtQml.Date.fromLocaleTimeString
    Scenario Outline: A time is read on its own
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          readonly property var read: Date.fromLocaleTimeString(Qt.locale(), "<text>", "<format>")
          property string time: read.getHours() + ":" + read.getMinutes() + ":" + read.getSeconds()
        }
        """
      Then the property "time" of "root" is "<time>"

      Examples:
        | text     | format   | time    |
        | 2:07 PM  | h:mm AP  | 14:7:0  |
        | 2:07 pm  | h:mm AP  | 14:7:0  |
        | 12:07 AM | h:mm AP  | 0:7:0   |
        | 14:07:09 | HH:mm:ss | 14:7:9  |

    @api-QtQml.Date.fromLocaleDateString @api-QtQml.Date.fromLocaleTimeString
    Scenario Outline: A text that does not fit the format is an invalid date
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          readonly property var read: Date.<function>(Qt.locale(), "<text>", <format>)
          property bool invalid: read instanceof Date && isNaN(read.getTime())
        }
        """
      Then the property "invalid" of "root" is true

      Examples:
        | function             | text                    | format             |
        | fromLocaleDateString | garbage                 | "dd MMMM yyyy"     |
        | fromLocaleDateString | 05 January 2024 x       | "dd MMMM yyyy"     |
        | fromLocaleDateString | 31/2/2024               | "d/M/yyyy"         |
        | fromLocaleDateString | 1/5/2024                | "M/d/yy"           |
        | fromLocaleDateString | 1/5/2024                | "MM/dd/yyyy"       |
        | fromLocaleDateString | Monday, January 5, 2024 | Locale.LongFormat  |
        | fromLocaleTimeString | 13:07 PM                | "h:mm AP"          |

    @api-QtQml.Date.fromLocaleDateString
    Scenario: Reading a date needs a locale
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string error: ""
          Component.onCompleted: {
            try { Date.fromLocaleDateString() } catch (e) { error = e.message }
          }
        }
        """
      Then the property "error" of "root" is "Locale: Date.fromLocaleDateString(): Invalid arguments"

  Rule: Qt.formatDateTime takes a Qt.DateFormat, a format string or a locale

    @api-QtQml.Qt.formatDateTime @api-QtQml.Qt.formatDate @api-QtQml.Qt.formatTime
    Scenario Outline: The formats of Qt.formatDateTime
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          readonly property var day: new Date(2024, 0, 5, 14, 7, 9)
          property string text: <expression>
        }
        """
      Then the property "text" of "root" is "<text>"

      Examples:
        | expression                                   | text                       |
        | Qt.formatDateTime(day)                       | 1/5/24 2:07 PM             |
        | Qt.formatDate(day)                           | 1/5/24                     |
        | Qt.formatTime(day)                           | 2:07 PM                    |
        | Qt.formatDateTime(day, Qt.TextDate)          | Fri Jan 5 14:07:09 2024    |
        | Qt.formatDateTime(day, Locale.LongFormat)    | Fri Jan 5 14:07:09 2024    |
        | Qt.formatDateTime(day, Qt.ISODate)           | 2024-01-05T14:07:09        |
        | Qt.formatDateTime(day, Qt.ISODateWithMs)     | 2024-01-05T14:07:09.000    |
        | Qt.formatDateTime(day, Qt.RFC2822Date)       | 05 Jan 2024 14:07:09 +0000 |
        | Qt.formatDateTime(day, Locale.NarrowFormat)  |                            |
        | Qt.formatDate(day, Qt.TextDate)              | Fri Jan 5 2024             |
        | Qt.formatDate(day, Qt.ISODate)               | 2024-01-05                 |
        | Qt.formatDate(day, Qt.RFC2822Date)           | 05 Jan 2024                |
        | Qt.formatTime(day, Qt.TextDate)              | 14:07:09                   |
        | Qt.formatTime(day, Qt.ISODateWithMs)         | 14:07:09.000               |
        | Qt.formatDateTime(day, "h:mm AP ddd MMM d")  | 2:07 PM Fri Jan 5          |
        | Qt.formatTime(day, "hh:mm ap")               | 02:07 pm                   |
        | Qt.formatDate(day, Qt.locale("de_DE"))       | 05.01.24                   |
        | Qt.formatDateTime(day, Qt.locale("de_DE"))   | 05.01.24 14:07             |
        | Qt.formatDate(new Date(NaN), "yyyy")         |                            |

  Rule: Numbers

    @api-QtQml.Number.toLocaleString
    Scenario Outline: A number written by a locale
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string text: <expression>
        }
        """
      Then the property "text" of "root" is "<text>"

      Examples:
        | expression                                       | text        |
        | Number(1234.5).toLocaleString()                  | 1,234.5     |
        | Number(1234567.891).toLocaleString()             | 1.23457e+06 |
        | Number(-1234.5).toLocaleString()                 | -1,234.5    |
        | Number(1234.5).toLocaleString(Qt.locale())       | 1,234.50    |
        | Number(1234.5).toLocaleString(Qt.locale("de_DE")) | 1.234,50   |
        | Number(1234.5).toLocaleString(Qt.locale("C"))    | 1234.50     |
        | Number(1234.5).toLocaleString(Qt.locale(), "f", 0) | 1,235     |
        | Number(1234.5).toLocaleString(Qt.locale(), "f", 3) | 1,234.500 |
        | Number(1234.5).toLocaleString(Qt.locale(), "e")  | 1.23e+03    |
        | Number(1234.5).toLocaleString(Qt.locale(), "E")  | 1.23E+03    |
        | Number(1234.567).toLocaleString(Qt.locale(), "g") | 1.2e+03    |
        | Number(1.5).toLocaleString(Qt.locale(), "g")     | 1.5         |
        | Number(12345678).toLocaleString(Qt.locale(), "f", 0) | 12,345,678 |
        | Number(1234.5).toLocaleString(Qt.locale("ar_EG")) | ١٬٢٣٤٫٥٠   |

    @api-QtQml.Number.toLocaleCurrencyString
    Scenario: Without a locale a currency amount is a plain number
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string text: Number(1234.5).toLocaleCurrencyString()
        }
        """
      Then the property "text" of "root" is "1,234.5"
