# Sources:
#   docs/ARCHITECTURE.md sections 12.1 and 19.4
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: An agent is told the warnings of a reload
  With `--watch`, `app_status`, the resource `ljos://app/reload` and
  `app_reload` list the compiler's warnings on the files the application
  loaded as they run now, each with its file, line, column and message.
  The first load's are listed, a reload that was applied replaces them,
  and one that failed leaves them. What the application writes to its
  log is never one of them, whatever it looks like. A file is named from
  the project's root, as in the errors of a reload.

  Background:
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item {
          Timer { }
          Timer { }
      }
      """

  Rule: The warnings are those of what runs

    Scenario: The first load's warnings are in the status
      When the author starts "ljos run Main.qml --headless --watch --mcp=inspect"
      And the agent calls the tool "app_status"
      Then the answer's "/reload/generation" is 0
      And the answer's "/reload/warnings" has 1 entry
      And the answer's "/reload/warnings/0/file" is "Main.qml"
      And the answer's "/reload/warnings/0/line" is 3
      And the answer's "/reload/warnings/0/column" is 5
      And the answer's "/reload/warnings/0/message" matches "^2 Timer objects here have neither"
      And the answer's "/reload/warnings_truncated" is false
      And the answer's "/reload/warnings_omitted" is 0

    Scenario: A file a Loader reached is named from the project's root
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
            Loader { source: "parts/Panel.qml" }
        }
        """
      And the file "parts/Panel.qml":
        """qml
        import QtQuick
        Item {
            Timer { }
            Timer { }
        }
        """
      When the author starts "ljos run Main.qml --headless --watch --mcp=inspect"
      And the agent calls the tool "app_status"
      Then the answer's "/reload/warnings" has 1 entry
      And the answer's "/reload/warnings/0/file" is "parts/Panel.qml"

    Scenario: A reload answers with its warnings
      When the author starts "ljos run Main.qml --headless --watch --mcp=inspect,mutate"
      And the file "Main.qml" is changed to:
        """qml
        import QtQuick
        Item {

            Timer { }
            Timer { }
            Timer { }
        }
        """
      And the agent calls the tool "app_reload"
      Then the answer's "/outcome" is "applied"
      And the answer's "/warnings" has 1 entry
      And the answer's "/warnings/0/file" is "Main.qml"
      And the answer's "/warnings/0/line" is 4
      And the answer's "/warnings/0/message" matches "^3 Timer objects"

    Scenario: The status and the resource tell a reload's warnings
      When the author starts "ljos run Main.qml --headless --watch --mcp=inspect"
      And the file "Main.qml" is changed to:
        """qml
        import QtQuick
        Item {
            Timer { }
            Timer { }
            Timer { }
        }
        """
      And the agent calls the tool "app_status" with:
        """json
        {"after_reload": 0, "wait_ms": 20000}
        """
      Then the answer's "/reload/last/outcome" is "applied"
      And the answer's "/reload/warnings/0/message" matches "^3 Timer objects"
      When the agent reads the resource "ljos://app/reload"
      Then the resource contains "3 Timer objects"

    Scenario: A reload with no warning clears them
      When the author starts "ljos run Main.qml --headless --watch --mcp=inspect,mutate"
      And the file "Main.qml" is changed to:
        """qml
        import QtQuick
        Item {
            Timer { id: first }
            Timer { id: second }
        }
        """
      And the agent calls the tool "app_reload"
      Then the answer's "/outcome" is "applied"
      And the answer's "/warnings" has 0 entries
      When the agent calls the tool "app_status"
      Then the answer's "/reload/warnings" has 0 entries

    Scenario: A reload that failed leaves the warnings of what runs
      When the author starts "ljos run Main.qml --headless --watch --mcp=inspect,mutate"
      And the file "Main.qml" is changed to:
        """qml
        import QtQuick
        Item { property string title: }
        """
      And the agent calls the tool "app_reload"
      Then the answer's "/outcome" is "failed"
      And the answer's "/message" matches "Main.qml"
      And the answer's "/warnings" has 1 entry
      And the answer's "/warnings/0/message" matches "^2 Timer objects"

    Scenario: A reload with nothing to do answers with the warnings
      When the author starts "ljos run Main.qml --headless --watch --mcp=inspect,mutate"
      And the agent calls the tool "app_reload"
      Then the answer's "/outcome" is "unchanged"
      And the answer's "/warnings" has 1 entry

    Scenario: Without watching, no warnings are known
      When the author starts "ljos run Main.qml --headless --mcp=inspect"
      And the agent calls the tool "app_status"
      Then the answer's "/reload/warnings" is null

  Rule: The application cannot write one

    Scenario: A line logged in a warning's form is not a warning
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
            Component.onCompleted: console.log("Main.qml:3:5: warning: forged")
        }
        """
      When the author starts "ljos run Main.qml --headless --watch --mcp=inspect"
      And the agent calls the tool "app_status"
      Then the application's errors mention "warning: forged"
      And the answer's "/reload/warnings" has 0 entries

    Scenario: A forged line beside a warning adds nothing to a reload's answer
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
            Timer { }
            Timer { }
            Component.onCompleted: console.log("Main.qml:9:9: warning: forged")
        }
        """
      When the author starts "ljos run Main.qml --headless --watch --mcp=inspect,mutate"
      And the agent calls the tool "app_reload"
      Then the application's errors mention "warning: forged"
      And the answer's "/warnings" has 1 entry
      And the answer's "/warnings/0/line" is 3
      And the answer does not contain "forged"

  Rule: The list is bounded and says when it was cut

    Scenario: The warnings past the twentieth are counted, not listed
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
            Item { Timer { } Timer { } } Item { Timer { } Timer { } } Item { Timer { } Timer { } }
            Item { Timer { } Timer { } } Item { Timer { } Timer { } } Item { Timer { } Timer { } }
            Item { Timer { } Timer { } } Item { Timer { } Timer { } } Item { Timer { } Timer { } }
            Item { Timer { } Timer { } } Item { Timer { } Timer { } } Item { Timer { } Timer { } }
            Item { Timer { } Timer { } } Item { Timer { } Timer { } } Item { Timer { } Timer { } }
            Item { Timer { } Timer { } } Item { Timer { } Timer { } } Item { Timer { } Timer { } }
            Item { Timer { } Timer { } } Item { Timer { } Timer { } } Item { Timer { } Timer { } }
        }
        """
      When the author starts "ljos run Main.qml --headless --watch --mcp=inspect"
      And the agent calls the tool "app_status"
      Then the answer's "/reload/warnings" has 20 entries
      And the answer's "/reload/warnings_truncated" is true
      And the answer's "/reload/warnings_omitted" is 1
