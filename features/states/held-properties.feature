# Sources:
#   https://doc.qt.io/qt-6/qtquick-statesanimations-states.html
#   https://doc.qt.io/qt-6/qml-qtquick-propertychanges.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#binding-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A property while a state holds it
  A state's value lies over what the property had. The binding it had stays
  alive underneath and is what the property goes back to.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 200; height: 200
        property int base: 10
        property int n: 40
        Rectangle { id: box; objectName: "box"; width: root.base; height: 10 }
        states: [
          State { name: "wide"
            PropertyChanges { target: box; width: 100; height: root.n } }
        ]
      }
      """

  Scenario: A change of what the property was bound to does not show while the state lasts
    When the property "state" of "root" is set to "wide"
    And the property "base" of "root" is set to 30
    Then the property "width" of "box" is 100

  Scenario: The property was bound all along and shows the new value when the state is left
    When the property "state" of "root" is set to "wide"
    And the property "base" of "root" is set to 30
    And the property "state" of "root" is set to ""
    Then the property "width" of "box" is 30

  Scenario: The binding the property had follows what it reads again after the state
    When the property "state" of "root" is set to "wide"
    And the property "state" of "root" is set to ""
    And the property "base" of "root" is set to 50
    Then the property "width" of "box" is 50

  Scenario: A value written while the state lasts shows
    When the property "state" of "root" is set to "wide"
    And the property "width" of "box" is set to 55
    Then the property "width" of "box" is 55

  Scenario: A value written while the state lasts is gone when the state is left
    When the property "state" of "root" is set to "wide"
    And the property "width" of "box" is set to 55
    And the property "state" of "root" is set to ""
    Then the property "width" of "box" is 10

  Scenario: A value written over a state's binding ends that binding
    When the property "state" of "root" is set to "wide"
    And the property "height" of "box" is set to 55
    And the property "n" of "root" is set to 60
    Then the property "height" of "box" is 55

  Scenario: A binding installed while the state lasts follows what it reads
    When the property "state" of "root" is set to "wide"
    And the property "width" of "box" is bound to "height * 2"
    And the property "n" of "root" is set to 30
    Then the property "width" of "box" is 60

  Scenario: A binding installed while the state lasts is gone when the state is left
    When the property "state" of "root" is set to "wide"
    And the property "width" of "box" is bound to "height * 2"
    And the property "state" of "root" is set to ""
    Then the property "width" of "box" is 10

  Scenario: Entering the state again applies its values again
    When the property "state" of "root" is set to "wide"
    And the property "width" of "box" is set to 55
    And the property "state" of "root" is set to ""
    And the property "state" of "root" is set to "wide"
    Then the property "width" of "box" is 100
