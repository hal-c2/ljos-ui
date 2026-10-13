# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-draghandler.html
#   https://doc.qt.io/qt-6/qml-qtquick-pointerhandler.html
#   https://doc.qt.io/qt-6/qtquickhandlers-index.html
# Verified on Qt 6.11.2 (qml6, qmltestrunner).
# A drag starts once the pointer is more than Qt.styleHints.startDragDistance
# (8 pixels) from the press; drag-threshold.feature pins it to the pixel.
# Every move here goes 20 or more.

@qt-parity
Feature: Dragging with a DragHandler
  A DragHandler moves its target as the pointer drags it. It takes the
  pointer once the drag has gone far enough, tells how far it went, and
  lets go at the release.

  Background:
    Given the file "Rail.qml":
      """qml
      import QtQuick
      Rectangle {
        id: rail
        objectName: "rail"
        width: 100; height: 100
        signal said(string s)
        DragHandler {
          objectName: "railHandler"
          xAxis.enabled: false
          yAxis.maximum: 20
          onActiveChanged: rail.said("rail=" + active + " ")
        }
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 400; height: 400
        property string log: ""
        Rectangle {
          id: box
          objectName: "box"
          width: 100; height: 100
          DragHandler {
            id: handler
            objectName: "handler"
            onActiveChanged: root.log += "active=" + active + " "
            onGrabChanged: (transition, point) => root.log += "grab=" + transition + " "
            onTranslationChanged: (delta) => root.log += "t=" + translation.x + "," + translation.y + " "
          }
        }
        Rail { x: 250; onSaid: (s) => root.log += s }
      }
      """

  Rule: The handler takes the pointer when the drag has gone far enough

    @api-QtQuick.PointerHandler.grabChanged
    Scenario: A press is only watched
      When the user presses the pointer at 50, 50
      Then the property "log" of "root" is "grab=1 "
      And the property "active" of "handler" is false

    @api-QtQuick.DragHandler.active
    Scenario: A short move does not start a drag
      When the user presses the pointer at 50, 50
      And the user moves the pointer to 50, 55
      Then the property "active" of "handler" is false
      And the property "y" of "box" is 0

    @api-QtQuick.DragHandler.active @api-QtQuick.PointerHandler.target
    Scenario: A long move takes the pointer and moves the target
      When the user presses the pointer at 50, 50
      And the user moves the pointer to 50, 80
      Then the property "log" of "root" is "grab=1 grab=16 active=true "
      And the property "y" of "box" is 30

    @api-QtQuick.DragHandler.translation @api-QtQuick.DragHandler.translationChanged
    Scenario: Each later move tells how far the drag went
      When the user presses the pointer at 50, 50
      And the user moves the pointer to 50, 80
      And the property "log" of "root" is set to ""
      And the user moves the pointer to 70, 110
      Then the property "log" of "root" is "t=20,60 "
      And the property "x" of "box" is 20
      And the property "y" of "box" is 60

    @api-QtQuick.DragHandler.active
    Scenario: The release lets go and leaves the target where it is
      When the user presses the pointer at 50, 50
      And the user moves the pointer to 50, 80
      And the user releases the pointer at 50, 80
      Then the property "log" of "root" is "grab=1 grab=16 active=true active=false grab=32 grab=2 "
      And the property "y" of "box" is 30
      And the property "active" of "handler" is false

  Rule: How far it went

    @api-QtQuick.DragHandler.translation @api-QtQuick.DragHandler.activeTranslation
    Scenario: The translation of a drag is zero again after it
      When the user drags from 50, 50 to 50, 90
      Then the property "translation.y" of "handler" is 0
      And the property "activeTranslation.y" of "handler" is 0

    @api-QtQuick.DragHandler.persistentTranslation
    Scenario: The persistent translation adds the drags up
      When the user drags from 50, 50 to 50, 90
      And the user drags from 50, 50 to 80, 50
      Then the property "persistentTranslation.x" of "handler" is 30
      And the property "persistentTranslation.y" of "handler" is 40

  Rule: The axes limit the drag

    @api-QtQuick.DragHandler.xAxis
    Scenario: A drag along an axis that is off does not start
      When the user drags from 300, 50 to 340, 50
      Then the property "log" of "root" is ""
      And the property "x" of "rail" is 250

    @api-QtQuick.DragHandler.yAxis
    Scenario: The target stops at the axis's maximum
      When the user presses the pointer at 300, 50
      And the user moves the pointer to 300, 80
      And the user moves the pointer to 300, 110
      Then the property "y" of "rail" is 20
      And the property "active" of "railHandler" is true
      And the property "translation.y" of "railHandler" is 60
      And the property "persistentTranslation.y" of "railHandler" is 20

  Rule: A target that is not the handler's item

    @api-QtQuick.PointerHandler.target
    Scenario: A handler without a target drags nothing
      When the property "target" of "handler" is set to null
      And the user presses the pointer at 50, 50
      And the user moves the pointer to 50, 80
      And the user moves the pointer to 50, 90
      And the user releases the pointer at 50, 90
      Then the property "log" of "root" is "grab=1 grab=16 active=true t=0,40 active=false grab=32 grab=2 "
      And the property "y" of "box" is 0
