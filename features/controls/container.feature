# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-container.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Container
  A container keeps a list of items, shows them in its content item and
  has a current one. The items are the rows of its contentModel, which
  a view in the content item can show.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 400; height: 400
        property string names: ""; property string taken: ""; property bool kept: false
        function list() { names = ""; for (var i = 0; i < box.count; ++i) names += box.itemAt(i).objectName + " " }
        function swap() { box.moveItem(0, 1); list() }
        function take() { var t = box.takeItem(0); taken = t.objectName + "," + (t.parent === null); list() }
        function drop() { box.removeItem(box.itemAt(0)); list() }
        function addOne() { box.addItem(extra); list() }
        function front() { box.insertItem(0, extra); list() }
        function beyond() { names = box.itemAt(9) + " " + box.itemAt(-1) }
        function fill() { empty.addItem(extra) }
        function inRow() { kept = a.parent === box.contentItem && box.contentModel.count === 2 }
        Container {
          id: box; objectName: "box"
          contentItem: Row { }
          Rectangle { id: a; objectName: "a"; width: 10; height: 10 }
          Rectangle { id: b; objectName: "b"; width: 20; height: 10 }
          QtObject { objectName: "plain" }
        }
        Rectangle { id: extra; objectName: "extra"; width: 5; height: 5 }
        Container { id: empty; objectName: "empty" }
      }
      """

  @api-QtQuick.Controls.Container.count @api-QtQuick.Controls.Container.contentChildren
  Scenario: The items declared in a container are its items
    Then the property "count" of "box" is 2
    And the property "contentChildren.length" of "box" is 2
    And the property "contentData.length" of "box" is 3

  @api-QtQuick.Controls.Container.contentModel
  Scenario: The items are in the content item and are the rows of the model
    When the method "inRow" of "root" is called
    Then the property "kept" of "root" is true

  @api-QtQuick.Controls.Container.currentIndex @api-QtQuick.Controls.Container.currentItem
  Scenario: The first item is the current one
    Then the property "currentIndex" of "box" is 0
    And the property "currentItem.objectName" of "box" is "a"

  @api-QtQuick.Controls.Container.contentWidth
  Scenario: A container is as big as its content item asks
    Then the property "contentWidth" of "box" is 30
    And the property "contentHeight" of "box" is 10

  Scenario: An empty container has no current item
    Then the property "count" of "empty" is 0
    And the property "currentIndex" of "empty" is -1
    And the property "contentWidth" of "empty" is 0

  @api-QtQuick.Controls.Container.moveItem
  Scenario: Moving the current item keeps it current
    When the method "swap" of "root" is called
    Then the property "names" of "root" is "b a "
    And the property "currentIndex" of "box" is 1

  @api-QtQuick.Controls.Container.takeItem
  Scenario: A taken item leaves the container
    When the method "take" of "root" is called
    Then the property "taken" of "root" is "a,true"
    And the property "names" of "root" is "b "
    And the property "currentIndex" of "box" is 0
    And the property "currentItem.objectName" of "box" is "b"

  @api-QtQuick.Controls.Container.removeItem
  Scenario: Removing the current item makes the next one current
    When the method "drop" of "root" is called
    Then the property "names" of "root" is "b "
    And the property "currentIndex" of "box" is 0
    And the property "currentItem.objectName" of "box" is "b"

  @api-QtQuick.Controls.Container.addItem
  Scenario: An added item is the last one
    When the method "addOne" of "root" is called
    Then the property "names" of "root" is "a b extra "
    And the property "currentIndex" of "box" is 0

  @api-QtQuick.Controls.Container.insertItem
  Scenario: An item inserted before the current one keeps the index
    When the method "front" of "root" is called
    Then the property "names" of "root" is "extra a b "
    And the property "currentIndex" of "box" is 0
    And the property "currentItem.objectName" of "box" is "extra"

  @api-QtQuick.Controls.Container.itemAt
  Scenario: There is no item outside the list
    When the method "beyond" of "root" is called
    Then the property "names" of "root" is "null null"

  @api-QtQuick.Controls.Container.incrementCurrentIndex
  Scenario: The current index goes forward and stops at the last item
    When the method "incrementCurrentIndex" of "box" is called
    And the method "incrementCurrentIndex" of "box" is called
    Then the property "currentIndex" of "box" is 1

  @api-QtQuick.Controls.Container.decrementCurrentIndex
  Scenario: The current index goes back and stops at the first item
    When the method "decrementCurrentIndex" of "box" is called
    Then the property "currentIndex" of "box" is 0

  @api-QtQuick.Controls.Container.setCurrentIndex
  Scenario: The current index is set by a method
    When the method "setCurrentIndex" of "box" is called with 1
    Then the property "currentItem.objectName" of "box" is "b"

  Scenario: An item added to an empty container is current
    When the method "fill" of "root" is called
    Then the property "count" of "empty" is 1
    And the property "currentIndex" of "empty" is 0
