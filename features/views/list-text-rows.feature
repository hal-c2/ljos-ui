# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (cacheBuffer)
#   https://doc.qt.io/qt-6/qml-qtquick-column.html
# Verified on Qt 6.11.2 (qml6, offscreen): 27 delegates while the rows are
# 23 long, 8 once they are 83.
# Delegate counts are bands: Qt does not promise exact numbers.

@qt-parity @api-QtQuick.ListView.cacheBuffer
Feature: A new ListView whose rows are as long as their text makes only the rows it shows
  A row whose length comes from the text in it has no length until the text is laid out.
  The view does not take such rows as having none and make a delegate for every row it
  could then fit.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 200; height: 300
        property int created: 0
        property int destroyed: 0
        readonly property int live: created - destroyed
        property bool grown: false
        ListView {
          objectName: "list"
          anchors.fill: parent
          model: 1000
          delegate: Column {
            required property int index
            objectName: "r" + index
            width: 200
            Text { text: "Row " + parent.index; font.pixelSize: 20 }
            Rectangle { width: 10; height: root.grown ? 60 : 0 }
            Component.onCompleted: root.created++
            Component.onDestruction: root.destroyed++
          }
        }
      }
      """
    When 200 milliseconds pass

  Scenario: The view makes the rows it shows and its cache, no more
    Then the property "created" of "root" is at least 20
    And the property "created" of "root" is at most 34
    And the object "r40" does not exist

  Scenario: Rows that grow leave fewer delegates
    When the property "grown" of "root" is set to true
    And 200 milliseconds pass
    Then the property "live" of "root" is at least 5
    And the property "live" of "root" is at most 12
