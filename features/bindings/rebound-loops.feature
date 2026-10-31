# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#binding-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Loops that close only once everything was evaluated
  A property bound again, after the file was read, to what depends on it
  makes a loop that no binding shows when the file is read. Qt goes
  round it once and leaves each property as it was when it changed.

  Background:
    Given the QML:
      """qml
      import QtQuick
      QtObject {
        objectName: "tank"
        property int layout: 0
        readonly property bool mobile: layout === 1
        property int least: mobile ? 364 : 484
        property int given: 300
        readonly property int side: Math.max(given, least)
      }
      """


  Scenario: A loop that changes nothing on the way round
    When the property "layout" of "tank" is bound to "side < 400 ? 1 : 0"
    Then the property "layout" of "tank" is 0
    And the property "mobile" of "tank" is false
    And the property "least" of "tank" is 484
    And the property "side" of "tank" is 484
    When the property "given" of "tank" is set to 380
    Then the property "layout" of "tank" is 0
    And the property "mobile" of "tank" is false
    And the property "least" of "tank" is 484
    And the property "side" of "tank" is 484
    When the property "given" of "tank" is set to 500
    Then the property "layout" of "tank" is 0
    And the property "mobile" of "tank" is false
    And the property "least" of "tank" is 484
    And the property "side" of "tank" is 500
    When the property "given" of "tank" is set to 100
    Then the property "layout" of "tank" is 0
    And the property "mobile" of "tank" is false
    And the property "least" of "tank" is 484
    And the property "side" of "tank" is 484

  Scenario: A loop that closes after one round rests there
    When the property "layout" of "tank" is bound to "side > 300 ? 1 : 0"
    Then the property "layout" of "tank" is 1
    And the property "mobile" of "tank" is true
    And the property "least" of "tank" is 364
    And the property "side" of "tank" is 364
    When the property "given" of "tank" is set to 250
    Then the property "layout" of "tank" is 1
    And the property "mobile" of "tank" is true
    And the property "least" of "tank" is 364
    And the property "side" of "tank" is 364
    When the property "given" of "tank" is set to 500
    Then the property "layout" of "tank" is 1
    And the property "mobile" of "tank" is true
    And the property "least" of "tank" is 364
    And the property "side" of "tank" is 500
    When the property "given" of "tank" is set to 100
    Then the property "layout" of "tank" is 1
    And the property "mobile" of "tank" is true
    And the property "least" of "tank" is 364
    And the property "side" of "tank" is 364

  # Ljós stops this loop where the change comes back, and leaves layout, mobile
  # and side from different rounds (init: 0, true, 364, 364; at 380: 1, false,
  # 484, 484), where Qt leaves each as it was when it changed.
  @backlog
  Scenario: A loop that never closes stops after one round, each property as it was when it changed
    When the property "layout" of "tank" is bound to "side > 400 ? 1 : 0"
    Then the property "layout" of "tank" is 1
    And the property "mobile" of "tank" is true
    And the property "least" of "tank" is 364
    And the property "side" of "tank" is 364
    When the property "given" of "tank" is set to 600
    Then the property "layout" of "tank" is 1
    And the property "mobile" of "tank" is true
    And the property "least" of "tank" is 364
    And the property "side" of "tank" is 600
    When the property "given" of "tank" is set to 380
    Then the property "layout" of "tank" is 0
    And the property "mobile" of "tank" is false
    And the property "least" of "tank" is 484
    And the property "side" of "tank" is 380
    When the property "given" of "tank" is set to 100
    Then the property "layout" of "tank" is 1
    And the property "mobile" of "tank" is true
    And the property "least" of "tank" is 364
    And the property "side" of "tank" is 484
