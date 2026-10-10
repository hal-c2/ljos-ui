# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-positioner.html
#   https://doc.qt.io/qt-6/qml-qtquick-row.html
#   https://doc.qt.io/qt-6/qml-qtquick-column.html
#   https://doc.qt.io/qt-6/qml-qtquick-flow.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Padding and the Positioner attached properties
  Every positioner has padding around its children, and tells each child
  where it stands through the Positioner attached properties.

  Rule: Padding

    Scenario Outline: Padding is added on every side and each side can have its own
      Given the QML:
        """qml
        import QtQuick
        Item {
          Row {
            objectName: "row"; spacing: 2; padding: 5; rightPadding: 20
            Item { objectName: "r1"; width: 40; height: 20 }
            Item { objectName: "r2"; width: 30; height: 10 }
          }
          Column {
            objectName: "column"; spacing: 2; padding: 5; bottomPadding: 20; leftPadding: 1
            Item { objectName: "c1"; width: 40; height: 10 }
            Item { objectName: "c2"; width: 30; height: 20 }
          }
          Flow {
            objectName: "flow"; width: 60; padding: 5; topPadding: 9
            Item { objectName: "f1"; width: 40; height: 10 }
            Item { objectName: "f2"; width: 30; height: 20 }
          }
        }
        """
      When 100 milliseconds pass
      Then the property "x" of "<last>" is <x>
      And the property "y" of "<last>" is <y>
      And the property "implicitWidth" of "<positioner>" is <width>
      And the property "implicitHeight" of "<positioner>" is <height>

      Examples:
        | positioner | last | x  | y  | width | height |
        | row        | r2   | 47 | 5  | 97    | 30     |
        | column     | c2   | 1  | 17 | 46    | 57     |
        | flow       | f2   | 5  | 19 | 50    | 44     |

    Scenario: Padding can change and be put back to follow the common padding
      Given the QML:
        """qml
        import QtQuick
        Row {
          objectName: "row"; padding: 5
          Item { objectName: "a"; width: 40; height: 20 }
        }
        """
      When the property "leftPadding" of "row" is set to 12
      And 100 milliseconds pass
      Then the property "x" of "a" is 12
      And the property "width" of "row" is 57
      And the property "topPadding" of "row" is 5

  # The numbers are Qt's. Parked: an attached property cannot be read in an
  # expression yet (the engine gives `Positioner` to JavaScript as its enum
  # keys only), so nothing in QML can observe these values.
  @backlog
  Rule: Positioner attached properties

    Scenario Outline: A child knows its place among the positioned children
      Given the QML:
        """qml
        import QtQuick
        Row {
          Item {
            objectName: "a"; width: 10; height: 10
            property int at: Positioner.index
            property bool first: Positioner.isFirstItem
            property bool last: Positioner.isLastItem
          }
          Item { objectName: "hidden"; visible: false; property int at: Positioner.index }
          Item {
            objectName: "b"; width: 10; height: 10
            property int at: Positioner.index
            property bool first: Positioner.isFirstItem
            property bool last: Positioner.isLastItem
          }
        }
        """
      When 100 milliseconds pass
      Then the property "at" of "<item>" is <at>
      And the property "first" of "<item>" is <first>
      And the property "last" of "<item>" is <last>

      Examples:
        | item | at | first | last  |
        | a    | 0  | true  | false |
        | b    | 1  | false | true  |

    Scenario: A child that is not positioned has no index
      Given the QML:
        """qml
        import QtQuick
        Column {
          Item { objectName: "hidden"; visible: false; property int at: Positioner.index }
          Item { objectName: "a"; width: 10; height: 10; property int at: Positioner.index }
        }
        """
      When 100 milliseconds pass
      Then the property "at" of "hidden" is -1
      And the property "at" of "a" is 0
