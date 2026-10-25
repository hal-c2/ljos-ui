# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#read-only-properties
#   https://doc.qt.io/qt-6/qtqml-typesystem-valuetypes.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (bindings), bucket throws:none/TypeError:step-member-write-readonly.

@qt-parity
Feature: A member of a read-only value-type property
  A script that writes a member of a `readonly` value-type property is not thrown an error, and
  the property keeps its value.

  @backlog
  Scenario: Writing a member of a read-only size is ignored
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        readonly property size s: Qt.size(2, 3)
        property string seen: ""
        function write() {
          try { s.height = 9; seen = "" + s.height } catch (e) { seen = e.name }
        }
      }
      """
    When the method "write" of "root" is called
    Then the property "seen" of "root" is "3"
