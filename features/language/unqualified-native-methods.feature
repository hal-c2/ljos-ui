# Sources:
#   https://doc.qt.io/qt-6/qtqml-documents-scope.html
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#mapFromItem-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A type's own methods called without qualifying them
  Inside an object, its own methods, the ones its type gives as well as
  the ones its QML declares, are in scope: a function or handler there
  calls them by name, as it reads its own properties.

  Scenario: A root's function calls a method its type gives
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"; width: 100; height: 100
        property real got: -1
        Item { id: kid; x: 7; y: 3 }
        function probe() { got = mapFromItem(kid, 1, 1).x }
      }
      """
    When the method "probe" of "root" is called
    Then the property "got" of "root" is 8

  Scenario: A nested object's completion handler calls a method its type gives
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"; width: 100; height: 100
        property real got: -1
        Item {
          Item { id: kid; x: 7; y: 3 }
          function probe() { got = mapFromItem(kid, 1, 1).x }
          Component.onCompleted: probe()
        }
      }
      """
    Then the property "got" of "root" is 8
