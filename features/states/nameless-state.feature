# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-state.html
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#state-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.State.name @api-QtQuick.State.when
Feature: A state without a name
  A state may be declared without a name and entered by its `when`
  alone. It is given a name of its own, `anonymousState` and a number
  counted over the nameless states of its item, so that `state` has one
  to hold while it is in force.

  Scenario: Each item counts its nameless states
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Item { id: a; states: [State { when: false }, State { when: true }, State { name: "x" }, State {}] }
        Item { id: b; states: [State { when: true }] }
        Item { id: c; states: [State {}] }
        property var names: []
        property var held: []
        Component.onCompleted: {
          names = a.states.map(s => s.name).concat(b.states[0].name, c.states[0].name)
          held = [a.state, b.state, c.state]
        }
      }
      """
    Then the property "names" of "root" is ["anonymousState1", "anonymousState2", "x", "anonymousState3", "anonymousState1", "anonymousState1"]
    And the property "held" of "root" is ["anonymousState2", "anonymousState1", ""]

  Scenario: A nameless state its when picks where the item is made is in force
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        property bool on: true
        QtObject { id: cfg; objectName: "cfg"; property bool checked: false; signal clicked() }
        states: [
          State { when: root.on; PropertyChanges { cfg.checked: true; cfg.onClicked: root.log += "on " } },
          State { when: !root.on; PropertyChanges { cfg.onClicked: root.log += "off " } }
        ]
      }
      """
    Then the property "checked" of "cfg" is true
    When the method "clicked" of "cfg" is called
    And the property "on" of "root" is set to false
    And the method "clicked" of "cfg" is called
    Then the property "log" of "root" is "on off "
    And the property "checked" of "cfg" is false
    And the property "state" of "root" is "anonymousState2"

  Scenario: A nameless state is entered by the name it was given
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property int p: 0
        states: [State { PropertyChanges { root.p: 1 } }]
      }
      """
    When the property "state" of "root" is set to "anonymousState1"
    Then the property "p" of "root" is 1
