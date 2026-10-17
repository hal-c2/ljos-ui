# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-image.html#sourceSize-prop
#   https://doc.qt.io/qt-6/qml-qtquick-image.html#fillMode-prop
#   docs/ARCHITECTURE.md §7.1 Images, §8.5 Memory budgets

@ljos-only
Feature: A picture is kept at the size its item draws it
  A picture larger than its item is kept at about the size the item
  draws it, at the display's scale, rounded up a little. This holds for
  an item that is made and not painted, such as a list's row kept beyond
  the view, as soon as the picture is loaded. What the item reports does
  not change: "sourceSize" and the implicit size are still the picture's
  own. The oracle does not say what it keeps.

  Scenario: A hidden item keeps its picture at the size it draws it
    Given a 2048 by 1024 image file "big.png" filled with "#2060a0"
    And the QML:
      """qml
      import QtQuick
      Item {
        width: 400; height: 300
        Image {
          objectName: "picture"
          visible: false
          width: 100; height: 50
          source: "big.png"
        }
      }
      """
    Then the property "status" of "picture" is 1
    And the property "sourceSize.width" of "picture" is 2048
    And the property "implicitWidth" of "picture" is 2048
    And the picture of "picture" is kept at 112 by 56 pixels

  Scenario: A row's picture from the web is kept at the size the row draws it
    Given the web server serves a 2048 by 1024 image file "photo.png" filled with "#2060a0"
    And the QML:
      """qml
      import QtQuick
      Item {
        width: 400; height: 300
        Image {
          objectName: "row"
          visible: false
          width: Math.min(200, sourceSize.width); height: 100
          fillMode: Image.PreserveAspectFit
          source: "$SERVER/photo.png"
        }
      }
      """
    When 2 seconds pass
    Then the property "status" of "row" is 1
    And the property "width" of "row" is 200
    And the picture of "row" is kept at 224 by 112 pixels
    When the property "visible" of "row" is set to true
    And 100 milliseconds pass
    Then the picture of "row" is kept at 224 by 112 pixels
    And the pixel at 100, 50 of the window is "#2060a0"
