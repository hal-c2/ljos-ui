# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-monthgrid.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-dayofweekrow.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#contentItem-prop
# Verified on Qt 6.12.0 (qml6, offscreen).

@qt-parity
Feature: Where a calendar puts its days
  The style's content item of a month grid or a day of week row is where
  its days are made, and it asks for their room. A content item the
  application gives one is the application's: no days are made in it, and
  it asks for the room the application says. A month grid shown a month out
  of range shows none, and shows its days once it is given one in range.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        objectName: "root"
        property real room: 50
        Component { id: month; Item { required property int index; objectName: "m" + index
                                      implicitWidth: 20; implicitHeight: 10 } }
        Component { id: day; Item { required property int index; objectName: "d" + index
                                    implicitWidth: 20; implicitHeight: 10 } }
        Component { id: late; Item { required property int index; objectName: "l" + index
                                     implicitWidth: 20; implicitHeight: 10 } }
        MonthGrid { objectName: "grid"; month: Calendar.February; year: 2024; delegate: month
                    contentItem: Item { implicitWidth: room; implicitHeight: 40 } }
        DayOfWeekRow { objectName: "row"; y: 50; delegate: day
                       contentItem: Item { implicitWidth: room; implicitHeight: 40 } }
        MonthGrid { objectName: "late"; y: 110; month: 12; year: 2024; delegate: late }
      }
      """

  Scenario Outline: A content item the application gives keeps the room it asks for
    When the property "room" of "root" is set to <room>
    And 100 milliseconds pass
    Then the property "width" of "<name>" is <width>
    And the property "height" of "<name>" is <height>

    Examples:
      | name | room | width | height |
      | grid | 50   | 50    | 40     |
      | grid | 80   | 80    | 40     |
      | row  | 50   | 50    | 52     |
      | row  | 80   | 80    | 52     |

  Scenario: No days are made in a content item the application gives
    When 100 milliseconds pass
    Then the object "m0" does not exist
    And the object "d0" does not exist

  # qml6 refuses a month out of range ("month 12 is out of range [0...11]")
  # and goes on showing the month it had. Ljós keeps the month it is given,
  # and there are no days in it to show.
  @qt-diverges
  Scenario: A month grid given a month out of range shows no days
    When 100 milliseconds pass
    Then the object "l0" does not exist
    And the property "width" of "late" is 0
    And the property "height" of "late" is 0

  Scenario: A month grid given a month in range shows its days
    When the property "month" of "late" is set to 1
    And 100 milliseconds pass
    Then the object "l41" exists
    And the property "width" of "late" is 176
    And the property "height" of "late" is 90
