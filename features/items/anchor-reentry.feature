# Sources:
#   https://doc.qt.io/qt-6/qtquick-positioning-anchors.html
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#anchors.centerIn-prop
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (geometry), bucket warning:binding-loop(ljos-only):width.

@qt-parity
Feature: Anchor re-entry

  Scenario: Centring does not reevaluate a width binding against intermediate geometry
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        height: 200
        Rectangle {
          id: box
          objectName: "box"
          anchors.centerIn: parent
          width: Object.keys(Object.assign({}, ({}), {k2: box.y})).length
          onXChanged: { box.height = parseInt(""); }
        }
      }
      """
    When the property "height" of "root" is set to 0
    Then the log has no warning matching "(?i)binding loop"
