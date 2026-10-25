# Sources:
#   https://doc.qt.io/qt-6/qtqml-typesystem-valuetypes.html
#   https://doc.qt.io/qt-6/qml-color.html
#   https://doc.qt.io/qt-6/qml-point.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (creation), buckets value:load-handlers:empty-qt,
# warning:assign(qt-only):Unable_to_assign_QPointF_to_QString,
# warning:assign(ljos-only):Unable_to_assign_double_to_color, …_bool_to_color, …_QJSValue_to_color
# and …_QColor_to_point; ljos-fuzz (bindings), …_QPointF_to_color.

@qt-parity
Feature: A binding to a value of another type
  A colour bound to what is no colour is an invalid colour, and a point bound to a colour is
  the origin, both without a word. A string bound to a point or a rectangle is told, and
  stays empty.

  @backlog
  Scenario: A colour or a point bound to another type is converted without a warning
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property color fromNumber: Math.abs(3)
        property color fromBool: Number.isInteger(1)
        property color fromArray: [].concat([])
        property color fromPoint: Qt.point(1, 2)
        property point fromColor: Qt.lighter("#000000")
        property string seen: [fromNumber.valid, fromBool.valid, fromArray.valid, fromPoint.valid, fromColor.x, fromColor.y].join(",")
      }
      """
    Then the property "seen" of "root" is "false,false,false,false,0,0"
    And the log has no warning matching "Unable to assign"

  @backlog
  Scenario Outline: A string bound to a <type> is told, and stays empty
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string s: <value>
      }
      """
    Then the property "s" of "root" is ""
    And the log has a warning matching "Unable to assign <qt> to QString"

    Examples:
      | type      | value              | qt     |
      | point     | Qt.point(0, 0)     | QPointF |
      | rectangle | Qt.rect(0, 0, 1, 1) | QRectF |
