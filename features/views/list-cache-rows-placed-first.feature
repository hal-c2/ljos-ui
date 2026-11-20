# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (cacheBuffer)
#   https://doc.qt.io/qt-6/qml-qtquick-gridview.html (cacheBuffer)
# Verified on Qt 6.11.2 (qml6, offscreen): a view shows its rows in order
# as soon as it is shown, while the rows of its cacheBuffer are still being
# made; none is drawn where the view has not placed it.

@qt-parity @api-QtQuick.ListView.cacheBuffer
Feature: A view draws the rows of its cacheBuffer only where it places them
  Rows past what a view shows are made over a while, and each is the
  view's from when it is made. Until the view places it, it is not drawn,
  so it never covers the first rows.

  Background:
    Given the file "HeavyRow.qml":
      """qml
      import QtQuick
      Rectangle {
        required property int index
        width: 240; height: 24
        color: index === 0 ? "#ff0000" : index < 5 ? "#00ff00" : "#0000ff"
        Repeater { model: 60; Item { width: 1; height: 1 } }
      }
      """

  Scenario Outline: The first rows of a <view> are in order once it shows
    Given the QML:
      """qml
      import QtQuick
      <view> {
        width: 240; height: 120; cacheBuffer: 200
        <cells>
        model: 40
        delegate: HeavyRow {}
      }
      """
    Then the pixel at 120, 12 of the window is "#ff0000"
    And the pixel at 120, 108 of the window is "#00ff00"
    When 200 milliseconds pass
    Then the pixel at 120, 12 of the window is "#ff0000"
    And the pixel at 120, 108 of the window is "#00ff00"

    Examples:
      | view     | cells                          |
      | ListView | spacing: 0                     |
      | GridView | cellWidth: 240; cellHeight: 24 |

  # The oracle shows no frame on its own to look at.
  @ljos-only
  Scenario Outline: The first frame of a <view> has its first rows in order
    Given the QML:
      """qml
      import QtQuick
      <view> {
        width: 240; height: 120; cacheBuffer: 200
        <cells>
        model: 40
        delegate: HeavyRow {}
      }
      """
    When the window shows its first frame
    Then the pixel at 120, 12 of the window is "#ff0000"
    And the pixel at 120, 108 of the window is "#00ff00"

    Examples:
      | view     | cells                          |
      | ListView | spacing: 0                     |
      | GridView | cellWidth: 240; cellHeight: 24 |
