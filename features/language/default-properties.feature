# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#default-properties
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#property-aliases
#   https://doc.qt.io/qt-6/qtqml-documents-definetypes.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Default properties of components
  A child object written without a property name goes to a default property. Which one depends on
  where it is written: the children a component writes in its own definition go to the default
  property of the type it extends, and only the children its users write go to the default
  property the component declares, so a component can forward its users' children into one of its
  own items with `default property alias`.

  Rule: A component's own children go to the default property of the type it extends

    Scenario: A card forwards its users' children into its body and keeps the body itself
      Given the file "Card.qml":
        """qml
        import QtQuick
        Rectangle {
          default property alias content: body.data
          color: "red"
          Item { id: body; anchors.fill: parent; anchors.margins: 20 }
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          width: 200; height: 200
          Card {
            anchors.fill: parent
            Rectangle { anchors.fill: parent; color: "blue" }
          }
        }
        """
      Then the pixel at 10, 10 of the window is "#ff0000"
      And the pixel at 100, 100 of the window is "#0000ff"
      And the pixel at 185, 185 of the window is "#ff0000"
      And the log has no warnings

    Scenario Outline: The component's own child is its child, the user's is in the alias
      Given the file "<file>":
        """qml
        import QtQuick
        Rectangle {
          <declaration>
          Item { id: body; objectName: "body" }
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string own
          property string given
          function names(list) { return Array.from(list).map(o => o.objectName).join(",") }
          <type> { id: card; Item { objectName: "user" } }
          Component.onCompleted: { own = names(card.children); given = names(card.content) }
        }
        """
      Then the property "own" of "root" is "<own>"
      And the property "given" of "root" is "user"

      Examples:
        | file      | type  | declaration                                | own  |
        | Card.qml  | Card  | default property alias content: body.data  | body |
        | Plain.qml | Plain | default property list<QtObject> content    | body |

    Scenario: An inline component keeps its own children the same way
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string own
          property string given
          function names(list) { return Array.from(list).map(o => o.objectName).join(",") }
          component Card: Rectangle {
            default property alias content: body.data
            Item { id: body; objectName: "body" }
          }
          Card { id: card; Item { objectName: "user" } }
          Component.onCompleted: { own = names(card.children); given = names(card.content) }
        }
        """
      Then the property "own" of "root" is "body"
      And the property "given" of "root" is "user"

  Rule: A component that extends another is one of its users

    Scenario: The children of a component made from the card go through the card's alias
      Given the file "Card.qml":
        """qml
        import QtQuick
        Rectangle {
          default property alias content: body.data
          Item { id: body; objectName: "body" }
        }
        """
      And the file "Tile.qml":
        """qml
        import QtQuick
        Card { Rectangle { objectName: "tiled" } }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string own
          property string given
          function names(list) { return Array.from(list).map(o => o.objectName).join(",") }
          Tile { id: tile }
          Component.onCompleted: { own = names(tile.children); given = names(tile.content) }
        }
        """
      Then the property "own" of "root" is "body"
      And the property "given" of "root" is "tiled"

  Rule: The alias may name an item deeper inside the component

    Scenario: The users' children are the deeper item's children, in order
      Given the file "Frame.qml":
        """qml
        import QtQuick
        Rectangle {
          default property alias content: inner.data
          Item {
            objectName: "outer"
            Item { id: inner; objectName: "inner" }
          }
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string own
          property string inside
          property string given
          property string parentName
          function names(list) { return Array.from(list).map(o => o.objectName).join(",") }
          Frame { id: frame; Item { objectName: "a" } Item { objectName: "b" } }
          Component.onCompleted: {
            own = names(frame.children)
            inside = names(frame.children[0].children[0].children)
            given = names(frame.content)
            parentName = frame.content[1].parent.objectName
          }
        }
        """
      Then the property "own" of "root" is "outer"
      And the property "inside" of "root" is "a,b"
      And the property "given" of "root" is "a,b"
      And the property "parentName" of "root" is "inner"
