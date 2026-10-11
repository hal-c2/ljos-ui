# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (section group, currentSection, attached section properties)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.ListView.section
Feature: Sections of a ListView
  Rows that follow one another with the same value of a role form a section, and a section
  delegate goes before the first row of each.

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
          ListElement { name: "apple"; kind: "fruit" }
          ListElement { name: "banana"; kind: "fruit" }
          ListElement { name: "carrot"; kind: "root" }
          ListElement { name: "daikon"; kind: "root" }
          ListElement { name: "elder"; kind: "berry" }
        }
        section.property: "kind"
        section.delegate: Item {
          required property string section
          objectName: "s-" + section; width: 200; height: 15
        }
        delegate: Item {
          required property string name
          objectName: name; width: 200; height: 20
        }
      }
      """

  Scenario: A section delegate goes before the first row of each section
    Then the property "y" of "s-fruit" is 0
    And the property "y" of "apple" is 15
    And the property "y" of "banana" is 35
    And the property "y" of "s-root" is 55
    And the property "y" of "carrot" is 70
    And the property "y" of "s-berry" is 110
    And the property "y" of "elder" is 125
    And the property "contentHeight" of "list" is 145

  Scenario Outline: A row knows its section and the ones around it
    Then the property "ListView.section" of "<row>" is "<section>"
    And the property "ListView.previousSection" of "<row>" is "<previous>"
    And the property "ListView.nextSection" of "<row>" is "<next>"

    Examples:
      | row    | section | previous | next  |
      | apple  | fruit   |          | fruit |
      | banana | fruit   | fruit    | root  |
      | carrot | root    | fruit    | root  |
      | elder  | berry   | root     |       |

  Scenario: The view tells the section at its start
    Then the property "currentSection" of "list" is "fruit"
    When the property "contentY" of "list" is set to 0
    Then the property "currentSection" of "list" is "fruit"

  Scenario: A row that moves to another section gets a section delegate
    When the method "setProperty" of "lm" is called with 1, "kind", "herb"
    And 100 milliseconds pass
    Then the property "y" of "s-herb" is 35
    And the property "y" of "banana" is 50
    And the property "ListView.section" of "banana" is "herb"
    And the property "contentHeight" of "list" is 160

  Scenario: Sections by first character
    When the property "section.property" of "list" is set to "name"
    And the property "section.criteria" of "list" is set to 1
    And 100 milliseconds pass
    Then the property "y" of "apple" is 15
    And the property "y" of "banana" is 50
    And the property "y" of "carrot" is 85
    And the property "ListView.section" of "carrot" is "c"
    And the property "currentSection" of "list" is "a"
    And the property "contentHeight" of "list" is 175

  Scenario: Without a section property there are no sections
    When the property "section.property" of "list" is set to ""
    And 100 milliseconds pass
    Then the property "y" of "apple" is 0
    And the property "y" of "banana" is 20
    And the property "ListView.section" of "banana" is ""
    And the property "contentHeight" of "list" is 100
