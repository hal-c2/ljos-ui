# Sources:
#   https://doc.qt.io/qt-6/qml-url.html
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#property-aliases
# Verified on Qt 6.11.2 (qml6, offscreen).
# The todolist demo hands its buttons their icons this way, from pages in
# another directory than the buttons.

@qt-parity @api-QtQml.url
Feature: An address given through an alias
  A url property keeps a relative address as it was written, also when it
  is given through an alias from a file in another directory. What uses
  it, an Image say, reads it from its own file: the todolist demo's icons,
  given from its pages, are looked for beside its buttons, and are not
  found there, on qml6 as well.

  Background:
    Given the file "parts/Box.qml":
      """qml
      import QtQuick
      Item {
        property alias pic: holder.u
        Item { id: holder; property url u }
        property url own: "art/own.png"
        property alias icon: image.source
        Image { id: image; objectName: "image" }
      }
      """
    And a 4 by 4 image file "art/dot.png" filled with "#0000ff"
    And the QML:
      """qml
      import QtQuick
      import "parts"
      Item {
        objectName: "root"
        Box { id: b; pic: "art/dot.png"; icon: "art/dot.png" }
        property string pic: b.pic
        property string own: b.own
      }
      """

  Scenario: The address stays as written
    Then the property "pic" of "root" is "art/dot.png"
    And the property "own" of "root" is "art/own.png"

  Scenario: An image given an address through an alias looks beside its own file
    Then the property "status" of "image" is 3
