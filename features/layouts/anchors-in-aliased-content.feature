# Sources:
#   https://doc.qt.io/qt-6/qtquick-positioning-anchors.html
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#default-properties
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#property-aliases
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Anchors of content given through a default property alias
  What a component's user puts in it through a default property alias is
  anchored to the item the alias leads to, in every instance of the
  component, also when other items of the component are anchored to that
  item too.

  Background:
    Given the file "Frame.qml":
      """qml
      import QtQuick
      Item {
        default property alias content: inner.data
        Item { id: inner; anchors.fill: parent }
        Item { anchors.fill: inner }
      }
      """

  Scenario Outline: Content anchored to the bottom of its parent sits there in each delegate
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 400; height: 300
        Repeater {
          model: 6
          delegate: Item {
            required property int index
            x: index * 60; width: 30; height: 200
            Frame {
              anchors.fill: parent
              Rectangle {
                objectName: "bar" + index
                anchors.bottom: parent.bottom
                width: 30; height: 120
              }
            }
          }
        }
      }
      """
    Then the property "y" of "<bar>" is 80

    Examples:
      | bar  |
      | bar0 |
      | bar1 |
      | bar2 |
      | bar5 |
