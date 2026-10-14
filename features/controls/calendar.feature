# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-monthgrid.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-dayofweekrow.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-weeknumbercolumn.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-calendarmodel.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: MonthGrid, DayOfWeekRow, WeekNumberColumn and CalendarModel
  A month grid shows the days of a `month` and `year` in six rows of seven,
  the first row starting on the first day of the week of its `locale`, with
  days of the months beside it filling the rows. A day of week row and a week
  number column are drawn from the same locale and put beside it. Each cell
  is made from `delegate`, which knows its row of the model.

  Background:
    Given the file "DayGrid.qml":
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      MonthGrid {
        id: self
        objectName: "grid"
        property string log: ""
        function text(d) { return d.getFullYear() + "-" + (d.getMonth() + 1) + "-" + d.getDate() }
        width: 700; height: 420
        month: Calendar.February; year: 2024
        locale: Qt.locale("en_US")
        delegate: Rectangle {
          required property var model
          required property int index
          objectName: "cell" + index
          property string info: model.day + "/" + model.month + "/" + model.year
                                + "/" + model.weekNumber + "/" + model.today
                                + "/" + self.text(model.date)
          color: "red"
        }
        onClicked: (date) => log += "clicked " + self.text(date) + ";"
        onPressed: (date) => log += "pressed " + self.text(date) + ";"
        onReleased: (date) => log += "released " + self.text(date) + ";"
        onPressAndHold: (date) => log += "held " + self.text(date) + ";"
      }
      """
    And the file "Others.qml":
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        MonthGrid {
          objectName: "german"
          y: 430; width: 140; height: 100
          month: Calendar.February; year: 2024
          locale: Qt.locale("de_DE")
          delegate: Item { required property var model; objectName: "de" + model.day + "_" + model.month }
        }
        MonthGrid {
          objectName: "sized"
          y: 540; width: 736; height: 330
          month: Calendar.February; year: 2024
          locale: Qt.locale("en_US")
          delegate: Item { required property int index; objectName: "s" + index }
        }
      }
      """
    And the file "Weeks.qml":
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        WeekNumberColumn {
          objectName: "wnc"
          y: 1000; width: 112; height: 330
          month: Calendar.February; year: 2024
          locale: Qt.locale("en_US")
          delegate: Item {
            required property int index
            required property int weekNumber
            objectName: "w" + index
            property string info: "" + weekNumber
          }
        }
      }
      """
    And the file "Names.qml":
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        DayOfWeekRow {
          objectName: "dow"
          y: 900; width: 736; height: 40
          locale: Qt.locale("en_US")
          delegate: Item {
            required property var model
            required property int index
            required property string shortName
            required property string narrowName
            required property string longName
            objectName: "d" + index
            property string info: shortName + "/" + narrowName + "/" + longName + "/" + model.day
          }
        }
      }
      """
    And the file "NamesDe.qml":
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        DayOfWeekRow {
          objectName: "dowDe"
          y: 950; width: 736; height: 40
          locale: Qt.locale("de_DE")
          delegate: Item {
            required property int index
            required property string longName
            objectName: "g" + index
            property string info: longName
          }
        }
      }
      """
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root
        objectName: "root"
        width: 800; height: 600
        property alias log: grid.log
        property int monthAt0; property int monthAt2
        property int yearAt0; property int yearAt2
        property int indexOfFeb; property int indexOfDate; property int indexBefore
        property int modelCount: cal.count
        Component.onCompleted: {
          monthAt0 = cal.monthAt(0); monthAt2 = cal.monthAt(2)
          yearAt0 = cal.yearAt(0); yearAt2 = cal.yearAt(2)
          indexOfFeb = cal.indexOf(2024, 1)
          indexOfDate = cal.indexOf(new Date(2023, 11, 5))
          indexBefore = cal.indexOf(2023, 5)
        }
        DayGrid { id: grid }
        Others {}
        Names {}
        NamesDe {}
        Weeks {}
        CalendarModel { id: cal; from: new Date(2023, 10, 1); to: new Date(2024, 1, 15) }
      }
      """

  @qt-parity
  Rule: Which days are shown

    Scenario Outline: The first row of February 2024 starts on the Sunday of an American week
      Then the property "info" of "cell<index>" is "<info>"

      Examples:
        | index | info                 |
        | 0     | 28/0/2024/4/false/2024-1-28 |
        | 3     | 31/0/2024/5/false/2024-1-31 |
        | 4     | 1/1/2024/5/false/2024-2-1   |
        | 9     | 6/1/2024/6/false/2024-2-6   |
        | 32    | 29/1/2024/9/false/2024-2-29 |
        | 33    | 1/2/2024/9/false/2024-3-1   |
        | 41    | 9/2/2024/10/false/2024-3-9  |

    Scenario: A grid with the German locale starts its week on a Monday
      Then the object "de29_0" exists
      And the object "de1_1" exists
      And the object "de28_0" does not exist

  @qt-parity
  Rule: Pressing a day

    Scenario: A press tells the date, and the release and the click follow
      When the user presses the pointer at 249, 103
      Then the property "log" of "root" is "pressed 2024-2-6;"
      When the user releases the pointer at 249, 103
      Then the property "log" of "root" is "pressed 2024-2-6;released 2024-2-6;clicked 2024-2-6;"

    Scenario: A day of the month beside this one is a day too
      When the user presses the pointer at 20, 20
      And the user releases the pointer at 20, 20
      Then the property "log" of "root" is "pressed 2024-1-28;released 2024-1-28;clicked 2024-1-28;"

    Scenario: A press that moves to another day lets go of the first and presses the second
      When the user presses the pointer at 249, 103
      And the user moves the pointer to 350, 103
      Then the property "log" of "root" is "pressed 2024-2-6;released 2024-2-6;pressed 2024-2-7;"
      When the user releases the pointer at 350, 103
      Then the property "log" of "root" is "pressed 2024-2-6;released 2024-2-6;pressed 2024-2-7;released 2024-2-7;clicked 2024-2-7;"

    Scenario: A press in the space between two days is no press
      When the user presses the pointer at 98, 103
      And the user releases the pointer at 98, 103
      Then the property "log" of "root" is ""

    Scenario: A press that is held is told after a while, and still ends in a click
      When the user presses the pointer at 249, 103
      And 1 second passes
      Then the property "log" of "root" is "pressed 2024-2-6;held 2024-2-6;"
      When the user releases the pointer at 249, 103
      Then the property "log" of "root" is "pressed 2024-2-6;held 2024-2-6;released 2024-2-6;clicked 2024-2-6;"

    Scenario: A press that is let go before the hold time is no hold
      When the user presses the pointer at 249, 103
      And 300 milliseconds pass
      And the user releases the pointer at 249, 103
      And 1 second passes
      Then the property "log" of "root" is "pressed 2024-2-6;released 2024-2-6;clicked 2024-2-6;"

  @qt-parity
  Rule: Names, titles and sizes

    Scenario Outline: A day of week row starts on the first day of the week of its locale
      Then the property "info" of "<name>" is "<info>"

      Examples:
        | name | info                           |
        | d0   | Sun/S/Sunday/0                 |
        | d1   | Mon/M/Monday/1                 |
        | d6   | Sat/S/Saturday/6               |
        | g0   | Montag                         |
        | g6   | Sonntag                        |

    Scenario: A month grid names its month in the language of its locale
      Then the property "title" of "grid" is "February 2024"

    Scenario Outline: The cells fill the room they are given, six pixels apart
      When 100 milliseconds pass
      Then the property "<property>" of "<name>" is <value>

      Examples:
        | name | property | value |
        | s0   | width    | 100   |
        | s0   | height   | 50    |
        | s1   | x        | 106   |
        | s7   | y        | 56    |
        | s41  | x        | 636   |
        | s41  | y        | 280   |
        | d0   | width    | 100   |
        | d1   | x        | 106   |
        | d0   | height   | 28    |
        | w0   | width    | 100   |
        | w1   | y        | 56    |
        | w5   | height   | 50    |

    Scenario Outline: A week number column shows the week of the first day of each row
      When 100 milliseconds pass
      Then the property "info" of "w<row>" is "<info>"

      Examples:
        | row | info    |
        | 0   | 4       |
        | 1   | 5       |
        | 5   | 9       |

  @qt-parity
  Rule: The calendar model

    Scenario: The model counts the months from its first to its last
      Then the property "modelCount" of "root" is 4

    Scenario Outline: The model tells the month and year of a row, and the row of a month
      Then the property "<property>" of "root" is <value>

      Examples:
        | property    | value |
        | monthAt0    | 10    |
        | monthAt2    | 0     |
        | yearAt0     | 2023  |
        | yearAt2     | 2024  |
        | indexOfFeb  | 3     |
        | indexOfDate | 1     |
        | indexBefore | -1    |
