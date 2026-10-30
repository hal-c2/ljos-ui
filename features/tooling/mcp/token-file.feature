# Sources:
#   docs/ARCHITECTURE.md section 19.7
#   https://modelcontextprotocol.io/specification/2025-11-25/basic/security_best_practices
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: Where the inspector keeps its token
  The token gives whoever reads it the application's inspector, with every
  capability it was granted. So the directory the token file goes in is the
  user's alone, as the file is, from the moment each exists. A directory
  that somebody else can look in is not used: the application says which
  one and what to do about it rather than hand the token over.

  Background:
    Given the file "Main.qml":
      """qml
      import QtQuick
      Rectangle { width: 200; height: 100; color: "#3366cc" }
      """

  Scenario: A directory the inspector makes is its user's alone
    When the author starts "ljos run Main.qml --headless --mcp=inspect"
    Then the project's directory "target/ljos" has the mode 700
    And the started application's token file has the mode 600

  Scenario: A directory that is already the user's alone is used
    Given the project's directory "target/ljos" was made with the mode 700
    When the author starts "ljos run Main.qml --headless --mcp=inspect"
    Then the started application's token file has the mode 600

  Scenario Outline: A directory others can get into is refused and left as it was
    Given the project's directory "target/ljos" was made with the mode <mode>
    When the author runs "ljos run Main.qml --headless --mcp=inspect"
    Then the command exits with status 1
    And the command's output mentions "target/ljos"
    And the command's output mentions "chmod 700"
    And the project's directory "target/ljos" has the mode <mode>
    And the project's directory "target/ljos" holds no file

    Examples:
      | mode |
      | 755  |
      | 775  |
      | 770  |
      | 701  |

  # That the directory is another user's cannot be arranged here: only root
  # can give one away. `check_own_directory`'s unit test asks it as another uid.
  Scenario: A link where the directory should be is refused and nothing goes where it leads
    Given the project's directory "elsewhere" was made with the mode 700
    And the project's directory "target/ljos" is a link to "elsewhere"
    When the author runs "ljos run Main.qml --headless --mcp=inspect"
    Then the command exits with status 1
    And the command's output mentions "target/ljos"
    And the command's output mentions "link"
    And the project's directory "elsewhere" holds no file
