# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html#labelledBy-prop
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html#labelFor-prop
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html#id-prop
#   docs/ARCHITECTURE.md (section 14)
# The Qt oracle cannot read the accessibility tree through QML alone, so these
# scenarios run on Ljós only.

@ljos-only @a11y
Feature: An item that is the label of another
  A screen reader that comes to a field reads the label the application says
  belongs to it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 300; height: 200
        function point() { mail.Accessible.labelledBy = caption }
        Text {
          id: caption; objectName: "caption"; text: "Your name"
          Accessible.role: Accessible.StaticText; Accessible.labelFor: name
        }
        Rectangle {
          id: name; objectName: "name"
          y: 30; width: 120; height: 30
          Accessible.role: Accessible.EditableText; Accessible.name: "Name"
        }
        Rectangle {
          id: mail; objectName: "mail"
          y: 70; width: 120; height: 30
          Accessible.role: Accessible.EditableText; Accessible.name: "Mail"
          Accessible.id: "mail-field"
        }
      }
      """
    And an assistive technology is listening

  Scenario: The item a label is for is told as labelled by it
    Then the "textinput" named "Name" is labelled by the "label" named "Your name"

  Scenario: An item no label is for is labelled by nothing
    Then the "textinput" named "Mail" is labelled by nothing

  Scenario: An item that names its label is labelled by it
    When the method "point" of "root" is called
    Then the "textinput" named "Mail" is labelled by the "label" named "Your name"

  Scenario: A label that is taken out of the tree labels nothing
    When the property "visible" of "caption" is set to false
    Then the "textinput" named "Name" is labelled by nothing

  Scenario: Accessible.id is the identifier a test finds the item by
    Then the object "mail-field" is exposed as a "textinput" named "Mail"

  Scenario: Without Accessible.id the identifier is the objectName
    Then the object "name" is exposed as a "textinput" named "Name"
