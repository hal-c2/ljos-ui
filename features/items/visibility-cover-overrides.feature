# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-propertychanges.html
#   https://doc.qt.io/qt-6/qml-qtquick-state.html
#   https://doc.qt.io/qt-6/qml-qtqml-binding.html#when-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#visible-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#enabled-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: State and Binding overrides under hidden or disabled parents

  Scenario: Entering and leaving a state that sets visible true on a child under a hidden parent leaves the child hidden when parent shows
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property int sigs: 0
        Item {
          id: box; objectName: "box"; visible: false
          Item {
            id: child; objectName: "child"; visible: false
            onVisibleChanged: root.sigs++
          }
        }
        states: State {
          name: "shown"
          PropertyChanges { target: child; visible: true }
        }
      }
      """
    When the property "state" of "root" is set to "shown"
    Then the property "visible" of "child" is false
    And the property "sigs" of "root" is 0
    When the property "state" of "root" is set to ""
    And the property "visible" of "box" is set to true
    Then the property "visible" of "child" is false
    And the property "sigs" of "root" is 0

  Scenario: Showing the parent while the state is active reveals the child and leaving the state hides it again
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property int sigs: 0
        Item {
          id: box; objectName: "box"; visible: false
          Item {
            id: child; objectName: "child"; visible: false
            onVisibleChanged: root.sigs++
          }
        }
        states: State {
          name: "shown"
          PropertyChanges { target: child; visible: true }
        }
      }
      """
    When the property "state" of "root" is set to "shown"
    And the property "visible" of "box" is set to true
    Then the property "visible" of "child" is true
    And the property "sigs" of "root" is 1
    When the property "state" of "root" is set to ""
    Then the property "visible" of "child" is false
    And the property "sigs" of "root" is 2

  Scenario: Activating a Binding with when while the parent is hidden keeps effective visible false until the parent is shown
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property bool forceShow: false
        property int sigs: 0
        Item {
          id: box; objectName: "box"; visible: false
          Item {
            id: child; objectName: "child"; visible: false
            onVisibleChanged: root.sigs++
          }
        }
        Binding {
          target: child; property: "visible"
          value: true; when: root.forceShow
        }
      }
      """
    When the property "forceShow" of "root" is set to true
    Then the property "visible" of "child" is false
    And the property "sigs" of "root" is 0
    When the property "visible" of "box" is set to true
    Then the property "visible" of "child" is true
    And the property "sigs" of "root" is 1
    When the property "forceShow" of "root" is set to false
    Then the property "visible" of "child" is false
    And the property "sigs" of "root" is 2

  Scenario: Toggling a Binding on enabled while the parent is disabled emits no enabledChanged until the parent is enabled
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property bool forceEnable: false
        property int sigs: 0
        Item {
          id: box; objectName: "box"; visible: false; enabled: false
          Item {
            id: child; objectName: "child"; enabled: false
            onEnabledChanged: root.sigs++
          }
        }
        Binding {
          target: child; property: "enabled"
          value: true; when: root.forceEnable
        }
      }
      """
    When the property "forceEnable" of "root" is set to true
    Then the property "enabled" of "child" is false
    And the property "sigs" of "root" is 0
    When the property "enabled" of "box" is set to true
    Then the property "enabled" of "child" is true
    And the property "sigs" of "root" is 1
    When the property "forceEnable" of "root" is set to false
    Then the property "enabled" of "child" is false
    And the property "sigs" of "root" is 2

  Scenario: A write made while a State shows a child under a hidden parent leaves with the State
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property int sigs: 0
        Item {
          id: box; objectName: "box"; visible: false
          Item {
            id: child; objectName: "child"; visible: false
            onVisibleChanged: root.sigs++
          }
        }
        states: State { name: "on"; PropertyChanges { target: child; visible: true } }
      }
      """
    When the property "state" of "root" is set to "on"
    And the property "visible" of "child" is set to true
    And the property "state" of "root" is set to ""
    And the property "visible" of "box" is set to true
    Then the property "visible" of "child" is false
    And the property "sigs" of "root" is 0

  Scenario: A write made while a State shows a child leaves with the State
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property int sigs: 0
        Item {
          id: box; objectName: "box"
          Item {
            id: child; objectName: "child"; visible: false
            onVisibleChanged: root.sigs++
          }
        }
        states: State { name: "on"; PropertyChanges { target: child; visible: true } }
      }
      """
    When the property "state" of "root" is set to "on"
    And the property "visible" of "child" is set to true
    And the property "state" of "root" is set to ""
    Then the property "visible" of "child" is false
    And the property "sigs" of "root" is 2
