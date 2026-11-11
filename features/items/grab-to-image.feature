# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#grabToImage-method
#   https://doc.qt.io/qt-6/qml-qtquick-itemgrabresult.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A picture of an item's pixels
  Item.grabToImage(callback, targetSize) takes a picture of the item and
  what it holds, as the next frame draws them, and calls back with an
  ItemGrabResult: its url names the picture where an image source is
  wanted, and saveToFile writes it. The item is drawn in its own
  coordinates, cut to its bounds: its own opacity and visibility do not
  count. A target size stretches the picture to it. A grab the item cannot give returns false
  at once, says why, and never calls back.

  Rule: A grab the item can give is drawn

    Background:
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          id: root; objectName: "root"
          width: 120; height: 40; color: "#00ff00"
          property var asked: "not yet"
          property var saved: "not yet"
          function grab() { asked = src.grabToImage(r => shot.source = r.url) }
          function grabSized(w, h) {
            asked = src.grabToImage(r => shot.source = r.url, Qt.size(w, h))
          }
          function save(name) {
            asked = src.grabToImage(r => {
              saved = r.saveToFile(Qt.resolvedUrl(name))
              if (saved) shot.source = name
            })
          }
          Item {
            id: src; objectName: "src"
            width: 20; height: 10
            opacity: 0.5
            Rectangle { width: 10; height: 10; color: "#ff0000" }
            Rectangle { x: 15; width: 10; height: 10; color: "#0000ff" }
          }
          Image { id: shot; objectName: "shot"; x: 60; y: 0 }
        }
        """

    @api-QtQuick.Item.grabToImage @api-QtQuick.ItemGrabResult.url
    Scenario: The picture shows the item's pixels where an image is wanted
      When the method "grab" of "root" is called
      And 200 milliseconds pass
      Then the property "asked" of "root" is true
      And the property "implicitWidth" of "shot" is 20
      And the property "implicitHeight" of "shot" is 10
      And the pixel at 65, 5 of the window is "#ff0000"
      And the pixel at 77, 5 of the window is "#0000ff"
      And the pixel at 85, 5 of the window is "#00ff00"

    @api-QtQuick.Item.grabToImage
    Scenario: A target size stretches the picture to it
      When the method "grabSized" of "root" is called with 40, 5
      And 200 milliseconds pass
      Then the property "asked" of "root" is true
      And the property "implicitWidth" of "shot" is 40
      And the property "implicitHeight" of "shot" is 5
      And the pixel at 70, 2 of the window is "#ff0000"
      And the pixel at 95, 2 of the window is "#0000ff"
      And the pixel at 70, 7 of the window is "#00ff00"

    @api-QtQuick.ItemGrabResult.saveToFile
    Scenario: The picture is saved as a PNG file
      When the method "save" of "root" is called with "shot.png"
      And 200 milliseconds pass
      Then the property "saved" of "root" is true
      And the property "implicitWidth" of "shot" is 20
      And the pixel at 65, 5 of the window is "#ff0000"
      And the pixel at 77, 5 of the window is "#0000ff"

    @api-QtQuick.ItemGrabResult.saveToFile
    Scenario: A file of a type no picture is written as is refused
      When the method "save" of "root" is called with "shot.unknownext"
      And 200 milliseconds pass
      Then the property "saved" of "root" is false
      And the property "implicitWidth" of "shot" is 0

    @api-QtQuick.Item.grabToImage
    Scenario: A hidden item is drawn all the same
      When the property "visible" of "src" is set to false
      And the method "grab" of "root" is called
      And 200 milliseconds pass
      Then the property "asked" of "root" is true
      And the pixel at 65, 5 of the window is "#ff0000"
      And the pixel at 77, 5 of the window is "#0000ff"

    @api-QtQuick.Item.grabToImage
    Scenario: An item of no size has no picture
      When the property "width" of "src" is set to 0
      And the method "grab" of "root" is called
      And 200 milliseconds pass
      Then the property "asked" of "root" is false
      And the log has a warning matching "QML Item: grabToImage: item's width and/or height are less than 1"
      And the property "implicitWidth" of "shot" is 0

  Rule: A grab the item cannot give is refused at once

    Background:
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          id: root; objectName: "root"
          width: 120; height: 40
          property var asked: "not yet"
          property bool called: false
          Item { id: src; width: 20; height: 10 }
          function grabWith(callback) { asked = src.grabToImage(callback) }
          Component { id: made; Rectangle { width: 10; height: 10 } }
          function grabUnshown() {
            asked = made.createObject(null).grabToImage(r => called = true)
          }
        }
        """

    @api-QtQuick.Item.grabToImage
    Scenario: A callback that is no function is refused
      When the method "grabWith" of "root" is called with 5
      Then the property "asked" of "root" is false
      And the log has a warning matching "QML Item: grabToImage: 'callback' is not a function"

    @api-QtQuick.Item.grabToImage
    Scenario: An item in no window is refused
      When the method "grabUnshown" of "root" is called
      And 200 milliseconds pass
      Then the property "asked" of "root" is false
      And the property "called" of "root" is false
      And the log has a warning matching "QML Rectangle: grabToImage: item is not attached to a window"
