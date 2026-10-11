# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-anchorchanges.html
#   https://doc.qt.io/qt-6/qtquick-statesanimations-states.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: AnchorChanges
  An AnchorChanges gives an item other anchors while a state lasts. An
  anchor it sets to `undefined` is taken off.

  Rule: A state gives an item other anchors

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          Item { id: holder; x: 50; y: 60; width: 100; height: 100 }
          Rectangle {
            id: box; objectName: "box"; width: 10; height: 10
            anchors.left: parent.left; anchors.top: parent.top
          }
          states: [
            State { name: "corner"
              AnchorChanges {
                target: box
                anchors.left: undefined; anchors.right: root.right
                anchors.top: undefined; anchors.verticalCenter: root.verticalCenter
              } },
            State { name: "beside"
              AnchorChanges { target: box; anchors.left: holder.right }
              PropertyChanges { target: box; anchors.leftMargin: 7 } }
          ]
        }
        """

    @api-QtQuick.AnchorChanges.anchors
    Scenario Outline: The item is laid out by the anchors of the state: <property>
      When the property "state" of "root" is set to "corner"
      Then the property "<property>" of "box" is <value>

      Examples:
        | property | value |
        | x        | 190   |
        | y        | 95    |

    Scenario Outline: Leaving the state gives the item its own anchors back: <property>
      When the property "state" of "root" is set to "corner"
      And the property "state" of "root" is set to ""
      Then the property "<property>" of "box" is <value>

      Examples:
        | property | value |
        | x        | 0     |
        | y        | 0     |

    Scenario: The item follows what the state anchors it to
      When the property "state" of "root" is set to "corner"
      And the property "width" of "root" is set to 300
      Then the property "x" of "box" is 290

    Scenario: An anchor's margin is changed by a PropertyChanges
      When the property "state" of "root" is set to "beside"
      Then the property "x" of "box" is 157

    Scenario: An anchor the state does not name stays
      When the property "state" of "root" is set to "beside"
      Then the property "y" of "box" is 0

  Rule: A line given `undefined` by a script is an anchor the state takes off

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property bool on: true
          Item { id: holder; objectName: "holder"; x: 50; width: 100; height: 100 }
          Item { id: other; objectName: "other"; x: 20; width: 30; height: 30 }
          Rectangle {
            id: box; objectName: "box"; width: 10; height: 10
            anchors.left: other.left
          }
          states: [
            State { name: "named"
              AnchorChanges { id: named; target: box; anchors.left: holder.right } },
            State { name: "plain"
              AnchorChanges { id: plain; target: box } },
            State { name: "bound"
              AnchorChanges { target: box; anchors.left: root.on ? holder.right : undefined } }
          ]
          function dropNamed() { named.anchors.left = undefined }
          function dropPlain() { plain.anchors.left = undefined }
        }
        """

    @api-QtQuick.AnchorChanges.anchors
    Scenario Outline: A line the change gave an anchor no longer does: moving <item>
      When the method "dropNamed" of "root" is called
      And the property "state" of "root" is set to "named"
      And the property "x" of "<item>" is set to <to>
      Then the property "x" of "box" is 20

      Examples:
        | item   | to |
        | holder | 60 |
        | other  | 25 |

    @api-QtQuick.AnchorChanges.anchors
    Scenario: A line the change did not name takes the anchor off as well
      When the method "dropPlain" of "root" is called
      And the property "state" of "root" is set to "plain"
      And the property "x" of "other" is set to 25
      Then the property "x" of "box" is 20

    Scenario: Without the script the state leaves that anchor alone
      When the property "state" of "root" is set to "plain"
      And the property "x" of "other" is set to 25
      Then the property "x" of "box" is 25

    Scenario: Leaving the state gives the item the anchor it had
      When the method "dropNamed" of "root" is called
      And the property "state" of "root" is set to "named"
      And the property "state" of "root" is set to ""
      And the property "x" of "other" is set to 25
      Then the property "x" of "box" is 25

    Scenario: A line taken back while the state lasts keeps its anchor until the state is left
      When the property "state" of "root" is set to "named"
      And the method "dropNamed" of "root" is called
      And the property "x" of "holder" is set to 60
      Then the property "x" of "box" is 160

    Scenario: A line taken back while the state lasts is off when the state is entered again
      When the property "state" of "root" is set to "named"
      And the method "dropNamed" of "root" is called
      And the property "state" of "root" is set to ""
      And the property "state" of "root" is set to "named"
      And the property "x" of "holder" is set to 60
      Then the property "x" of "box" is 20

    Scenario: A line whose expression comes to `undefined` takes the anchor off at once
      When the property "state" of "root" is set to "bound"
      And the property "on" of "root" is set to false
      And the property "x" of "holder" is set to 60
      Then the property "x" of "box" is 150

    Scenario: A line whose expression gives an anchor again is followed again
      When the property "state" of "root" is set to "bound"
      And the property "on" of "root" is set to false
      And the property "x" of "holder" is set to 60
      And the property "on" of "root" is set to true
      Then the property "x" of "box" is 160

    Scenario: A state entered while the expression is `undefined` takes the anchor off
      When the property "on" of "root" is set to false
      And the property "state" of "root" is set to "bound"
      And the property "x" of "other" is set to 25
      Then the property "x" of "box" is 20
