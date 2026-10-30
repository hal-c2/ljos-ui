# Sources:
#   docs/ARCHITECTURE.md sections 19.4 and 19.9
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#childAt-method
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#clip-prop
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: Finding the item at a point of the window
  A tool that shows a running application and lets a person or an agent
  point at it asks what is drawn at that point and where in the QML it was
  written. The locator `at=X,Y` names the items drawn at a point of the
  window, in logical pixels, topmost first: the item painted last there
  comes first. An item that is not visible, or whose opacity is 0, is not
  drawn and is not found; nor is the part of an item a clip cuts away.
  Items with no accessible role are found as well. A tool that needs one
  item takes the topmost; `>> nth=1` is the one under it.

  Background:
    Given the file "Badge.qml":
      """qml
      import QtQuick
      Rectangle {
        width: 40; height: 40
        Rectangle { objectName: "dot"; x: 10; y: 10; width: 20; height: 20 }
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 300; height: 200
        component Pill: Rectangle { width: 20; height: 20 }
        Rectangle {
          objectName: "panel"
          width: 100; height: 100
          Rectangle { objectName: "chip"; x: 20; y: 20; width: 30; height: 30 }
        }
        Rectangle {
          objectName: "clipper"
          x: 120; width: 40; height: 40; clip: true
          Rectangle { objectName: "spill"; x: 30; width: 40; height: 40 }
        }
        Rectangle { objectName: "hidden"; x: 200; width: 40; height: 40; visible: false }
        Rectangle { objectName: "clear"; x: 200; y: 50; width: 40; height: 40; opacity: 0 }
        Badge { objectName: "badge"; x: 250 }
        Pill { objectName: "pill"; y: 150 }
        Row {
          x: 100; y: 150
          Repeater { model: 2; Rectangle { width: 20; height: 20 } }
        }
      }
      """
    And the inspector is connected

  Scenario: An item with no role is found by a point inside it
    When the agent inspects "at=80,80"
    Then the answer's "/object_name" is "panel"
    And the answer's "/type" is "Rectangle"
    And the answer's "/role" is null
    And the answer's "/geometry/window" is [0, 0, 100, 100]
    And the answer's "/source/file" matches "Scenario\.qml$"
    And the answer's "/source/line" is 6

  Scenario: A child drawn over its parent is the one at the point
    When the agent inspects "at=30.5,30.5"
    Then the answer's "/object_name" is "chip"

  Scenario: The items at a point are listed topmost first
    When the agent searches for "at=30,30"
    Then the answer's "/matches/0/obj" is "chip"
    And the answer's "/matches/1/obj" is "panel"
    And the answer's "/matches/2/obj" is "root"
    And the answer's "/matches/1/type" is "Rectangle"
    And the answer's "/matches/1/source/line" is 6

  Scenario: The item under the topmost is the next one down
    When the agent inspects "at=30,30 >> nth=1"
    Then the answer's "/object_name" is "panel"

  Scenario: The part of a child its parent clips away is not found
    When the agent inspects "at=175,20"
    Then the answer's "/object_name" is "root"
    When the agent inspects "at=155,20"
    Then the answer's "/object_name" is "spill"

  Scenario Outline: An item that is not drawn is not found
    When the agent inspects "<point>"
    Then the answer's "/object_name" is "root"

    Examples:
      | point      |
      | at=220,20  |
      | at=220,70  |

  Scenario: A point outside the window finds nothing
    When the agent inspects "at=500,500"
    Then the agent is refused with "not_found"
    When the agent searches for "at=500,500"
    Then the answer's "/total" is 0

  Scenario Outline: A point that cannot be read, or a step it cannot go with, is refused
    When the agent inspects "<locator>"
    Then the agent is refused with "invalid_argument"
    And the refusal mentions "at="

    Examples:
      | locator                 |
      | at=40                   |
      | at=forty,40             |
      | at=40,40[disabled]      |
      | role=button >> at=40,40 |
      | at=40,40 >> #chip       |

  Scenario: An item written in another file says where it was written there
    When the agent inspects "at=265,15"
    Then the answer's "/object_name" is "dot"
    And the answer's "/source/file" matches "Badge\.qml$"
    And the answer's "/source/line" is 4

  # The root of a component file and an instance of an inline component
  # are where they are used, which is where their object name and
  # position are written; the copies a Repeater makes are where its
  # delegate is written.
  Scenario Outline: An item made from a component says where it was used
    When the agent inspects "<point>"
    Then the answer's "/object_name" is "<item>"
    And the answer's "/source/file" matches "Scenario\.qml$"
    And the answer's "/source/line" is <line>

    Examples:
      | point    | item  | line |
      | at=252,2 | badge | 18   |
      | at=5,155 | pill  | 19   |

  Scenario: A copy a Repeater made says where its delegate is written
    When the agent inspects "at=125,155"
    Then the answer's "/type" is "Rectangle"
    And the answer's "/source/file" matches "Scenario\.qml$"
    And the answer's "/source/line" is 22
