# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-monthgrid.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-dayofweekrow.html
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html
# Verified on Qt 6.12.0 (qml6, offscreen).

@qt-parity
Feature: A month grid and a day of week row that take their size from their days
  A calendar given no size is as large as its days ask: the widest day once
  per column and the tallest once per row, `spacing` apart. One in a column
  layout is as wide as the column, and its days are spread over that. The
  days asking for room and the room being given is no binding loop.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      import QtQuick.Layouts
      Item {
        ColumnLayout {
          width: 736; height: 400
          DayOfWeekRow {
            Layout.fillWidth: true; locale: Qt.locale("en_US")
            delegate: Label { required property string narrowName; required property int index
                              objectName: "d" + index; text: narrowName }
          }
          MonthGrid {
            objectName: "grid"; Layout.fillWidth: true; Layout.fillHeight: true
            month: Calendar.February; year: 2024; locale: Qt.locale("en_US")
            delegate: Label { required property var model; required property int index
                              objectName: "m" + index; text: model.day }
          }
        }
        MonthGrid {
          id: free; objectName: "free"; y: 420; month: Calendar.February; year: 2024
          property real day: 20
          delegate: Item { required property int index; objectName: "f" + index
                           implicitWidth: free.day; implicitHeight: 10 }
        }
      }
      """

  Scenario: Taking the size from the days is no binding loop
    When 100 milliseconds pass
    Then the log has no warning matching "Binding loop"

  Scenario Outline: The days are laid out in the room they asked for or were given
    When 100 milliseconds pass
    Then the property "<property>" of "<name>" is <value>

    Examples: In a column layout, as wide as the column
      | name | property | value |
      | grid | width    | 736   |
      | d0   | width    | 100   |
      | d1   | x        | 106   |
      | d6   | x        | 636   |
      | m0   | width    | 100   |
      | m1   | x        | 106   |
      | m41  | x        | 636   |

    Examples: Given no size, as large as the days ask
      | name | property | value |
      | free | width    | 176   |
      | free | height   | 90    |
      | f0   | width    | 20    |
      | f1   | x        | 26    |
      | f7   | y        | 16    |

  # qml6 keeps the size a calendar given no size had when its days were
  # made: days that later ask for more room are cut off, and ones that ask
  # for less leave a gap. Ljós follows what the days ask for now.
  @qt-diverges
  Scenario: A calendar given no size grows with its days
    When the property "day" of "free" is set to 40
    And 100 milliseconds pass
    Then the property "width" of "free" is 316
    And the property "width" of "f0" is 40
    And the log has no warning matching "Binding loop"
