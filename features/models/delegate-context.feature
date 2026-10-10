# Sources:
#   https://doc.qt.io/qt-6/qtquick-modelviewsdata-modelview.html
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: What a delegate sees of its row
  A delegate reads its row through index, model, modelData and the role names, or through
  required properties.

  Rule: Roles, index and model are names in the delegate

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          ListModel {
            id: lm
            objectName: "lm"
            ListElement { name: "a"; cost: 1 }
            ListElement { name: "b"; cost: 2 }
          }
          Repeater {
            model: lm
            Item {
              objectName: "d-" + name
              property int row: index
              property int price: cost
              property int viaModel: model.cost
              property int modelRow: model.index
              property string data: typeof modelData
              function raise() { model.cost = cost + 10 }
              function assign() { cost = 50 }
            }
          }
        }
        """

    Scenario: A delegate reads its row by role name, through model and by index
      Then the property "row" of "d-b" is 1
      And the property "price" of "d-b" is 2
      And the property "viaModel" of "d-b" is 2
      And the property "modelRow" of "d-b" is 1

    Scenario: A ListModel with several roles has no modelData
      Then the property "data" of "d-a" is "undefined"

    Scenario: A binding on a role follows the model
      When the method "setProperty" of "lm" is called with 1, "cost", 7
      Then the property "price" of "d-b" is 7
      And the property "viaModel" of "d-b" is 7

    Scenario: A binding on index follows the row's position
      When the method "insert" of "lm" is called with 0, {"name": "n", "cost": 0}
      Then the property "row" of "d-n" is 0
      And the property "row" of "d-a" is 1
      And the property "modelRow" of "d-b" is 2

    Scenario: Writing through model changes the model
      When the method "raise" of "d-a" is called
      Then the property "price" of "d-a" is 11
      And the property "count" of "lm" is 2

    Scenario: Assigning to a role name changes the model
      When the method "assign" of "d-b" is called
      Then the property "viaModel" of "d-b" is 50

  Rule: Required properties take the place of the injected names

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          ListModel {
            id: lm
            objectName: "lm"
            ListElement { name: "a"; cost: 1 }
            ListElement { name: "b"; cost: 2 }
          }
          Repeater {
            model: lm
            Item {
              required property string name
              required property int index
              objectName: "d-" + name
              property int row: index
              property string others: typeof cost + " " + typeof model + " " + typeof modelData
            }
          }
        }
        """

    Scenario: Required properties are filled from the roles of the same name
      Then the property "name" of "d-b" is "b"
      And the property "index" of "d-b" is 1

    Scenario: Roles that are not required are not names in the delegate
      Then the property "others" of "d-a" is "undefined undefined undefined"

    Scenario: A required property follows the model
      When the method "setProperty" of "lm" is called with 0, "name", "z"
      Then the object "d-z" exists
      And the object "d-a" does not exist

    Scenario: A required index follows the row's position
      When the method "insert" of "lm" is called with 0, {"name": "n", "cost": 0}
      Then the property "row" of "d-n" is 0
      And the property "row" of "d-b" is 2

  Rule: model and modelData can be required too

    Scenario: A required model gives access to every role
      Given the QML:
        """qml
        import QtQuick
        Item {
          ListModel {
            id: lm
            objectName: "lm"
            ListElement { name: "a"; cost: 1 }
          }
          Repeater {
            model: lm
            Item {
              required property var model
              objectName: "d-" + model.name
              property int price: model.cost
              property int row: model.index
              function raise() { model.cost = model.cost + 10 }
            }
          }
        }
        """
      Then the property "price" of "d-a" is 1
      And the property "row" of "d-a" is 0
      When the method "raise" of "d-a" is called
      Then the property "price" of "d-a" is 11

    Scenario: A required modelData is filled from an array
      Given the QML:
        """qml
        import QtQuick
        Item {
          Repeater {
            model: ["p", "q"]
            Item {
              required property var modelData
              required property int index
              objectName: "d-" + modelData
              property string both: modelData + index
            }
          }
        }
        """
      Then the property "both" of "d-q" is "q1"

    Scenario: A required property with no role of that name stops the delegate
      Given the QML:
        """qml
        import QtQuick
        Item {
          Repeater {
            objectName: "rep"
            model: 2
            Item { objectName: "d" + index; required property string missing }
          }
        }
        """
      Then the object "d0" does not exist
      And the log has a warning matching "Required property missing was not initialized"
