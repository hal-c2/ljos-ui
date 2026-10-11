# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (header, footer, headerItem, footerItem)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.ListView.header @api-QtQuick.ListView.footer
Feature: The header and the footer of a ListView
  A header goes before the first row and a footer after the last; both scroll with the rows.

  Background:
    Given the QML:
      """qml
      import QtQuick
      ListView {
        id: list
        objectName: "list"
        width: 200; height: 300
        model: 4
        function noHeader() { list.header = null }
        header: Item { objectName: "head"; width: 200; height: 40 }
        footer: Item { objectName: "foot"; width: 200; height: 30 }
        delegate: Item { objectName: "r" + index; width: 200; height: 20 }
      }
      """

  Scenario: The header is above the first row, which stays at zero
    Then the property "y" of "head" is -40
    And the property "y" of "r0" is 0
    And the property "originY" of "list" is -40
    And the property "contentY" of "list" is -40

  Scenario: The footer follows the last row
    Then the property "y" of "foot" is 80
    And the property "contentHeight" of "list" is 150

  Scenario: The view tells which items they are
    Then the property "headerItem.objectName" of "list" is "head"
    And the property "footerItem.objectName" of "list" is "foot"

  Scenario: The footer follows the rows when the model grows
    When the property "model" of "list" is set to 6
    And 100 milliseconds pass
    Then the property "y" of "foot" is 120
    And the property "contentHeight" of "list" is 190

  Scenario: A header that changes its height keeps the first row at zero
    When the property "height" of "head" is set to 60
    And 100 milliseconds pass
    Then the property "y" of "head" is -60
    And the property "y" of "r0" is 0
    And the property "contentHeight" of "list" is 170

  Scenario: Taking the header away destroys its item
    When the method "noHeader" of "list" is called
    And 100 milliseconds pass
    Then the object "head" does not exist
    And the property "headerItem" of "list" is null
    And the property "originY" of "list" is 0
    And the property "contentHeight" of "list" is 110

  Scenario: An empty model still has its header and footer
    When the property "model" of "list" is set to 0
    And 100 milliseconds pass
    Then the property "y" of "head" is -40
    And the property "y" of "foot" is 0
    And the property "contentHeight" of "list" is 70
