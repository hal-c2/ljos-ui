# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-image.html#sourceSize-prop
#   https://doc.qt.io/qt-6/qml-qtquick-context2d.html#drawImage-method
#   docs/ARCHITECTURE.md §7.1 Images, §8.5 Memory budgets
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A picture larger than the window
  A picture may hold far more pixels than the window can show. Without a
  "sourceSize" the Image still reports the picture's own size, however
  small it is drawn, and whatever draws the Image itself sees all of it.

  Scenario: A large picture from the web reports its own size
    Given the web server serves a 3000 by 2000 image file "large.png" filled with "#2060a0"
    And the QML:
      """qml
      import QtQuick
      Item {
        width: 200; height: 100
        Image {
          objectName: "picture"
          anchors.fill: parent
          fillMode: Image.PreserveAspectFit
          source: "$SERVER/large.png"
        }
      }
      """
    When 2 seconds pass
    Then the property "status" of "picture" is 1
    And the property "implicitWidth" of "picture" is 3000
    And the property "implicitHeight" of "picture" is 2000
    And the property "sourceSize.width" of "picture" is 3000
    And the property "sourceSize.height" of "picture" is 2000
    And the property "paintedWidth" of "picture" is 150
    And the pixel at 100, 50 of the window is "#2060a0"

  Scenario: A large local picture reports its own size
    Given a 3000 by 2000 image file "large.jpg" filled with "#2060a0"
    And the QML:
      """qml
      import QtQuick
      Item {
        width: 200; height: 100
        Image {
          objectName: "picture"
          width: 150; height: 100
          source: "large.jpg"
        }
      }
      """
    Then the property "implicitWidth" of "picture" is 3000
    And the property "sourceSize.width" of "picture" is 3000
    And the property "sourceSize.height" of "picture" is 2000
    When 100 milliseconds pass
    Then the pixel at 75, 50 of the window is within 4 of "#2060a0"

  Scenario: A Canvas draws a large Image at the picture's own size
    Given a 3000 by 2000 image file "large.png" filled with "#2060a0"
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        width: 200; height: 100
        property string drawn: ""
        Image { id: picture; width: 200; height: 100; source: "large.png" }
        Canvas {
          width: 100; height: 100
          function px(ctx, x, y) { return ctx.getImageData(x, y, 1, 1).data[3] }
          onPaint: {
            var ctx = getContext("2d")
            ctx.drawImage(picture, -2950, -1950)
            root.drawn = px(ctx, 25, 25) + " " + px(ctx, 75, 25) + " " + px(ctx, 25, 75)
          }
        }
      }
      """
    When 200 milliseconds pass
    Then the property "drawn" of "root" is "255 0 0"
