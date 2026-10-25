# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#default-properties
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#property-aliases
#   https://doc.qt.io/qt-6/qtqml-documents-definetypes.html#inline-components
# Verified on Qt 6.11.2 (qml6, offscreen).
# Issue #132.

@qt-parity
Feature: A default property alias declared again under the same name
  A type that declares the default property alias of its base again, under the same name,
  gives what its own definition holds to the base's default property, and what its users
  give it to its own alias. It does so as a file and as an inline component, and in a chain
  of such types each level's children go one level in.

  Background:
    Given the file "Card.qml":
      """qml
      import QtQuick
      Rectangle {
        default property alias content: inner.data
        Item { id: inner; objectName: "inner"; anchors.fill: parent; anchors.margins: 4 }
      }
      """
    And the file "Same.qml":
      """qml
      import QtQuick
      Card {
        default property alias content: padded.data
        Item { id: padded; objectName: "padded"; anchors.fill: parent; anchors.margins: 10 }
      }
      """
    And the file "Top.qml":
      """qml
      import QtQuick
      Same {
        default property alias content: top.data
        Item { id: top; objectName: "top"; anchors.fill: parent; anchors.margins: 1 }
      }
      """
    And the file "Where.js":
      """js
      .pragma library
      function of(o, root) {
        const path = []
        for (let p = o.parent; p && p !== root; p = p.parent) path.push(p.objectName || "card")
        return path.join("<") + "@" + o.mapToItem(root, 0, 0).x
      }
      """
    And the QML:
      """qml
      import QtQuick
      import "Where.js" as Where
      Item {
        id: root
        objectName: "root"
        property string seen: ""
        component InSame: Card {
          default property alias content: padded.data
          Item { id: padded; objectName: "padded"; anchors.fill: parent; anchors.margins: 10 }
        }
        component InOther: Card {
          default property alias other: padded.data
          Item { id: padded; objectName: "padded"; anchors.fill: parent; anchors.margins: 10 }
        }
        component InTop: InSame {
          default property alias content: top.data
          Item { id: top; objectName: "top"; anchors.fill: parent; anchors.margins: 1 }
        }
        Same { x: 0; width: 50; height: 50; Item { id: a } }
        InSame { x: 100; width: 50; height: 50; Item { id: b } }
        InOther { x: 200; width: 50; height: 50; Item { id: c } }
        Top { x: 300; width: 50; height: 50; Item { id: d } }
        InTop { y: 100; width: 50; height: 50; Item { id: e } }
        function look() { seen = [a, b, c, d, e].map(o => Where.of(o, root)).join(" ") }
      }
      """

  Scenario: Each user's child goes to the innermost item, inset by every level
    When the method "look" of "root" is called
    Then the property "seen" of "root" is "padded<inner<card@14 padded<inner<card@114 padded<inner<card@214 top<padded<inner<card@315 top<padded<inner<card@15"
