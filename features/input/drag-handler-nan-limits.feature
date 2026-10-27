# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-draghandler.html
#   https://doc.qt.io/qt-6/qml-qtquick-draganaxis.html
# Verified on Qt 6.11.2 (qml6).

@qt-parity
Feature: A DragHandler axis whose limit is not a number
  An author can set an axis's minimum or maximum to NaN. The drag still
  runs: a NaN maximum does not limit it, and a NaN minimum leaves the
  target where it is along that axis.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 400; height: 400
        property bool persistentIsNaN: isNaN(handler.persistentTranslation.x)
        Rectangle {
          id: box
          objectName: "box"
          x: 100; y: 100
          width: 100; height: 100
          DragHandler {
            id: handler
            objectName: "handler"
            xAxis.minimum: NaN
            yAxis.maximum: NaN
          }
        }
      }
      """

  Rule: A NaN minimum leaves the target where it is along that axis

    @api-QtQuick.DragAxis.minimum
    Scenario: A drag left does not move the target along that axis
      When the user presses the pointer at 150, 150
      And the user moves the pointer to 120, 150
      And the user moves the pointer to 90, 150
      Then the property "x" of "box" is 100
      And the property "active" of "handler" is true

    @api-QtQuick.DragAxis.minimum @api-QtQuick.DragHandler.persistentTranslation
    Scenario: The drag still runs, and the other axis still moves
      When the user presses the pointer at 150, 150
      And the user moves the pointer to 180, 150
      And the user moves the pointer to 210, 170
      Then the property "x" of "box" is 100
      And the property "y" of "box" is 120
      And the property "active" of "handler" is true
      And the property "translation.x" of "handler" is 60
      And the property "persistentIsNaN" of "root" is true
      And the property "persistentTranslation.y" of "handler" is 20

  Rule: A NaN maximum is no limit

    @api-QtQuick.DragAxis.maximum
    Scenario: A drag down along an axis whose maximum is NaN
      When the user presses the pointer at 150, 150
      And the user moves the pointer to 150, 180
      And the user moves the pointer to 150, 210
      Then the property "y" of "box" is 160
