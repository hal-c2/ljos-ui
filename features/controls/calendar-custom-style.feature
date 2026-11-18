# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-monthgrid.html
#   https://doc.qt.io/qt-6/qtquickcontrols-customize.html#creating-a-custom-style
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#contentItem-prop
# Verified on Qt 6.12.0 (qml6, offscreen, QT_QUICK_CONTROLS_STYLE=Kit).

Feature: A custom style's calendar
  A month grid of a custom style has its days made in the content item
  the style gives it, as a built-in style's has. A content item the
  application gives one is still the application's: no days are made in
  it, and it asks for the room the application says.

  Background:
    Given the file "Kit/qmldir":
      """
      module Kit
      MonthGrid 1.0 MonthGrid.qml
      """
    And the file "Kit/MonthGrid.qml":
      """qml
      import QtQuick
      import QtQuick.Templates as T
      T.AbstractMonthGrid {
        implicitWidth: implicitContentWidth
        implicitHeight: implicitContentHeight
        spacing: 6
        property string madeBy: "Kit"
        contentItem: Item {}
      }
      """
    And the application starts with the controls style "Kit"
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        Component { id: day; Item { required property int index; objectName: "k" + index
                                    implicitWidth: 20; implicitHeight: 10 } }
        Component { id: mine; Item { required property int index; objectName: "o" + index
                                     implicitWidth: 20; implicitHeight: 10 } }
        MonthGrid { objectName: "grid"; month: Calendar.February; year: 2024; delegate: day }
        MonthGrid { objectName: "own"; y: 100; month: Calendar.February; year: 2024; delegate: mine
                    contentItem: Item { implicitWidth: 50; implicitHeight: 40 } }
      }
      """

  # qml6's month grid makes no days itself: a style makes them in its
  # content item from the grid's `source`, so a custom style's empty Item
  # stays empty, 0x0. Ljós's month grid makes them in the content item the
  # style gives it, a custom style's as well as a built-in one's.
  @qt-diverges
  Scenario: The days are made in a custom style's content item
    When 100 milliseconds pass
    Then the property "madeBy" of "grid" is "Kit"
    And the object "k41" exists
    And the property "width" of "grid" is 176
    And the property "height" of "grid" is 90

  @qt-parity
  Scenario: No days are made in a content item the application gives a custom style's month grid
    When 100 milliseconds pass
    Then the property "madeBy" of "own" is "Kit"
    And the object "o0" does not exist
    And the property "width" of "own" is 50
    And the property "height" of "own" is 40
