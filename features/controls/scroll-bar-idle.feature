# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-scrollbar.html
#   docs/ARCHITECTURE.md (principle 6: nothing polls)

# What an application has scheduled is Ljós's own to say.
@ljos-only @headless
Feature: A scroll bar nobody uses wakes nothing
  A scroll bar's handle fades after use. A bar that has not been used has
  nothing to fade, and one that is always off shows nothing at all, so an
  application that starts with either has nothing scheduled.

  Scenario Outline: A list with a <policy> bar is at rest as it starts
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      ListView {
        width: 200; height: 100
        model: 50
        delegate: Text { text: index; height: 20 }
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.<policy> }
      }
      """
    Then the application has nothing scheduled

    Examples:
      | policy    |
      | AsNeeded  |
      | AlwaysOff |
      | AlwaysOn  |

  Scenario: A scroll view that fits is at rest as it starts
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      ScrollView {
        width: 200; height: 100
        Text { text: "short" }
      }
      """
    Then the application has nothing scheduled

  Scenario Outline: A list that starts scrolled by <how> is at rest as it starts
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      ListView {
        id: list
        width: 200; height: 100
        model: 50
        delegate: Text { text: index; height: 20 }
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.<policy> }
        <how>
      }
      """
    Then the application has nothing scheduled

    Examples:
      | policy    | how                                                |
      | AsNeeded  | contentY: 40                                       |
      | AlwaysOff | contentY: 40                                       |
      | AsNeeded  | currentIndex: 30                                   |
      | AlwaysOff | currentIndex: 30                                   |
      | AsNeeded  | Component.onCompleted: positionViewAtIndex(30, ListView.Beginning) |
      | AlwaysOff | Component.onCompleted: positionViewAtIndex(30, ListView.Beginning) |
