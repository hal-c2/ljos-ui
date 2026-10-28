# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#visible-prop
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Bindings installed while an inheritance cover is active

  Scenario: A binding installed under a hidden parent stays hidden and survives the cover
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property bool wanted: true
        function install() {
          child.visible = Qt.binding(function() { return root.wanted })
        }
        Item {
          objectName: "parent"
          visible: false
          Item { id: child; objectName: "child"; visible: false }
        }
      }
      """
    When the method "install" of "root" is called
    Then the property "visible" of "child" is false
    And the log has no warnings
    When the property "visible" of "parent" is set to true
    Then the property "visible" of "child" is true
    When the property "wanted" of "root" is set to false
    Then the property "visible" of "child" is false
    And the log has no warnings
