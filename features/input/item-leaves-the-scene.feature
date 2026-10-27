# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#canceled-signal
#   https://doc.qt.io/qt-6/qml-qtquick-taphandler.html
# Verified on Qt 6.11.2 (qml6).

@qt-parity
Feature: An item taken out of the scene while it holds the pointer
  A pressed MouseArea that leaves the scene loses the pointer and says it
  was canceled; it hears nothing of the release. A TapHandler whose item
  leaves keeps its press, and the release taps.

  Rule: A MouseArea

    @api-QtQuick.MouseArea.canceled
    Scenario: A pressed MouseArea that leaves the scene is canceled
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string log: ""
          property bool areaPressed: area.pressed
          MouseArea {
            id: area
            width: 100; height: 100
            onPressed: { root.log += "pressed "; area.parent = null }
            onReleased: root.log += "released "
            onClicked: root.log += "clicked "
            onCanceled: root.log += "canceled "
          }
        }
        """
      When the user presses the pointer at 50, 50
      And the user releases the pointer at 50, 50
      Then the property "log" of "root" is "pressed canceled "
      And the property "areaPressed" of "root" is false

  Rule: A TapHandler

    @api-QtQuick.TapHandler.tapped
    Scenario: A TapHandler whose item leaves the scene still taps
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string log: ""
          Rectangle {
            id: tapped
            width: 100; height: 100
            TapHandler {
              gesturePolicy: TapHandler.WithinBounds
              onPressedChanged: {
                root.log += "pressed=" + pressed + " "
                if (pressed) tapped.parent = null
              }
              onTapped: root.log += "tapped "
              onCanceled: root.log += "canceled "
            }
          }
        }
        """
      When the user presses the pointer at 50, 50
      And the user releases the pointer at 50, 50
      Then the property "log" of "root" is "pressed=true tapped pressed=false "
