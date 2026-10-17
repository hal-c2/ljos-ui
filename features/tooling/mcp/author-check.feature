# Sources:
#   docs/ARCHITECTURE.md section 19.3
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html
# The authoring tools are Ljós's own: Qt has nothing to compare them with.
# A file given here is written without the spaces its doc string is indented by, so the
# first column of a line is 1. A name one of Qt's types lacks is an error where the build
# has Qt's table (`@qt-table`), as Qt fails to load the file and the compiler stops it; a
# build without it reads Qt's types against Ljós's own table (`@no-qt-table`), which may
# lack what Qt has, so there it is a warning, as the compiler's is.

@ljos-only @headless
Feature: Checking QML with the authoring tools
  `qml_check` tells an agent what is wrong with a file before anything
  runs it, each finding with its file, line, column, severity and code.
  A name that is no member of the type it is written on is reported as
  the compiler and `ljos check` report it, as surely as they do. What
  only compiling finds is reported too, and what both find, once.

  Background:
    Given the authoring tools are connected
    And the file "Main.qml":
      """qml
      import QtQuick
      Item {
          widht: 100
          onWidhtChanged: console.log("never")
          Rectangle { border.widht: 2 }
      }
      """

  Rule: A name the type does not have is found where it is written

    @qt-table
    Scenario Outline: A name one of Qt's types lacks is an error
      When the agent calls the tool "qml_check" with:
        """json
        {"paths": ["Main.qml"]}
        """
      Then the answer's "/diagnostics" has 3 entries
      And the answer's "/diagnostics/<nth>/file" is "Main.qml"
      And the answer's "/diagnostics/<nth>/line" is <line>
      And the answer's "/diagnostics/<nth>/column" is <column>
      And the answer's "/diagnostics/<nth>/severity" is "error"
      And the answer's "/diagnostics/<nth>/code" is "<code>"
      And the answer's "/diagnostics/<nth>/message" matches "<message>"

      Examples:
        | nth | line | column | code             | message                                                  |
        | 0   | 3    | 5      | unknown-property | `widht` is not a property of `Item`; did you mean `width` |
        | 1   | 4    | 5      | unknown-signal   | did you mean `onWidthChanged`                            |
        | 2   | 5    | 17     | unknown-property | `widht` is not a property of `border`                    |

    @no-qt-table
    Scenario Outline: Read against Ljós's own table, a name it does not list is a warning
      When the agent calls the tool "qml_check" with:
        """json
        {"paths": ["Main.qml"]}
        """
      Then the answer's "/diagnostics" has 3 entries
      And the answer's "/diagnostics/<nth>/line" is <line>
      And the answer's "/diagnostics/<nth>/column" is <column>
      And the answer's "/diagnostics/<nth>/severity" is "warning"
      And the answer's "/diagnostics/<nth>/code" is "<code>"

      Examples:
        | nth | line | column | code             |
        | 0   | 3    | 5      | unknown-property |
        | 1   | 4    | 5      | unknown-signal   |
        | 2   | 5    | 17     | unknown-property |

    Scenario: A name a module only Ljós describes lacks is a warning
      Given the file "tst_case.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
            name: "Case"
            wen: true
        }
        """
      When the agent calls the tool "qml_check" with:
        """json
        {"paths": ["tst_case.qml"]}
        """
      Then the answer's "/diagnostics" has 1 entries
      And the answer's "/diagnostics/0/line" is 5
      And the answer's "/diagnostics/0/column" is 5
      And the answer's "/diagnostics/0/severity" is "warning"
      And the answer's "/diagnostics/0/message" matches "did you mean `when`"

    Scenario: What Ljós adds to Qt's types is no finding
      Given the file "Added.qml":
        """qml
        import QtQuick
        Item {
            Text { font.scaleWithSystem: true }
            Component.onReloaded: console.log("again")
        }
        """
      When the agent calls the tool "qml_check" with:
        """json
        {"paths": ["Added.qml"]}
        """
      Then the answer's "/diagnostics" has 0 entries

    Scenario: A property's change handler is a member whatever its signal is called
      Given the file "Hover.qml":
        """qml
        import QtQuick
        MouseArea {
            hoverEnabled: true
            onContainsMouseChanged: console.log(containsMouse)
        }
        """
      When the agent calls the tool "qml_check" with:
        """json
        {"paths": ["Hover.qml"]}
        """
      Then the answer's "/diagnostics" has 0 entries

  Rule: What the compiler finds is reported as `ljos check` reports it

    Scenario: A value a property cannot hold is found by compiling the file
      Given the file "Align.qml":
        """qml
        import QtQuick
        Text {
            horizontalAlignment: "left"
        }
        """
      When the agent calls the tool "qml_check" with:
        """json
        {"paths": ["Align.qml"]}
        """
      Then the answer's "/diagnostics" has 1 entries
      And the answer's "/diagnostics/0/file" is "Align.qml"
      And the answer's "/diagnostics/0/line" is 3
      And the answer's "/diagnostics/0/column" is 26
      And the answer's "/diagnostics/0/severity" is "error"
      And the answer's "/diagnostics/0/code" is "compiler"
      And the answer's "/diagnostics/0/message" is "Invalid property assignment: unknown enumeration"

    Scenario: What the compiler and the front end both find is reported once
      Given the file "Twice.qml":
        """qml
        import QtQuick
        Item {
            id: a
            Rectanglez { }
            Item { id: a }
        }
        """
      When the agent calls the tool "qml_check" with:
        """json
        {"paths": ["Twice.qml"]}
        """
      Then the answer's "/diagnostics" has 2 entries
      And the answer's "/diagnostics/0/code" is "unknown-type"
      And the answer's "/diagnostics/1/code" is "duplicate-id"
