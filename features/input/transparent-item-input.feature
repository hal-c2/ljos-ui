# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#opacity-prop
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).

@qt-parity
Feature: A transparent item still takes input
  Opacity is about how an item looks. An item at `opacity: 0` is not drawn, and it still takes the
  pointer where it is; only `visible: false` and `enabled: false` stop that. Designs lay a transparent
  button over the look of a row this way.

  Rule: Opacity does not change what the pointer finds

    Scenario: A click lands on what is inside a transparent item
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 100; height: 100
          Item { anchors.fill: parent; opacity: 0
            MouseArea { objectName: "inside"; anchors.fill: parent
              property int clicks: 0; onClicked: clicks++ } }
        }
        """
      When the user presses the pointer at 50, 50
      And the user releases the pointer at 50, 50
      Then the property "clicks" of "inside" is 1

    Scenario: A transparent button lying over a row is clicked
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        Rectangle {
          width: 100; height: 100
          Text { text: "row" }
          Button { objectName: "over"; anchors.fill: parent; opacity: 0
            property int clicks: 0; onClicked: clicks++ }
        }
        """
      When the user presses the pointer at 50, 50
      And the user releases the pointer at 50, 50
      Then the property "clicks" of "over" is 1

    Scenario: A transparent item lying over another takes the click from it
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 100; height: 100
          MouseArea { objectName: "below"; anchors.fill: parent
            property int clicks: 0; onClicked: clicks++ }
          MouseArea { objectName: "above"; anchors.fill: parent; opacity: 0
            property int clicks: 0; onClicked: clicks++ }
        }
        """
      When the user presses the pointer at 50, 50
      And the user releases the pointer at 50, 50
      Then the property "clicks" of "above" is 1
      And the property "clicks" of "below" is 0

    Scenario: An item that is not visible takes no click
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 100; height: 100
          MouseArea { objectName: "below"; anchors.fill: parent
            property int clicks: 0; onClicked: clicks++ }
          MouseArea { objectName: "above"; anchors.fill: parent; visible: false
            property int clicks: 0; onClicked: clicks++ }
        }
        """
      When the user presses the pointer at 50, 50
      And the user releases the pointer at 50, 50
      Then the property "clicks" of "below" is 1
      And the property "clicks" of "above" is 0
