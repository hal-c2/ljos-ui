# Sources:
#   https://developer.android.com/reference/android/view/ViewConfiguration#getScaledTouchSlop()
#   https://doc.qt.io/qt-6/qml-qtquick-tableview.html#resizableColumns-prop
#   features/views/table-view-resize.feature (the same drags at the default 10)
# A TableView's own drags start from the platform's drag distance too:
# the end of a column follows the pointer from the first move past it.
@ljos-only
Feature: A TableView's drags start at the platform's drag distance
  When the system says a drag starts at 8 pixels, a press on the end of
  a column a TableView can resize starts resizing it at the first move
  more than 8 pixels away, not 10.

  Background:
    Given the system setting "drag distance" is 8
    And the QML:
      """qml
      import QtQuick; import Qt.labs.qmlmodels
      Item {
        id: root; objectName: "root"; width: 400; height: 300
        property string out: ""
        TableView {
          id: tv; width: 300; height: 200
          resizableColumns: true
          model: TableModel { TableModelColumn { display: "a" } TableModelColumn { display: "b" }
            rows: [{ a: 0, b: 0 }, { a: 0, b: 0 }] }
          delegate: Rectangle { implicitWidth: 100; implicitHeight: 30 }
        }
        function explicit() { out = tv.explicitColumnWidth(0) }
      }
      """

  Scenario: The end of a column follows the pointer from the first move past the system's distance
    When 50 milliseconds pass
    And the user moves the pointer to 99, 15
    And the user moves the pointer to 100, 15
    And the user presses the pointer at 100, 15
    And the user moves the pointer to 104, 15
    And the user moves the pointer to 109, 15
    And the user moves the pointer to 120, 15
    And the user releases the pointer at 120, 15
    And the method "explicit" of "root" is called
    Then the property "out" of "root" is "111"
