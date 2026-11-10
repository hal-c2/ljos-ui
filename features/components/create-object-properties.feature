# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#createObject-method
#   https://doc.qt.io/qt-6/qtqml-javascript-dynamicobjectcreation.html
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#required-properties
# Verified on Qt 6.11.2 and 6.12.0 (qml6, offscreen); the scenarios marked
# @qt-diverges say where the two differ and which one Ljós follows.

@qt-parity
Feature: The properties given to Component.createObject
  The second argument of createObject sets properties of the new object
  before it is complete. A property the object does not have is passed
  over. A value it cannot take is reported, and no object is given back.

  Background:
    Given the file "Tag.qml":
      """qml
      import QtQuick
      Item {
        required property string label
        property int n: 1
        property string shown: label + "!"
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property int base: 5
        property bool nothing: false
        property string log: ""
        Component {
          id: tag
          Tag {
            objectName: "tag"
            Component.onCompleted: root.log += "completed;"
            Component.onDestruction: root.log += "destroyed;"
          }
        }
        function make(props) { nothing = tag.createObject(root, props) === null }
        function follow() {
          make({ label: "a", n: Qt.binding(function() { return root.base * 2 }) })
        }
      }
      """

  Scenario: A property given there counts as setting a required property
    When the method "make" of "root" is called with {"label": "new"}
    Then the property "shown" of "root/tag" is "new!"

  Scenario: Without a required property no object is made
    When the method "make" of "root" is called with {"n": 3}
    Then the property "nothing" of "root" is true
    And the object "root/tag" does not exist
    And the log has a warning matching "Required property label was not initialized"

  Scenario: A property the object does not have does not keep it from being made
    When the method "make" of "root" is called with {"label": "a", "nope": 1}
    Then the property "shown" of "root/tag" is "a!"

  # Qt 6.11.2 warns "Setting initial properties failed: Tag does not have a
  # property called nope"; Qt 6.12.0 says nothing, and Ljós follows 6.12.
  @qt-diverges
  Scenario: A property the object does not have is passed over in silence
    When the method "make" of "root" is called with {"label": "a", "nope": 1}
    Then the log has no warning matching "nope"

  Scenario: A value of the wrong type is reported
    When the method "make" of "root" is called with {"label": "a", "n": "many"}
    Then the log has a warning matching "Could not set initial property n"

  # Qt 6.11.2 makes the object with its own value there; Qt 6.12.0 makes it,
  # completes it and destroys it, and gives back null. Ljós follows 6.12.
  @qt-diverges
  Scenario: A value of the wrong type leaves no object
    When the method "make" of "root" is called with {"label": "a", "n": "many"}
    Then the property "nothing" of "root" is true
    And the object "root/tag" does not exist
    And the property "log" of "root" is "completed;destroyed;"

  Scenario: A property given as Qt.binding follows what it reads
    When the method "follow" of "root" is called
    And the property "base" of "root" is set to 6
    Then the property "n" of "root/tag" is 12
