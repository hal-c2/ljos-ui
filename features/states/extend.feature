# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-state.html#extend-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A state that extends another
  A state that extends another makes that state's changes and then its own.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 200; height: 200
        Rectangle { id: box; objectName: "box"; width: 10; height: 10; x: 1 }
        states: [
          State { name: "wide"
            PropertyChanges { target: box; width: 100; x: 20 } },
          State { name: "big"; extend: "wide"
            PropertyChanges { target: box; height: 100; x: 30 } },
          State { name: "huge"; extend: "big"
            PropertyChanges { target: box; height: 300 } }
        ]
      }
      """

  @api-QtQuick.State.extend
  Scenario Outline: In "<state>" the <property> is <value>
    When the property "state" of "root" is set to "<state>"
    Then the property "<property>" of "box" is <value>

    Examples: The state that is extended is as it was
      | state | property | value |
      | wide  | width    | 100   |
      | wide  | height   | 10    |
      | wide  | x        | 20    |

    Examples: The extending state has both states' changes, and its own count where both change a property
      | state | property | value |
      | big   | width    | 100   |
      | big   | height   | 100   |
      | big   | x        | 30    |

    Examples: A state may extend one that extends another
      | state | property | value |
      | huge  | width    | 100   |
      | huge  | height   | 300   |
      | huge  | x        | 30    |

  Scenario Outline: Leaving an extending state puts <property> back
    When the property "state" of "root" is set to "huge"
    And the property "state" of "root" is set to ""
    Then the property "<property>" of "box" is <value>

    Examples:
      | property | value |
      | width    | 10    |
      | height   | 10    |
      | x        | 1     |

  Scenario: Going from an extending state to the one it extends keeps the shared changes
    When the property "state" of "root" is set to "big"
    And the property "state" of "root" is set to "wide"
    Then the property "width" of "box" is 100
    And the property "height" of "box" is 10
    And the property "x" of "box" is 20
