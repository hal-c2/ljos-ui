# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-binding.html#restoreMode-prop
#   https://doc.qt.io/qt-6/qml-qtquick-propertychanges.html
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#implicitWidth-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A size that followed the implicit size, under a Binding or a state
  An item given no size is as large as its implicit size. A Binding or a
  state that takes the size writes it, as a size given to the item: what
  comes back when it lets go is a value, and the item no longer follows its
  implicit size.

  Rule: A Binding on the size gives back the size the item had, or keeps its own

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property bool on: false
          property real k: 30
          component Sized: Item { implicitWidth: root.k }
          Sized { id: plain; objectName: "plain"; Binding on width { when: root.on; value: 120 } }
          Sized { id: value; objectName: "value"; Binding on width { when: root.on; value: 120; restoreMode: Binding.RestoreValue } }
          Sized { id: bound; objectName: "bound"; Binding on width { when: root.on; value: 120; restoreMode: Binding.RestoreBinding } }
          Sized { id: none; objectName: "none"; Binding on width { when: root.on; value: 120; restoreMode: Binding.RestoreNone } }
        }
        """

    @api-QtQml.Binding.restoreMode
    Scenario Outline: "<object>" is <width> wide once the Binding lets go, whatever the implicit width does then
      When the property "on" of "root" is set to true
      Then the property "width" of "<object>" is 120
      When the property "on" of "root" is set to false
      Then the property "width" of "<object>" is <width>
      When the property "k" of "root" is set to 50
      Then the property "width" of "<object>" is <width>
      And the property "implicitWidth" of "<object>" is 50

      Examples: The size it had comes back as a value
        | object | width |
        | plain  | 30    |
        | value  | 30    |

      Examples: There was no binding to put back, so the Binding's value stays
        | object | width |
        | bound  | 120   |
        | none   | 120   |

  Rule: A state that changed the size gives back the size the item had

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property real k: 30
          Item {
            id: it; objectName: "it"
            implicitWidth: root.k
            states: State { name: "wide"; PropertyChanges { target: it; width: 120 } }
          }
        }
        """

    @api-QtQuick.PropertyChanges
    Scenario: The item keeps the size it had when the state began
      When the property "state" of "it" is set to "wide"
      Then the property "width" of "it" is 120
      When the property "state" of "it" is set to ""
      Then the property "width" of "it" is 30
      When the property "k" of "root" is set to 50
      Then the property "width" of "it" is 30
      And the property "implicitWidth" of "it" is 50

    Scenario: An item never put in the state goes on following its implicit size
      When the property "k" of "root" is set to 50
      Then the property "width" of "it" is 50
