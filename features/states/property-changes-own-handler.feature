# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-propertychanges.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.PropertyChanges.target
Feature: A state's handler in place of the object's own
  An object's own handler for a signal does not run while a state gives
  it another: the state's runs alone, and the object's own runs again
  once the state is left.

  Scenario: The own handler steps aside for the state's
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        QtObject {
          id: cfg; objectName: "cfg"
          signal clicked()
          onClicked: root.log += "own "
        }
        states: State { name: "a"; PropertyChanges { cfg.onClicked: root.log += "a " } }
      }
      """
    When the method "clicked" of "cfg" is called
    And the property "state" of "root" is set to "a"
    And the method "clicked" of "cfg" is called
    And the property "state" of "root" is set to ""
    And the method "clicked" of "cfg" is called
    Then the property "log" of "root" is "own a own "
