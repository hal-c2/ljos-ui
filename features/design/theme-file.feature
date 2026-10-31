# Sources:
#   docs/ARCHITECTURE.md (section 12.1, the theme file)
#   docs/theme.md (the theme file's contract)
# A theme file is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: Where an application's theme file is found
  An application written in QML alone needs no Rust for a theme:
  `ljos run`, `ljos render` and `ljos audit` read `theme.json` themselves
  and set the roles of `Ljos.Design` before the root is made. The file is
  looked for in the entry's directory, then in each directory above it up
  to the project's (the entry's directory, or `--project`), and the
  nearest one is read, so a module's own theme is nearer its entry than
  the project's. Nothing above the project is read. `--theme FILE` is read
  in place of any of them. A run that watches its files reads the same
  one.

  Background:
    Given the file "app/Main.qml":
      """qml
      import QtQuick
      import Ljos.Design
      Item {
        width: 40; height: 40
        Component.onCompleted: {
          console.warn("accent " + Colors.accent)
          Qt.quit()
        }
      }
      """

  Rule: The nearest theme from the entry up to the project is read

    Scenario: The theme beside the entry is read
      Given the file "app/theme.json":
        """json
        { "Colors": { "accent": "#9c4dc4" } }
        """
      When the author runs "ljos run app/Main.qml --headless"
      Then the command's output mentions "accent #9c4dc4"

    Scenario: The project's theme is read for an entry below it
      Given the file "theme.json":
        """json
        { "Colors": { "accent": "#2e7d32" } }
        """
      When the author runs "ljos run app/Main.qml --project . --headless"
      Then the command's output mentions "accent #2e7d32"

    Scenario: The theme nearest the entry wins
      Given the file "app/theme.json":
        """json
        { "Colors": { "accent": "#9c4dc4" } }
        """
      And the file "theme.json":
        """json
        { "Colors": { "accent": "#2e7d32" } }
        """
      When the author runs "ljos run app/Main.qml --project . --headless"
      Then the command's output mentions "accent #9c4dc4"

    Scenario: Nothing above the project is read
      Given the file "theme.json":
        """json
        { "Colors": { "accent": "#2e7d32" } }
        """
      When the author runs "ljos run app/Main.qml --headless"
      Then the command's output mentions "accent #007cb7"

    Scenario: A theme named on the command line is read in place of the others
      Given the file "app/theme.json":
        """json
        { "Colors": { "accent": "#9c4dc4" } }
        """
      And the file "brand.json":
        """json
        { "Colors": { "accent": "#2e7d32" } }
        """
      When the author runs "ljos run app/Main.qml --theme brand.json --headless"
      Then the command's output mentions "accent #2e7d32"

  Rule: Every way of running the application reads the same theme

    Scenario: A run that watches its files reads the theme nearest the entry
      Given the file "app/theme.json":
        """json
        { "Colors": { "accent": "#9c4dc4" } }
        """
      And the file "theme.json":
        """json
        { "Colors": { "accent": "#2e7d32" } }
        """
      When the author runs "ljos run app/Main.qml --project . --watch --headless"
      Then the command's output mentions "accent #9c4dc4"

    Scenario: A render reads the theme
      Given the file "theme.json":
        """json
        { "Colors": { "accent": "#2e7d32" } }
        """
      When the author runs "ljos render . --entry app/Main.qml --out shot.png --settle 0"
      Then the command's output mentions "accent #2e7d32"

    Scenario: An audit reads the theme
      Given the file "app/theme.json":
        """json
        { "Colors": { "text": "#c0c0c0" } }
        """
      When the author runs "ljos audit app/Main.qml --matrix current"
      Then the command's output mentions "Colors.text on canvas is"
