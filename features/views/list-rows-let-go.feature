# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (cacheBuffer)
#   https://doc.qt.io/qt-6/qml-qtquick-column.html
#   https://doc.qt.io/qt-6/qml-qtquick-image.html#fillMode-prop
# Verified on Qt 6.11.2 (qml6, offscreen): rows of 95 make 7 delegates in a
# view of 300 and its cache of 320, and none is let go; rows of a title
# over a picture given only a width make 5 and keep them.
# Delegate counts are bands: Qt does not promise exact numbers.

@qt-parity @api-QtQuick.ListView.cacheBuffer
Feature: A new ListView makes no row it lets go once its rows are laid out
  A row that is a column of a text and something of a fixed length is, until the
  text is laid out, only as long as that something. The view does not take the
  rows to be that long and make delegates it lets go as soon as they have
  their own length. Nor is a row with a picture given only a width as tall as
  the picture: it is as tall as the picture is drawn.

  Scenario: Rows of a text over a block are made once
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 400; height: 300
        property int created: 0
        property int destroyed: 0
        ListView {
          anchors.fill: parent
          model: 40
          delegate: Column {
            required property int index
            objectName: "r" + index
            width: 400
            Text { text: "Row " + parent.index; font.pixelSize: 40 }
            Rectangle { width: 10; height: 50 }
            Component.onCompleted: root.created++
            Component.onDestruction: root.destroyed++
          }
        }
      }
      """
    When 300 milliseconds pass
    Then the property "created" of "root" is at least 6
    And the property "created" of "root" is at most 8
    And the property "destroyed" of "root" is 0

  Scenario: Rows of a title over a picture given only a width are as tall as the picture is drawn
    Given a 400 by 200 image file "wide.png" filled with "#2060a0"
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 400; height: 300
        property int created: 0
        property int destroyed: 0
        ListView {
          anchors.fill: parent
          model: 40
          delegate: Column {
            required property int index
            width: 400
            Text { text: "Row " + parent.index; font.pixelSize: 40 }
            Image {
              objectName: "p" + parent.index
              source: "wide.png"
              width: 200
              fillMode: Image.PreserveAspectFit
            }
            Component.onCompleted: root.created++
            Component.onDestruction: root.destroyed++
          }
        }
      }
      """
    When 300 milliseconds pass
    Then the property "height" of "p0" is 100
    And the property "created" of "root" is at least 5
    And the property "created" of "root" is at most 7
    And the property "destroyed" of "root" is 0
