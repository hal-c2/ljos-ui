# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-transition.html#from-prop
#   https://doc.qt.io/qt-6/qml-qtquick-transition.html#enabled-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#transitions-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Which transition runs
  Of an item's transitions the one that names the change most exactly
  runs: both states before one of them, one before none. Of equals the
  first in the list runs.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 400; height: 400
        property string ran: ""
        property bool allowed: false
        component Named: Transition {
          id: t; property string tag
          ScriptAction { script: root.ran += "[" + t.tag + "]" }
        }
        states: [ State { name: "a" }, State { name: "b" }, State { name: "c" },
          State { name: "d" }, State { name: "e" } ]
        transitions: [
          Named { tag: "any" },
          Named { tag: "to b"; to: "b" },
          Named { tag: "from a"; from: "a" },
          Named { tag: "to c"; to: "c" },
          Named { tag: "a to b"; from: "a"; to: "b" },
          Named { tag: "c to d, off"; from: "c"; to: "d"; enabled: root.allowed },
          Named { tag: "list"; from: "b,c"; to: "d,e" },
          Named { tag: "a to b again"; from: "a"; to: "b" },
          Named { tag: "star"; from: "*"; to: "e" }
        ]
      }
      """

  @api-QtQuick.Transition.from @api-QtQuick.Transition.to
  Scenario Outline: From "<from>" to "<to>" the transition "<ran>" runs
    When the property "state" of "root" is set to "<from>"
    And the property "ran" of "root" is set to ""
    And the property "state" of "root" is set to "<to>"
    Then the property "ran" of "root" is "[<ran>]"

    Examples: A transition that names neither state runs for any change
      | from | to | ran |
      | d    | a  | any |
      | b    |    | any |

    Examples: A transition that names one state runs before one that names none
      | from | to | ran    |
      | d    | b  | to b   |
      | a    | d  | from a |
      | a    |    | from a |

    Examples: A transition that names both states runs before one that names one
      | from | to | ran    |
      | a    | b  | a to b |

    Examples: Of two that name the change equally exactly the first in the list runs
      | from | to | ran    |
      | a    | c  | from a |

    Examples: A list of names matches each of them
      | from | to | ran  |
      | b    | d  | list |
      | b    | e  | list |
      | c    | e  | list |

    Examples: An asterisk stands for any state
      | from | to | ran  |
      | a    | e  | from a |
      | d    | e  | star |

  @api-QtQuick.Transition.enabled
  Scenario: A transition that is not enabled is passed over
    When the property "state" of "root" is set to "c"
    And the property "ran" of "root" is set to ""
    And the property "state" of "root" is set to "d"
    Then the property "ran" of "root" is "[list]"

  @api-QtQuick.Transition.enabled
  Scenario: A transition that is enabled again runs
    When the property "state" of "root" is set to "c"
    And the property "ran" of "root" is set to ""
    And the property "allowed" of "root" is set to true
    And the property "state" of "root" is set to "d"
    Then the property "ran" of "root" is "[c to d, off]"

  Scenario: The base state is named by the empty string
    When the property "state" of "root" is set to "b"
    Then the property "ran" of "root" is "[to b]"
