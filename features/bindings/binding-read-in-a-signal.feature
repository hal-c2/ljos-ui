# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#released-signal
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
# Verified on Qt 6.11.2 (qml6, qmltestrunner), whose log is
# "pc true held false|released false held true active true|dropped|pc false held true|".

Feature: A binding read in a signal sent before the change it depends on
  A MouseArea sends released before pressedChanged, and its pressed is
  already false. Qt brings a binding on pressed up to date after the
  handlers of the change have run, so they, and onReleased before them,
  read it as it was. Ljós works a binding out when it is read, so every
  handler reads it new. A Drag.active bound to pressed and read in
  onReleased ends the drag there, and the drop() after it lands nowhere.
  docs/ARCHITECTURE.md (6.2) says where else this shows.

  # A binding is worked out when it is read, against what it depends on
  # now; nothing in Ljós holds the value from before the change.
  @qt-diverges
  Scenario: onReleased reads a binding on pressed as it is after the release
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 300; height: 300
        property string log: ""
        DropArea { anchors.fill: parent; onDropped: root.log += "dropped|" }
        Rectangle {
          width: 100; height: 100
          property bool held: area.pressed
          Drag.active: area.pressed
          Drag.hotSpot.x: 50; Drag.hotSpot.y: 50
          MouseArea {
            id: area
            anchors.fill: parent
            onReleased: {
              root.log += "released " + pressed + " held " + parent.held
                        + " active " + parent.Drag.active + "|"
              parent.Drag.drop()
            }
            onPressedChanged: root.log += "pc " + pressed + " held " + parent.held + "|"
          }
        }
      }
      """
    When the user presses the pointer at 50, 50
    And the user moves the pointer to 60, 60
    And the user releases the pointer at 60, 60
    Then the property "log" of "root" is "pc true held true|released false held false active false|pc false held false|"
