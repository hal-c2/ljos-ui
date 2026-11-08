# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#visible-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#enabled-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menu.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: An item that a binding shows shows what is inside it
  An item whose `visible` binding is false when it is made hides what is
  inside it. When the binding turns true later, what is inside reads
  visible again, at once, whatever else the same change made run: a
  binding of an item that no window shows yet, such as one in a closed
  menu, does not keep them hidden.

  # Seen first in a toolbar shown once a view came: the Flow showed, but
  # every control in it read not visible and the Flow kept a height of 0.
  # A closed menu in it had an item whose `enabled` read the same value.
  Scenario: A Flow that a binding shows shows its children while a closed menu in it reads the same value
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        id: page; objectName: "page"
        width: 400; height: 300
        property var plans: ({})
        readonly property var saved: plans.e ?? null
        property var working: null
        readonly property string environment: working?.environment ?? "e"
        readonly property var items: plans[environment]?.items ?? []
        onSavedChanged: working = saved
        function work() { plans = { e: { items: [] } } }
        Flow {
          objectName: "flow"; width: 400
          visible: page.working !== null
          Item { objectName: "kid"; width: 10; height: 10 }
          Item {
            objectName: "holder"
            width: 10; height: 10
            Menu {
              MenuItem { text: "Clear"; enabled: page.items.length > 0 }
            }
          }
        }
      }
      """
    Then the property "visible" of "kid" is false
    When the method "work" of "page" is called
    Then the property "visible" of "flow" is true
    And the property "visible" of "kid" is true
    And the property "visible" of "holder" is true
    And the property "height" of "flow" is 10
    And the log has no warnings

  Scenario: An item that its binding shows shows its children
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property bool wanted: false
        Item {
          objectName: "box"
          visible: root.wanted
          Item {
            objectName: "kid"
            Item { objectName: "grandkid" }
          }
        }
      }
      """
    Then the property "visible" of "grandkid" is false
    When the property "wanted" of "root" is set to true
    Then the property "visible" of "box" is true
    And the property "visible" of "kid" is true
    And the property "visible" of "grandkid" is true
    And the log has no warnings
