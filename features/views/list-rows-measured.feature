# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (cacheBuffer)
#   https://doc.qt.io/qt-6/qml-qtquick-column.html
# Verified on Qt 6.11.2 (qml6, offscreen): a row is measured before the
# next is made, so a first row shorter than the rest does not stand for
# them; the counts below are exact on qml6.

@qt-parity @api-QtQuick.ListView.cacheBuffer
Feature: A ListView measures each row before it makes the next
  A view fills what is shown and its cacheBuffer with rows as long as each
  row is once laid out, not as long as the first row was. A row that is a
  column of a text is only as long as the text once it is laid out, so the
  view lays each new row out before it decides whether another one fits.
  Moving through the list makes every row once.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 400; height: 300
        property int created: 0; property int destroyed: 0
        property int twice: 0; property var seen: []
        ListView {
          objectName: "list"
          anchors.fill: parent; cacheBuffer: 0; model: 40
          delegate: Column {
            required property int index
            objectName: "r" + index; width: 400
            Text {
              text: parent.index === 0 ? "first" : "a\nb\nc\nd"
              font.pixelSize: 16
              lineHeightMode: Text.FixedHeight; lineHeight: 25
            }
            Component.onCompleted: {
              if (root.seen[index]) root.twice++
              root.seen[index] = true; root.created++
            }
            Component.onDestruction: root.destroyed++
          }
        }
      }
      """

  Scenario: A short first row does not stand for the rows after it
    When 300 milliseconds pass
    Then the property "created" of "root" is 4
    And the property "destroyed" of "root" is 0
    And the property "y" of "r3" is 225

  Scenario: The cacheBuffer is filled with rows as long as each is
    When the property "cacheBuffer" of "list" is set to 200
    And 300 milliseconds pass
    Then the property "created" of "root" is 6
    And the property "destroyed" of "root" is 0
    And the property "y" of "r5" is 425

  Scenario: Moving through the list makes no row twice
    When 300 milliseconds pass
    And the property "contentY" of "list" is set to 150
    And 50 milliseconds pass
    And the property "contentY" of "list" is set to 400
    And 50 milliseconds pass
    And the property "contentY" of "list" is set to 900
    And 50 milliseconds pass
    And the property "contentY" of "list" is set to 1500
    And 50 milliseconds pass
    Then the property "twice" of "root" is 0
    And the property "created" of "root" is 16
