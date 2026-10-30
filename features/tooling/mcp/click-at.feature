# Sources:
#   docs/ARCHITECTURE.md sections 19.4, 19.6 and 19.7
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html
#   https://doc.qt.io/qt-6/qml-qtquick-taphandler.html
#   https://doc.qt.io/qt-6/qml-qtquick-hoverhandler.html
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: A click at an exact point of the window
  A click on an item lands at its centre, which is not always where an agent
  needs it: a thin handle on a larger item's edge, the corner of a resize
  grip. `app_click` with `at` is a click at that point of the window, in its
  logical pixels. Nothing is aimed at: the point goes through the same hit
  test as a person's pointer, so whatever is on top there gets it. The
  pointer moves there first, so what follows the hover sees it arrive.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        id: top
        objectName: "root"; width: 300; height: 200
        property int big: 0; property int small: 0; property int tapped: 0
        property int rights: 0; property int doubles: 0
        property bool hovered: hover.hovered
        property point spot: Qt.point(-1, -1)
        Rectangle {
          x: 100; y: 100; width: 20; height: 20
          MouseArea { anchors.fill: parent; onClicked: top.small += 1 }
        }
        Rectangle {
          x: 10; y: 10; width: 100; height: 100
          MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onClicked: function (e) { if (e.button === Qt.RightButton) top.rights += 1; else top.big += 1; top.spot = Qt.point(e.x, e.y) }
            onDoubleClicked: top.doubles += 1
          }
        }
        Rectangle { x: 200; y: 10; width: 40; height: 40; TapHandler { onTapped: top.tapped += 1 } }
        Rectangle { x: 200; y: 100; width: 40; height: 40; HoverHandler { id: hover } }
      }
      """
    And the inspector is connected with the capabilities "inspect, input"

  Rule: The point goes where a person's pointer would

    # The small item is below the large one where they overlap, and its centre
    # (110, 110) is on the very edge of the large one: this is what aiming at
    # an item's centre cannot tell apart.
    Scenario Outline: A small item under a larger one's edge gets a click only where it shows
      When the agent calls the tool "app_click" with:
        """json
        {"at": [<x>, <y>]}
        """
      Then the property "small" of "root" is <small>
      And the property "big" of "root" is <big>

      Examples:
        | x   | y   | small | big |
        | 105 | 105 | 0     | 1   |
        | 115 | 115 | 1     | 0   |

    Scenario Outline: The pixels of an item's corner are its own and the next ones are not
      When the agent calls the tool "app_click" with:
        """json
        {"at": [<x>, <y>]}
        """
      Then the property "big" of "root" is <big>

      Examples:
        | x     | y     | big |
        | 10    | 10    | 1   |
        | 109.5 | 10    | 1   |
        | 10    | 109.5 | 1   |
        | 9.5   | 10    | 0   |
        | 110   | 10    | 0   |

    Scenario: The item is told the point, in its own coordinates
      When the agent calls the tool "app_click" with:
        """json
        {"at": [13, 17]}
        """
      Then the property "spot" of "root" is {"x": 3, "y": 7}

    Scenario: A handler gets it as it gets a person's tap
      When the agent calls the tool "app_click" with:
        """json
        {"at": [201, 11]}
        """
      Then the property "tapped" of "root" is 1

    Scenario: A point on nothing clicks nothing and says where it was
      When the agent calls the tool "app_click" with:
        """json
        {"at": [290, 190]}
        """
      Then the answer's "/at/0" is 290
      And the answer's "/at/1" is 190
      And the property "big" of "root" is 0
      And the property "small" of "root" is 0

    Scenario: The pointer moves there first, and what follows the hover sees it stay
      When the agent calls the tool "app_click" with:
        """json
        {"at": [210, 110]}
        """
      Then the property "hovered" of "root" is true

    Scenario: A button that is not the left one is a different click
      When the agent calls the tool "app_click" with:
        """json
        {"at": [20, 20], "button": "right"}
        """
      Then the property "rights" of "root" is 1
      And the property "big" of "root" is 0

    Scenario: A double click at a point is a click and then a double click
      When the agent calls the tool "app_click" with:
        """json
        {"at": [20, 20], "count": 2}
        """
      Then the property "big" of "root" is 1
      And the property "doubles" of "root" is 1

    Scenario: A batch may click at a point
      When the agent calls the tool "app_batch" with:
        """json
        {"steps": [{"tool": "app_click", "args": {"at": [20, 20]}}, {"tool": "app_click", "args": {"at": [115, 115]}}]}
        """
      Then the property "big" of "root" is 1
      And the property "small" of "root" is 1

  Rule: The point is in logical pixels whatever the scale

    Scenario: A window at twice the scale takes the same point
      Given the agent has called the tool "app_resize" with:
        """json
        {"width": 300, "height": 200, "scale": 2}
        """
      When the agent calls the tool "app_click" with:
        """json
        {"at": [115, 115]}
        """
      Then the property "small" of "root" is 1

  Rule: A point that cannot be clicked is told before anything is done

    Scenario Outline: A point outside the window is refused
      When the agent calls the tool "app_click" with:
        """json
        {"at": [<x>, <y>]}
        """
      Then the agent is refused with "not_actionable"
      And the refusal mentions "outside the window"
      And the property "big" of "root" is 0

      Examples:
        | x     | y     |
        | -1    | 10    |
        | 10    | -0.5  |
        | 400   | 10    |
        | 10    | 400   |
        | 90000 | 90000 |

    Scenario: An item and a point at once is refused
      When the agent calls the tool "app_click" with:
        """json
        {"target": "#root", "at": [20, 20]}
        """
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "target"
      And the property "big" of "root" is 0

    Scenario: A click on neither an item nor a point is refused
      When the agent calls the tool "app_click" with:
        """json
        {"count": 1}
        """
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "target"

    Scenario Outline: A point that is not two numbers is refused
      When the agent calls the tool "app_click" with:
        """json
        {"at": <at>}
        """
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "at"

      Examples:
        | at         |
        | [20]       |
        | [20, 20, 1]|
        | "20,20"    |

  Rule: It is input, and asks for what input asks for

    Scenario: Without the capability it is refused
      Given the inspector is connected with the capabilities "inspect"
      When the agent calls the tool "app_click" with:
        """json
        {"at": [20, 20]}
        """
      Then the agent is refused with "capability_denied"
      And the property "big" of "root" is 0
