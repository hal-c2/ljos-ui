# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-column.html
#   https://doc.qt.io/qt-6/qml-qtquick-row.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Anchors on the children of a positioner
  A positioner owns its children's position along the axis it lays out.
  A child of non-zero size anchored along that axis makes the positioner
  warn, place nothing in that pass and have no size of its own. Children
  it would skip anyway (invisible, or of zero size) are not checked, so
  their anchors are no conflict; one that made it give up still counts
  once it shrinks to nothing, so it does not start again.

  Scenario Outline: A child anchored along the axis stops a <type>
    Given the QML:
      """qml
      import QtQuick
      Item {
        <type> {
          objectName: "p"
          Item { objectName: "a"; width: 20; height: 20 }
          Item { objectName: "b"; width: 20; height: 20 }
          <child>
        }
      }
      """
    When 100 milliseconds pass
    Then the log has a warning matching "QML <type>: Cannot specify .* anchors for items inside <type>. <type> will not function."
    And the property "<pos>" of "b" is <at>
    And the property "implicitWidth" of "p" is 0
    And the property "implicitHeight" of "p" is 0

    # A child that fills the positioner has no size until the positioner
    # has one: the first pass places everyone, the next one warns and
    # leaves them where they are, and the positioner, without its size,
    # takes the child's away too.
    Examples:
      | type   | pos | at | child                                                     |
      | Column | y   | 20 | MouseArea { anchors.fill: parent }                        |
      | Column | y   | 0  | Item { width: 5; height: 5; anchors.top: parent.top }     |
      | Column | y   | 0  | Item { width: 5; height: 5; anchors.centerIn: parent }    |
      | Row    | x   | 20 | MouseArea { anchors.fill: parent }                        |
      | Row    | x   | 0  | Item { width: 5; height: 5; anchors.right: parent.right } |

  Scenario Outline: A child the <type> skips, or one anchored across it, is no conflict
    Given the QML:
      """qml
      import QtQuick
      Item {
        <type> {
          objectName: "p"
          Item { objectName: "a"; width: 20; height: 20 }
          Item { objectName: "b"; width: 20; height: 20 }
          <child>
        }
      }
      """
    When 100 milliseconds pass
    Then the log has no warnings
    And the property "<pos>" of "b" is 20

    Examples:
      | type   | pos | child                                                                |
      | Column | y   | Item { anchors.top: parent.top }                                     |
      | Column | y   | Item { visible: false; width: 5; height: 5; anchors.top: parent.top } |
      | Column | y   | Item { width: 5; height: 5; anchors.left: parent.left }              |
      | Row    | x   | Item { width: 0; height: 5; anchors.left: parent.left }              |
      | Row    | x   | Item { width: 5; height: 5; anchors.bottom: parent.bottom }          |
