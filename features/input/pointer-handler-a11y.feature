# Sources:
#   https://doc.qt.io/qt-6/qtquickhandlers-index.html
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html
#   docs/ARCHITECTURE.md (section 14)
# Pointer handlers are not items: they add nothing to the accessibility tree
# and take nothing from it. The Qt oracle cannot see the tree through QML
# alone, so these scenarios run on Ljós only.

@ljos-only @a11y
Feature: Pointer handlers and assistive technologies
  An item with pointer handlers is read by a screen reader as it would be
  without them, and a Flickable whose content has handlers still scrolls
  when a screen reader asks.

  Background:
    Given the file "Handled.qml":
      """qml
      import QtQuick
      Rectangle {
        width: 100; height: 40
        Accessible.role: Accessible.Button
        Accessible.name: "Handled"
        TapHandler { }
        DragHandler { }
        HoverHandler { }
        PointHandler { }
        PinchHandler { }
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        width: 300; height: 250
        Handled { }
        Flickable {
          objectName: "f"
          y: 50; width: 150; height: 100
          contentWidth: 150; contentHeight: 400
          Accessible.role: Accessible.List; Accessible.name: "Rows"
          Rectangle {
            width: 150; height: 400
            DragHandler { yAxis.enabled: false }
            TapHandler { }
            Rectangle {
              y: 380; width: 150; height: 20
              Accessible.role: Accessible.Button
              Accessible.name: "Last"
            }
          }
        }
      }
      """

  Scenario: An item with handlers is read as it would be without them
    Then the "button" named "Handled" is inside the "window" named ""

  Scenario: Handlers in a Flickable's content leave a screen reader's scroll alone
    When an assistive technology asks the "list" named "Rows" to scroll down a page
    Then the property "contentY" of "f" is 100
    And the "button" named "Last" is inside the "list" named "Rows"

  Scenario: Hovering and pressing over the handlers keep the item in the tree
    When the user moves the pointer to 50, 20
    And the user presses the pointer at 50, 20
    Then the "button" named "Handled" is inside the "window" named ""
