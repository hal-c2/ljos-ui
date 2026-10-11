# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#font.pixelSize-prop
#   https://doc.qt.io/qt-6/qml-font.html
#   docs/ARCHITECTURE.md (sections 7.5 and 14, SystemPrefs)
# Qt has no text size setting of its own: on the systems that have one it is
# part of the scale of the whole screen. Ours: the system's text size scales
# text and nothing else, and font.scaleWithSystem opts one item out. Runs on
# Ljós only.

@ljos-only @a11y
Feature: The system's text size
  A user who asks the system for larger text gets larger text in the
  application, at the sizes the application wrote.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 600; height: 300
        property real wider: scaled.implicitWidth / fixed.implicitWidth
        property real taller: scaled.implicitHeight / fixed.implicitHeight
        property real typed: field.implicitHeight / fixed.implicitHeight
        property bool scales: scaled.font.scaleWithSystem
        property bool fixedScales: fixed.font.scaleWithSystem
        Text { id: scaled; objectName: "scaled"; text: "Hello world"; font.pixelSize: 20 }
        Text {
          id: fixed; objectName: "fixed"; text: "Hello world"
          font.pixelSize: 20; font.scaleWithSystem: false
        }
        TextInput { id: field; objectName: "field"; text: "Hello world"; font.pixelSize: 20 }
        Rectangle { objectName: "box"; width: 50; height: 20 }
      }
      """

  Scenario: Text has the size that was written while the system asks for nothing
    Then the property "wider" of "root" is 1
    And the property "taller" of "root" is 1

  Scenario: Larger text of the system is larger text
    When the system setting "font scale" changes to 2
    Then the property "wider" of "root" is within 0.1 of 2
    And the property "taller" of "root" is within 0.1 of 2

  Scenario: Text that is typed into grows as well
    When the system setting "font scale" changes to 2
    Then the property "typed" of "root" is within 0.1 of 2

  Scenario: The size the application wrote reads as it was written
    When the system setting "font scale" changes to 2
    Then the property "font.pixelSize" of "scaled" is 20

  Scenario: Text that opted out keeps its size
    When the system setting "font scale" changes to 2
    Then the property "implicitHeight" of "fixed" is at most 30

  Scenario: Nothing but text grows
    When the system setting "font scale" changes to 2
    Then the property "width" of "box" is 50
    And the property "height" of "box" is 20

  Scenario: Text goes back when the system does
    When the system setting "font scale" changes to 2
    And the system setting "font scale" changes to 1
    Then the property "wider" of "root" is 1

  Scenario: scaleWithSystem starts out true
    Then the property "scales" of "root" is true
    And the property "fixedScales" of "root" is false

  Scenario: An item can opt out while the application runs
    When the system setting "font scale" changes to 2
    And the property "font.scaleWithSystem" of "scaled" is set to false
    Then the property "wider" of "root" is 1
