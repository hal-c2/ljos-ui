# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-pointhandler.html
#   https://doc.qt.io/qt-6/qml-qtquick-handlerpoint.html
# Verified on Qt 6.11.2 (qml6, qmltestrunner).

@qt-parity
Feature: Following a pointer with a PointHandler
  A PointHandler follows the pointer that presses its item without taking
  it: what is under it still gets the press.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 400; height: 400
        property string log: ""
        property real pressX: handler.point.pressPosition.x
        property real sceneY: handler.point.scenePosition.y
        Rectangle {
          x: 50; y: 40; width: 100; height: 100
          PointHandler {
            id: handler
            objectName: "handler"
            onActiveChanged: root.log += "active=" + active + " "
            onGrabChanged: (transition, point) => root.log += "grab=" + transition + " "
            onPointChanged: root.log += "p=" + point.position.x + "," + point.position.y + " "
            onTranslationChanged: root.log += "t=" + translation.x + "," + translation.y + " "
          }
          TapHandler {
            onTapped: root.log += "tapped "
          }
        }
      }
      """

  Rule: While the pointer is pressed

    @api-QtQuick.PointHandler.active @api-QtQuick.PointHandler.point
    Scenario: A press makes the handler active with the point in its item
      When the user presses the pointer at 60, 50
      Then the property "log" of "root" is "grab=1 active=true t=0,0 p=10,10 "

    @api-QtQuick.PointHandler.translation
    Scenario: A move tells how far the pointer went, then where it is
      When the user presses the pointer at 60, 50
      And the property "log" of "root" is set to ""
      And the user moves the pointer to 62, 53
      Then the property "log" of "root" is "t=2,3 p=12,13 "

    @api-QtQuick.HandlerPoint.pressPosition
    Scenario: The point remembers where the press was
      When the user presses the pointer at 60, 50
      And the user moves the pointer to 62, 53
      Then the property "pressX" of "root" is 10
      And the property "sceneY" of "root" is 53

  Rule: When the press is over

    @api-QtQuick.PointHandler.active
    Scenario: The release empties the point and keeps the translation
      When the user presses the pointer at 60, 50
      And the user moves the pointer to 62, 53
      And the property "log" of "root" is set to ""
      And the user releases the pointer at 62, 53
      Then the property "log" of "root" is "tapped active=false t=2,3 p=0,0 grab=2 "
      And the property "point.id" of "handler" is -1

    @api-QtQuick.PointHandler.active
    Scenario: The handler does not keep a tap from its item
      When the user presses the pointer at 60, 50
      And the user releases the pointer at 60, 50
      Then the property "log" of "root" is "grab=1 active=true t=0,0 p=10,10 tapped active=false t=0,0 p=0,0 grab=2 "
