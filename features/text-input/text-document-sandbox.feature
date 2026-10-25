# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textdocument.html
#   docs/ARCHITECTURE.md (18.4: isolated acceptance scenarios)

@ljos-only
Feature: Document files stay inside the harness sandbox
  A sandbox gives file URLs a private root. Traversal is denied rather
  than reaching the host filesystem, including when URL components are encoded.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int status: edit.textDocument.status
        property bool denied: edit.textDocument.errorString.indexOf("sandbox") >= 0
        function open(url) { edit.textDocument.source = url }
        function store(url) { edit.textDocument.saveAs(url) }
        function save() { edit.textDocument.save() }
        TextEdit { id: edit; text: "private text" }
      }
      """

  Scenario Outline: Reading an escaping file URL is denied
    When the method "open" of "root" is called with "<url>"
    And 100 milliseconds pass
    Then the property "status" of "root" is 5
    And the property "denied" of "root" is true

    Examples:
      | url                                    |
      | file:///../outside.txt                 |
      | file:///a//../../outside.txt           |
      | file:///%2e%2e/outside.txt              |
      | file:///a/%2E%2E/%2e%2e/outside.txt     |
      | file:////outside.txt                   |

  Scenario Outline: Writing an escaping file URL is denied
    When the method "store" of "root" is called with "<url>"
    And 100 milliseconds pass
    Then the property "status" of "root" is 6
    And the property "denied" of "root" is true

    Examples:
      | url                                    |
      | file:///../outside.txt                 |
      | file:///a//../../outside.txt           |
      | file:///%2e%2e/outside.txt              |
      | file:////outside.txt                   |

  Scenario: Saving an escaping source is denied
    When the method "open" of "root" is called with "file:///../outside.txt"
    And 100 milliseconds pass
    And the method "save" of "root" is called
    And 100 milliseconds pass
    Then the property "status" of "root" is 6
    And the property "denied" of "root" is true

  Scenario: Repeated separators within a confined URL preserve normal reads
    Given the text file "docs/notes.txt":
      """
      inside
      """
    When the method "open" of "root" is called with "file:///docs//notes.txt"
    And 100 milliseconds pass
    Then the property "status" of "root" is 2
    And the property "denied" of "root" is false

  Scenario: Repeated separators within a confined URL preserve normal writes
    Given the text file "docs/notes.txt":
      """
      inside
      """
    When the method "store" of "root" is called with "file:///docs//copy.txt"
    And 100 milliseconds pass
    Then the property "status" of "root" is 4
    And the text file "docs/copy.txt" has the line "private text"
