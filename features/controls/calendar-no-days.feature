# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-monthgrid.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-dayofweekrow.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-weeknumbercolumn.html
# Verified on Qt 6.12.0 (qml6, offscreen).

@qt-parity
Feature: A calendar with no days asks for no room
  A month grid, a day of week row or a week number column whose delegate
  is taken away has no days to lay out, and asks for no room for them: its
  padding is all that is left. Given a delegate again, it asks for its
  days' room again.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        objectName: "root"
        property bool days: true
        Component { id: cell; Item { implicitWidth: 20; implicitHeight: 10 } }
        DayOfWeekRow { objectName: "row"; delegate: days ? cell : null }
        WeekNumberColumn { objectName: "column"; y: 30; month: Calendar.February; year: 2024
                           delegate: days ? cell : null }
        MonthGrid { objectName: "grid"; y: 130; month: Calendar.February; year: 2024
                    delegate: days ? cell : null }
      }
      """

  Scenario Outline: With no days, the padding is all that is left
    When the property "days" of "root" is set to false
    And 100 milliseconds pass
    Then the property "width" of "<name>" is <width>
    And the property "height" of "<name>" is <height>

    Examples:
      | name   | width | height |
      | row    | 0     | 12     |
      | column | 12    | 0      |

  Scenario Outline: Given days again, a calendar asks for their room again
    When the property "days" of "root" is set to false
    And 100 milliseconds pass
    And the property "days" of "root" is set to true
    And 100 milliseconds pass
    Then the property "width" of "<name>" is <width>
    And the property "height" of "<name>" is <height>

    Examples:
      | name   | width | height |
      | row    | 176   | 22     |
      | column | 32    | 90     |
      | grid   | 176   | 90     |

  # qml6 keeps a month grid with no days as tall as the five gaps between
  # its rows (30 at a spacing of 6), while a day of week row and a week
  # number column with no days ask for nothing. Ljós asks for nothing for
  # all three.
  @qt-diverges
  Scenario: A month grid with no days asks for no room
    When the property "days" of "root" is set to false
    And 100 milliseconds pass
    Then the property "width" of "grid" is 0
    And the property "height" of "grid" is 0
