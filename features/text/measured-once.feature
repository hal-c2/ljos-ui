# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html (implicitWidth, contentWidth)
#   docs/ARCHITECTURE.md (principle 6: work is scheduled by what changed)

# When Qt lays its text out cannot be seen from QML: how often is Ljós's own.
@ljos-only
Feature: A text is measured again only when what it measures changes
  A Text is laid out to say how large it is: its implicit size, its
  content size and its line count. That is done again when its text, its
  font or the width it wraps to changes, and not when something it does
  not measure with changes: where it is, a width it does not wrap to, or a
  height it is not aligned in.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 400; height: 200
        property bool small: false
        property bool tiny: false
        property real side: 100
        Text {
          objectName: "label"; width: root.side; text: "Living Room"
          font.pixelSize: root.small ? 18 : 24
        }
        Text {
          objectName: "wrapped"; y: 40; width: root.side
          wrapMode: Text.Wrap; text: "Energy usage this week"
        }
        Text {
          objectName: "elided"; y: 120; elide: Text.ElideRight; text: "Cool"
          font.pixelSize: root.tiny ? 10 : 12
        }
      }
      """
    When 50 milliseconds pass
    And the times text is measured are counted

  Scenario: A text is not measured again when it moves
    When the property "x" of "label" is set to 30
    And 50 milliseconds pass
    Then text was measured at most 0 times since that was counted

  Scenario: A text that does not wrap is not measured again when its width changes
    When the property "width" of "label" is set to 300
    And 50 milliseconds pass
    Then text was measured at most 0 times since that was counted

  Scenario: A text at the top of its item is not measured again when its height changes
    When the property "height" of "label" is set to 80
    And 50 milliseconds pass
    Then text was measured at most 0 times since that was counted

  Scenario: A text is measured again once when its font changes
    When the property "small" of "root" is set to true
    And 50 milliseconds pass
    Then text was measured at most 1 time since that was counted
    And the property "font.pixelSize" of "label" is 18

  # An eliding text reads its width; one that takes its width from what it
  # measures gets a new width from the measuring itself, which it then
  # does not elide to.
  Scenario: A text that elides and sizes itself is measured once when its font changes
    When the property "tiny" of "root" is set to true
    And 50 milliseconds pass
    Then text was measured at most 1 time since that was counted
    And the property "width" of "elided" is at most 25

  Scenario: A text that wraps is measured again when the width it wraps to changes
    When the property "side" of "root" is set to 60
    And 50 milliseconds pass
    Then text was measured at most 2 times since that was counted

