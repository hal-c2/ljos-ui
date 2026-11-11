# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#grabToImage-method
#   https://doc.qt.io/qt-6/qml-qtquick-itemgrabresult.html

Feature: What nothing paints in an item's picture
  The picture Item.grabToImage takes is of the item alone: where neither
  it nor anything it holds paints, the picture is transparent, and shows
  what is under it wherever it is drawn.

  # Qt's software renderer fills those pixels with black when everything
  # that does paint is opaque, and leaves them transparent otherwise; its
  # documentation does not say either. Ljós leaves them transparent always.
  @qt-diverges @api-QtQuick.Item.grabToImage
  Scenario: A gap in the item shows what is under its picture
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"
        width: 120; height: 40
        color: "#00ff00"
        property var asked: "not yet"
        function grab() { asked = src.grabToImage(r => shot.source = r.url) }
        Item {
          id: src
          width: 20; height: 10
          Rectangle { width: 10; height: 10; color: "#ff0000" }
          Rectangle { x: 15; width: 5; height: 10; color: "#0000ff" }
        }
        Image { id: shot; objectName: "shot"; x: 60; y: 0 }
      }
      """
    When the method "grab" of "root" is called
    And 200 milliseconds pass
    Then the property "asked" of "root" is true
    And the pixel at 65, 5 of the window is "#ff0000"
    And the pixel at 72, 5 of the window is "#00ff00"
    And the pixel at 77, 5 of the window is "#0000ff"
