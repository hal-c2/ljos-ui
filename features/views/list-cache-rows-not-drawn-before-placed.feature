# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (cacheBuffer)
#   https://doc.qt.io/qt-6/qml-qtquick-gridview.html (cacheBuffer)
# Qt makes a view's delegates ahead of time, over as many screen
# refreshes as it takes, and draws none until the view has put it in its
# place. Ljós-only: the Qt oracle cannot be given a slow clock, nor show
# each picture it draws.

@ljos-only @api-QtQuick.ListView.cacheBuffer @api-QtQuick.GridView.cacheBuffer
Feature: A delegate made ahead for the cacheBuffer is not drawn before the view places it
  A row the view makes for its cacheBuffer takes a while to make, and
  until the view puts it in its place it stands where any item starts, at
  the top of the view. It is not drawn there, over the first row.

  Background:
    Given each reading of the application's clock takes 40 microseconds
    And every picture of the window is kept

  Scenario Outline: The rows in sight are all that is drawn while the <view> makes its cacheBuffer
    Given the QML:
      """qml
      import QtQuick
      <view> {
        component Leaf: Item { width: parent.width - 1; height: 1 }
        objectName: "view"; width: 200; height: 100; cacheBuffer: 100
        <cells>
        model: 40
        delegate: Rectangle {
          required property int index
          width: 200; height: 20; color: index === 0 ? "#ff0000" : "#0000ff"
          Leaf {} Leaf {} Leaf {} Leaf {} Leaf {} Leaf {} Leaf {} Leaf {} Leaf {} Leaf {}
          Leaf {} Leaf {} Leaf {} Leaf {} Leaf {} Leaf {} Leaf {} Leaf {} Leaf {} Leaf {}
          Leaf {} Leaf {} Leaf {} Leaf {} Leaf {} Leaf {} Leaf {} Leaf {} Leaf {} Leaf {}
          Leaf {} Leaf {} Leaf {} Leaf {} Leaf {} Leaf {} Leaf {} Leaf {} Leaf {} Leaf {}
        }
      }
      """
    Then the pixel at 5, 5 of the window is "#0000ff" in no picture kept
    And the pixel at 5, 5 of the window is "#ff0000"

    Examples:
      | view     | cells                          |
      | ListView | spacing: 0                     |
      | GridView | cellWidth: 200; cellHeight: 20 |
