# Sources:
#   docs/ARCHITECTURE.md sections 5 (the parser is a tool's, never the runtime's) and 19.3
#   https://doc.qt.io/qt-6/qml-codingconventions.html
#   https://doc.qt.io/qt-6/qtqml-syntax-basics.html
# `ljos fmt` is Ljós's own program: Qt has nothing to compare it with.
# The layout itself is described in mcp/author-format.feature, which asks the same formatter
# through the authoring tools. A file given here is written without the spaces its doc string
# is indented by.

@ljos-only @headless
Feature: Formatting QML from the command line
  `ljos fmt` lays out the QML files it is given, or says what it would
  change without changing it. A script can tell from the exit status
  whether every file is laid out, and a file that does not parse is never
  touched.

  Background:
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item { width: 10 }
      """
    And the file "parts/Label.qml":
      """qml
      import QtQuick

      Text {
          text: "hi"
      }
      """

  Rule: Files are rewritten in place

    Scenario: A file that is not laid out is rewritten
      When the author runs "ljos fmt Main.qml"
      Then the command prints:
        """text
        formatted Main.qml
        1 files, 1 formatted
        """
      And the command exits with status 0
      And the file "Main.qml" is:
        """qml
        import QtQuick

        Item {
            width: 10
        }
        """

    Scenario: A directory stands for every QML file below it
      When the author runs "ljos fmt ."
      Then the command prints:
        """text
        formatted Main.qml
        2 files, 1 formatted
        """
      And the command exits with status 0

    Scenario: A second run finds nothing to do
      Given the author has run "ljos fmt ."
      When the author runs "ljos fmt ."
      Then the command prints:
        """text
        2 files, 0 formatted
        """
      And the command exits with status 0

  Rule: A check changes nothing and fails when a file is not laid out

    Scenario: The files that would change are named
      When the author runs "ljos fmt --check ."
      Then the command prints:
        """text
        would format Main.qml
        2 files, 1 would be formatted
        """
      And the command exits with status 1
      And the file "Main.qml" is:
        """qml
        import QtQuick
        Item { width: 10 }
        """

    Scenario: A check of files that are laid out passes
      When the author runs "ljos fmt --check parts"
      Then the command prints:
        """text
        1 files, 0 would be formatted
        """
      And the command exits with status 0

    Scenario: The change can be shown as a diff
      When the author runs "ljos fmt --diff Main.qml"
      Then the command prints:
        """text
        --- a/Main.qml
        +++ b/Main.qml
        @@ -1,2 +1,5 @@
         import QtQuick
        -Item { width: 10 }
        +
        +Item {
        +    width: 10
        +}
        1 files, 1 would be formatted
        """
      And the command exits with status 0
      And the file "Main.qml" is:
        """qml
        import QtQuick
        Item { width: 10 }
        """

  Rule: A file that does not parse is reported and left alone

    Background:
      Given the file "Broken.qml":
        """qml
        import QtQuick
        Item {
            width: 10 +
        }
        """

    Scenario: The mistake is reported where it is and the other files are still formatted
      When the author runs "ljos fmt ."
      Then the command prints:
        """text
        Broken.qml:4:1: error: Unexpected token
        formatted Main.qml
        3 files, 1 formatted, 1 not parsed
        """
      And the command exits with status 2
      And the file "Broken.qml" is:
        """qml
        import QtQuick
        Item {
            width: 10 +
        }
        """

    Scenario: A file that is not there is an error
      When the author runs "ljos fmt Missing.qml"
      Then the command exits with status 2
      And the command's output mentions "Missing.qml"
