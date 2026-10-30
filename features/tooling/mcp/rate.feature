# Sources:
#   docs/ARCHITECTURE.md sections 19.6 and 19.7
#   https://www.rfc-editor.org/rfc/rfc6585#section-4 (429 Too Many Requests)
#   https://www.rfc-editor.org/rfc/rfc9110#section-10.2.3 (Retry-After)
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: How many calls a second the inspector answers
  The inspector answers at most so many calls a second for its token, so
  that a runaway agent cannot keep the application busy. A program that
  starts the application as its own child and drives it as part of its
  own interface (a live preview, a test harness) makes several calls for
  each thing a person does, and may say it trusts itself with more.

  Background:
    Given the file "Main.qml":
      """qml
      import QtQuick
      Rectangle { width: 200; height: 100; color: "#3366cc" }
      """

  Rule: The default is 20 calls a second

    Scenario: The 21st call in a second is told when to ask again
      When the author starts "ljos run Main.qml --headless --mcp=inspect"
      And a client makes 21 calls to the started application's inspector within one second
      Then the first 20 calls are answered
      And call 21 is refused with status 429 and "Retry-After: 1"

    Scenario: The token file states the default rate
      When the author starts "ljos run Main.qml --headless --mcp=inspect"
      Then the started application's token file says 20 calls a second

  Rule: Whoever starts the application may grant a higher rate

    Scenario: A higher rate answers the calls the default would refuse
      When the author starts "ljos run Main.qml --headless --mcp=inspect --mcp-rate 100"
      And a client makes 101 calls to the started application's inspector within one second
      Then the first 100 calls are answered
      And call 101 is refused with status 429 and "Retry-After: 1"

    Scenario: The token file states the rate in force
      When the author starts "ljos run Main.qml --headless --mcp=inspect --mcp-rate 100"
      Then the started application's token file says 100 calls a second

    Scenario Outline: A rate outside the range is a usage error that names the range
      When the author runs "ljos run Main.qml --headless --mcp=inspect --mcp-rate <rate>"
      Then the command exits with status 2
      And the command's output mentions "from 1 to 1000"

      Examples:
        | rate |
        | 0    |
        | 1001 |
        | many |

    Scenario: A rate needs the inspector
      When the author runs "ljos run Main.qml --headless --mcp-rate 100"
      Then the command exits with status 2
      And the command's output mentions "--mcp"
