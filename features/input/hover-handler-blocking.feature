# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-hoverhandler.html#blocking-prop
# A blocking HoverHandler keeps the items and handlers behind it from being
# hovered at the same time. The oracle cannot observe it: qml6 stops
# answering once the pointer is over a blocking HoverHandler (the step times
# out after 20 s on Qt 6.11.2), so these run on Ljós only.

@ljos-only
Feature: A blocking HoverHandler
  A HoverHandler with blocking set takes the hover for itself; what is
  behind it is not hovered while the pointer is over its item.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 400; height: 400
        Rectangle {
          x: 250; y: 40; width: 100; height: 100
          HoverHandler {
            objectName: "below"
          }
          Rectangle {
            width: 100; height: 50
            HoverHandler {
              objectName: "top"
              blocking: true
            }
          }
        }
      }
      """

  @api-QtQuick.HoverHandler.blocking
  Scenario: A blocking handler keeps the hover from the items below
    When the user moves the pointer to 260, 70
    Then the property "hovered" of "top" is true
    And the property "hovered" of "below" is false

  @api-QtQuick.HoverHandler.blocking
  Scenario: Beside the blocking item, the item below is hovered
    When the user moves the pointer to 260, 120
    Then the property "hovered" of "top" is false
    And the property "hovered" of "below" is true
