# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-propertychanges.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.PropertyChanges.target
Feature: A signal handler of a state, named through an id
  A PropertyChanges may give an object, named by its id, a handler for
  one of its signals: `id.onClicked: ...`. The handler runs while the
  state lasts, in place of the one before, and is gone when it is left.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        QtObject { id: cfg; objectName: "cfg"; property bool checked: false; signal clicked() }
        states: [
          State { name: "a"; PropertyChanges { cfg.checked: true; cfg.onClicked: root.log += "a " } },
          State { name: "b"; PropertyChanges { cfg.onClicked: root.log += "b " } }
        ]
      }
      """

  Scenario: The handler of the state runs
    When the property "state" of "root" is set to "a"
    And the method "clicked" of "cfg" is called
    Then the property "log" of "root" is "a "
    And the property "checked" of "cfg" is true
    And the log has no warning matching "non-existent"

  Scenario: The next state's handler takes its place
    When the property "state" of "root" is set to "a"
    And the property "state" of "root" is set to "b"
    And the method "clicked" of "cfg" is called
    Then the property "log" of "root" is "b "
    And the property "checked" of "cfg" is false

  Scenario: Leaving the state takes the handler away
    When the property "state" of "root" is set to "a"
    And the property "state" of "root" is set to ""
    And the method "clicked" of "cfg" is called
    Then the property "log" of "root" is ""
