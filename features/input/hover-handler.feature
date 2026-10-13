# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-hoverhandler.html
#   https://doc.qt.io/qt-6/qml-qtquick-handlerpoint.html
# Verified on Qt 6.11.2 (qml6).

@qt-parity
Feature: Hovering with a HoverHandler
  A HoverHandler tells whether the pointer is over its item, and where,
  without a button and without taking the pointer from anything.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        width: 400; height: 400
        property real px: handler.point.position.x
        property real py: handler.point.position.y
        Rectangle {
          x: 50; y: 40; width: 100; height: 100
          HoverHandler { id: handler; objectName: "handler" }
          MouseArea { objectName: "area"; anchors.fill: parent }
        }
        Rectangle {
          x: 250; y: 40; width: 100; height: 100
          HoverHandler { objectName: "below" }
          Rectangle {
            width: 100; height: 100
            HoverHandler { objectName: "top" }
          }
        }
      }
      """

  Rule: The pointer over the item

    @api-QtQuick.HoverHandler.hovered
    Scenario: A handler is not hovered before the pointer comes
      Then the property "hovered" of "handler" is false

    @api-QtQuick.HoverHandler.hovered @api-QtQuick.HoverHandler.point
    Scenario: The pointer over the item hovers the handler
      When the user moves the pointer to 60, 70
      Then the property "hovered" of "handler" is true
      And the property "px" of "root" is 10
      And the property "py" of "root" is 30

    @api-QtQuick.HoverHandler.hovered
    Scenario: The pointer that leaves the item ends the hover
      When the user moves the pointer to 60, 70
      And the user moves the pointer to 200, 70
      Then the property "hovered" of "handler" is false

    @api-QtQuick.HoverHandler.hovered
    Scenario: A MouseArea over the handler's item does not hide it
      When the user moves the pointer to 60, 70
      And the user presses the pointer at 60, 70
      Then the property "hovered" of "handler" is true
      And the property "pressed" of "area" is true

  Rule: Handlers that do not block

    @api-QtQuick.HoverHandler.blocking
    Scenario: The handlers of an item and of the item over it are hovered together
      When the user moves the pointer to 260, 70
      Then the property "hovered" of "top" is true
      And the property "hovered" of "below" is true
