# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-hoverhandler.html
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-tooltip.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A press or release elsewhere moves the pointer there first
  A button pressed or let go where the pointer was not is told as the
  pointer moving there first, then the button. What the pointer left stops
  being hovered before the press lands, so a tool tip that showed for it is
  gone and does not take the press.

  @api-QtQuick.HoverHandler.hovered
  Scenario: A press on another item ends the hover first
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        width: 400; height: 200
        property string log: ""
        Rectangle {
          x: 20; y: 20; width: 100; height: 100
          HoverHandler { onHoveredChanged: root.log += "hovered:" + hovered + " " }
        }
        Rectangle {
          x: 200; y: 20; width: 100; height: 100
          MouseArea { anchors.fill: parent; onPressed: root.log += "pressed " }
        }
      }
      """
    When the user moves the pointer to 60, 60
    And the user presses the pointer at 250, 60
    Then the property "log" of "root" is "hovered:true hovered:false pressed "

  @api-QtQuick.MouseArea.containsMouse @api-QtQuick.MouseArea.exited
  Scenario: A press on another item ends a MouseArea's hover first
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        width: 400; height: 200
        property string log: ""
        MouseArea {
          x: 20; y: 20; width: 100; height: 100; hoverEnabled: true
          onEntered: root.log += "entered "
          onExited: root.log += "exited:" + containsMouse + " "
        }
        MouseArea {
          x: 200; y: 20; width: 100; height: 100
          onPressed: root.log += "pressed "
        }
      }
      """
    When the user moves the pointer to 60, 60
    And the user presses the pointer at 250, 60
    Then the property "log" of "root" is "entered exited:false pressed "

  @api-QtQuick.Controls.ToolTip.visible
  Scenario: A tool tip shown for the hover is gone before the press under it
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 200
        Rectangle {
          x: 20; y: 20; width: 100; height: 100
          HoverHandler { id: hover }
          ToolTip { objectName: "tip"; visible: hover.hovered; delay: 0
                    enter: null; exit: null; text: "A"
                    x: 180; y: 0; width: 100; height: 100 }
        }
        Rectangle {
          x: 200; y: 20; width: 100; height: 100
          MouseArea { objectName: "b"; anchors.fill: parent }
        }
      }
      """
    When the user moves the pointer to 60, 60
    Then the property "opened" of "tip" is true
    When the user presses the pointer at 250, 60
    Then the property "pressed" of "b" is true
    And the property "opened" of "tip" is false

  @api-QtQuick.MouseArea.released
  Scenario: A release elsewhere moves the held area's pointer first
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        width: 400; height: 200
        property string log: ""
        MouseArea {
          x: 20; y: 20; width: 100; height: 100
          onExited: root.log += "exited "
          onReleased: root.log += "released:" + containsMouse + " "
        }
      }
      """
    When the user presses the pointer at 60, 60
    And the user releases the pointer at 250, 60
    Then the property "log" of "root" is "exited released:false "

  Rule: The move comes before the button wherever it lands

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          width: 400; height: 200
          property string log: ""
          MouseArea {
            x: 20; y: 20; width: 100; height: 100; hoverEnabled: true
            onEntered: root.log += "entered "
            onExited: root.log += "exited "
            onPositionChanged: (mouse) => root.log += "pos:" + mouse.x + " "
            onPressed: (mouse) => root.log += "pressed:" + mouse.x + " "
            onReleased: (mouse) => root.log += "released:" + mouse.x + " "
          }
        }
        """

    @api-QtQuick.MouseArea.positionChanged @api-QtQuick.MouseArea.entered
    Scenario: The first press moves the pointer in first
      When the user presses the pointer at 60, 60
      Then the property "log" of "root" is "entered pos:40 pressed:40 "

    @api-QtQuick.MouseArea.positionChanged
    Scenario: A press and a release at new places inside the area each move there first
      When the user moves the pointer to 60, 60
      And the user presses the pointer at 70, 60
      And the user releases the pointer at 80, 60
      Then the property "log" of "root" is "entered pos:40 pos:50 pressed:50 pos:60 released:60 "

    @api-QtQuick.MouseArea.positionChanged
    Scenario: A press where the pointer is moves nothing
      When the user moves the pointer to 60, 60
      And the user presses the pointer at 60, 60
      And the user releases the pointer at 60, 60
      Then the property "log" of "root" is "entered pos:40 pressed:40 released:40 "

    @api-QtQuick.MouseArea.positionChanged
    Scenario: A finger's press is no move
      When the user moves the pointer to 300, 60
      And the user taps 60, 60 with a finger
      Then the property "log" of "root" is "entered pressed:40 released:40 exited "
