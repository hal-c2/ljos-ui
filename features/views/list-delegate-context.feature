# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html
#   https://doc.qt.io/qt-6/qtquick-modelviewsdata-modelview.html (required properties, the model object)
#   https://doc.qt.io/qt-6/qml-qtqml-models-listmodel.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.ListView.delegate
Feature: What a delegate of a ListView can read
  A delegate reads its row through required properties, or through `index`, `model` and the
  role names when it declares none.

  Scenario: Roles, index and model are in the delegate's context
    Given the QML:
      """qml
      import QtQuick
      ListView {
        objectName: "list"
        width: 200; height: 300
        model: ListModel {
          ListElement { name: "ada"; age: 36 }
          ListElement { name: "bo"; age: 7 }
        }
        delegate: Item {
          objectName: "r" + index
          width: 200; height: 20
          property string shown: name + "/" + model.age + "/" + model.index
        }
      }
      """
    Then the property "shown" of "r0" is "ada/36/0"
    And the property "shown" of "r1" is "bo/7/1"

  Scenario: Required properties are filled from the roles of the same name
    Given the QML:
      """qml
      import QtQuick
      ListView {
        objectName: "list"
        width: 200; height: 300
        model: ListModel {
          ListElement { name: "ada"; age: 36 }
          ListElement { name: "bo"; age: 7 }
        }
        delegate: Item {
          required property string name
          required property int index
          required property var model
          objectName: "r" + index
          width: 200; height: 20
          property string shown: name + "/" + model.age
        }
      }
      """
    Then the property "shown" of "r0" is "ada/36"
    And the property "shown" of "r1" is "bo/7"

  Scenario: A required modelData is filled from the value of an array
    Given the QML:
      """qml
      import QtQuick
      ListView {
        objectName: "list"
        width: 200; height: 300
        model: ["x", "y", "z"]
        delegate: Item {
          required property string modelData
          required property int index
          objectName: "r" + index
          width: 200; height: 20
        }
      }
      """
    Then the property "modelData" of "r2" is "z"

  Rule: A delegate follows its row

    Background:
      Given the QML:
        """qml
        import QtQuick
        ListView {
          objectName: "list"
          width: 200; height: 300
          model: lm
          ListModel {
            id: lm
            objectName: "lm"
            ListElement { name: "ada" }
            ListElement { name: "bo" }
            ListElement { name: "cy" }
          }
          delegate: Item {
            required property string name
            required property int index
            objectName: "d-" + name
            width: 200; height: 20
            function rename(to) { lm.setProperty(index, "name", to) }
          }
        }
        """

    Scenario: A changed role reaches the delegate
      When the method "setProperty" of "lm" is called with 1, "name", "bea"
      Then the property "name" of "d-bea" is "bea"
      And the property "index" of "d-bea" is 1
      And the object "d-bo" does not exist

    Scenario: The index of later rows follows an insertion
      When the method "insert" of "lm" is called with 0, {"name": "zed"}
      And 100 milliseconds pass
      Then the property "index" of "d-zed" is 0
      And the property "index" of "d-ada" is 1
      And the property "index" of "d-cy" is 3
      And the property "y" of "d-cy" is 60

    Scenario: The index of later rows follows a removal
      When the method "remove" of "lm" is called with 0
      And 100 milliseconds pass
      Then the object "d-ada" does not exist
      And the property "index" of "d-bo" is 0
      And the property "y" of "d-cy" is 20

    Scenario: A delegate changes its own row
      When the method "rename" of "d-cy" is called with "cleo"
      Then the property "name" of "d-cleo" is "cleo"
