# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (positionViewAtEnd, contentHeight is an estimate)
#   docs/ARCHITECTURE.md section 8.4
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A list sent to its end again keeps its length
  A row the view has measured keeps its height in the length estimate, so bringing a list
  that is already at its end there again, with nothing changed, leaves contentHeight alone.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: win
        objectName: "win"; width: 900; height: 758
        property int changes: 0
        property bool grown: false
        readonly property var last: [246, 48, 42, 246, 48, 42, 370]
        function toEnd() { view.positionViewAtEnd() }
        function toStart() { view.positionViewAtBeginning() }
        function toMiddle() { view.positionViewAtIndex(200, ListView.Center) }
        function grow() { grown = true }
        ListView {
          id: view
          objectName: "list"; anchors.fill: parent
          topMargin: 12; bottomMargin: 12
          model: 390
          onContentHeightChanged: win.changes++
          delegate: Item {
            required property int index
            width: 900
            height: index === 389 ? (win.grown ? 320 : 40) : index === 0 ? 198
                  : index >= 382 ? win.last[index - 382] : 60 + (index * 37) % 300
          }
        }
      }
      """
    When the method "toEnd" of "win" is called
    And 100 milliseconds pass
    And the method "grow" of "win" is called
    And the method "toEnd" of "win" is called
    And 100 milliseconds pass

  Scenario: Sending the list to its end again does not change its length
    When the property "changes" of "win" is set to 0
    And the method "toEnd" of "win" is called
    And 100 milliseconds pass
    And the method "toEnd" of "win" is called
    And 100 milliseconds pass
    And the method "toEnd" of "win" is called
    And 100 milliseconds pass
    Then the property "changes" of "win" is 0

  Scenario: Going to the start and back to a row does not flip the length
    When the method "toStart" of "win" is called
    And 100 milliseconds pass
    And the method "toMiddle" of "win" is called
    And 100 milliseconds pass
    And the property "changes" of "win" is set to 0
    And the method "toMiddle" of "win" is called
    And 100 milliseconds pass
    And the method "toMiddle" of "win" is called
    And 100 milliseconds pass
    Then the property "changes" of "win" is 0
