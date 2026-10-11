# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#textDocument-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textdocument.html#modified-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The document of a TextEdit
  A TextEdit has a document, which says whether the text was changed since
  a program last said it is saved.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 200; height: 120
        property bool hasDocument: edit.textDocument !== null
        property bool modified: edit.textDocument.modified
        function saved() { edit.textDocument.modified = false }
        function touched() { edit.textDocument.modified = true }
        TextEdit {
          id: edit; objectName: "edit"; anchors.fill: parent; focus: true
          text: "abcd"
        }
      }
      """
    When 100 milliseconds pass

  Scenario: A TextEdit has a document
    Then the property "hasDocument" of "root" is true

  Scenario: The document is not modified at first
    Then the property "modified" of "root" is false

  Scenario: Typing modifies the document
    When the user types "x"
    Then the property "modified" of "root" is true

  Scenario: A program that saved the text says so
    When the user types "x"
    And the method "saved" of "root" is called
    Then the property "modified" of "root" is false

  Scenario: A program says the document is modified
    When the method "touched" of "root" is called
    Then the property "modified" of "root" is true

  Scenario: Text a program inserts modifies the document
    When the method "insert" of "edit" is called with 0, "x"
    Then the property "modified" of "root" is true

  Scenario: Text a program removes modifies the document
    When the method "remove" of "edit" is called with 0, 1
    Then the property "modified" of "root" is true

  Scenario: A text a program assigns is not a modification
    When the property "text" of "edit" is set to "other"
    Then the property "modified" of "root" is false

  Scenario: Assigning a text makes a modified document unmodified
    When the user types "x"
    And the property "text" of "edit" is set to "other"
    Then the property "modified" of "root" is false

  Scenario: Undoing the only change leaves the document unmodified
    When the user types "x"
    And the method "undo" of "edit" is called
    Then the property "modified" of "root" is false

  Scenario: Undoing past where the text was saved modifies the document
    When the user types "x"
    And the method "saved" of "root" is called
    And the method "undo" of "edit" is called
    Then the property "modified" of "root" is true

  Scenario: Redoing back to where the text was saved leaves it unmodified
    When the user types "x"
    And the method "saved" of "root" is called
    And the method "undo" of "edit" is called
    And the method "redo" of "edit" is called
    Then the property "modified" of "root" is false
