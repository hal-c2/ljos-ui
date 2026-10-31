# Sources:
#   docs/ARCHITECTURE.md (section 12.1, the theme file)
#   docs/adr/0003-blocks-and-shells.md (section 4, shells and skins)
#   https://www.w3.org/TR/WCAG22/#contrast-minimum
#   https://www.w3.org/TR/WCAG22/#non-text-contrast
# A theme file is a Ljós extension: Qt has nothing to compare it with.

@ljos-only
Feature: A theme file sets the roles of Ljos.Design
  A `theme.json` beside the application's QML gives the roles of
  `Colors`, `Spacing`, `Typography` and `Motion` other values, without a
  change to the QML. A colour may have a value for light and one for
  dark. While the files are watched an edit of the theme sets the roles
  again and makes nothing again, so what the user did is kept. A theme
  that cannot be read, names a role the module does not have, or makes
  text too faint to read is refused, and the last theme that could be
  read stays.

  Scenario: A theme sets a colour from the start
    Given the theme file "theme.json" is:
      """json
      { "Colors": { "accent": "#9c4dc4" } }
      """
    And the QML:
      """qml
      import QtQuick
      import Ljos.Design
      Item {
        objectName: "root"
        property color accent: Colors.accent
        property color canvas: Colors.canvas
      }
      """
    Then the property "accent" of "root" is "#9c4dc4"
    And the property "canvas" of "root" is "#f8f9fa"
    And there is no reload error

  Scenario Outline: A colour has a value for light and one for dark
    Given the system setting "color scheme" is <scheme>
    And the theme file "theme.json" is:
      """json
      { "Colors": { "accent": { "light": "#6a1b9a", "dark": "#9c4dc4" } } }
      """
    And the QML:
      """qml
      import QtQuick
      import Ljos.Design
      Item {
        objectName: "root"
        property color accent: Colors.accent
      }
      """
    Then the property "accent" of "root" is "<accent>"

    Examples:
      | scheme | accent  |
      | light  | #6a1b9a |
      | dark   | #9c4dc4 |

  Scenario: A colour follows the colour scheme when it changes
    Given the theme file "theme.json" is:
      """json
      { "Colors": { "accent": { "light": "#6a1b9a", "dark": "#9c4dc4" } } }
      """
    And the QML:
      """qml
      import QtQuick
      import Ljos.Design
      Item {
        objectName: "root"
        property color accent: Colors.accent
      }
      """
    When the system setting "color scheme" changes to dark
    Then the property "accent" of "root" is "#9c4dc4"

  Scenario: High contrast without a value of the theme's keeps the module's own
    Given the system setting "contrast" is "high"
    And the theme file "theme.json" is:
      """json
      { "Colors": { "accent": "#9c4dc4" } }
      """
    And the QML:
      """qml
      import QtQuick
      import Ljos.Design
      Item {
        objectName: "root"
        property color accent: Colors.accent
      }
      """
    Then the property "accent" of "root" is "#005e8c"

  Scenario: Spacing and type sizes are set too
    Given the theme file "theme.json" is:
      """json
      {
        "Spacing": { "md": 20 },
        "Typography": { "bodySize": 15, "bodyLineHeight": 24 }
      }
      """
    And the QML:
      """qml
      import QtQuick
      import Ljos.Design
      Item {
        objectName: "root"
        property int gap: Spacing.md
        property int small: Spacing.sm
        property int body: Typography.bodySize
        property int line: Typography.bodyLineHeight
      }
      """
    Then the property "gap" of "root" is 20
    And the property "small" of "root" is 12
    And the property "body" of "root" is 15
    And the property "line" of "root" is 24

  Scenario: An edit of the theme sets the roles again and keeps what the user did
    Given the theme file "theme.json" is:
      """json
      { "Colors": { "accent": "#9c4dc4" } }
      """
    And the QML:
      """qml
      import QtQuick
      import Ljos.Design
      Item {
        objectName: "root"
        property color accent: Colors.accent
        property int presses: 0
      }
      """
    When the property "presses" of "root" is set to 3
    And the theme file "theme.json" is changed to:
      """json
      { "Colors": { "accent": "#2e7d32" } }
      """
    And 120 milliseconds pass
    Then the property "accent" of "root" is "#2e7d32"
    And the property "presses" of "root" is 3
    And 0 reloads have happened
    And the reload generation is 1
    And there is no reload error

  Scenario: An edit of the theme waits for the quiet time
    Given the theme file "theme.json" is:
      """json
      { "Colors": { "accent": "#9c4dc4" } }
      """
    And the QML:
      """qml
      import QtQuick
      import Ljos.Design
      Item {
        objectName: "root"
        property color accent: Colors.accent
      }
      """
    When the theme file "theme.json" is changed to:
      """json
      { "Colors": { "accent": "#2e7d32" } }
      """
    And 100 milliseconds pass
    Then the property "accent" of "root" is "#9c4dc4"

  Scenario: A theme that cannot be read keeps the last one and says where
    Given the theme file "theme.json" is:
      """json
      { "Colors": { "accent": "#9c4dc4" } }
      """
    And the QML:
      """qml
      import QtQuick
      import Ljos.Design
      Item {
        objectName: "root"
        property color accent: Colors.accent
      }
      """
    When the theme file "theme.json" is changed to:
      """json
      {
        "Colors": {
          "accent": "#2e7d32",
        }
      }
      """
    And a reload happens
    Then the property "accent" of "root" is "#9c4dc4"
    And the reload error is in "theme.json" at line 4
    And the reload generation is 0
    When the theme file "theme.json" is changed to:
      """json
      { "Colors": { "accent": "#2e7d32" } }
      """
    And a reload happens
    Then the property "accent" of "root" is "#2e7d32"
    And there is no reload error

  Scenario Outline: A role the module does not have, or a value of the wrong kind, is refused
    Given the QML:
      """qml
      import QtQuick
      import Ljos.Design
      Item {
        objectName: "root"
        property color accent: Colors.accent
      }
      """
    When the theme file "theme.json" is changed to:
      """json
      {
        "Colors": { <entry> }
      }
      """
    And a reload happens
    Then the reload error matches "<error>"
    And the reload error is in "theme.json" at line 2
    And the property "accent" of "root" is "#007cb7"

    Examples:
      | entry               | error                                 |
      | "acent": "#6a1b9a"  | Colors has no role .acent.            |
      | "accent": 12        | Colors.accent is a colour             |
      | "accent": "#6a1b9z" | Colors.accent: .#6a1b9z. is no colour |
      | "dark": true        | Colors.dark is the user's to choose   |

  Scenario: A colour that makes text too faint to read is refused
    Given the QML:
      """qml
      import QtQuick
      import Ljos.Design
      Item {
        objectName: "root"
        property color text: Colors.text
      }
      """
    When the theme file "theme.json" is changed to:
      """json
      { "Colors": { "text": "#c0c0c0" } }
      """
    And a reload happens
    Then the reload error matches "Colors.text on canvas is 1.\d+:1 in light; text needs 4.5:1"
    And the reload error is in "theme.json" at line 1
    And the property "text" of "root" is "#242729"

  Scenario: Removing the theme gives the module's own values back
    Given the theme file "theme.json" is:
      """json
      { "Colors": { "accent": "#9c4dc4" }, "Spacing": { "md": 20 } }
      """
    And the QML:
      """qml
      import QtQuick
      import Ljos.Design
      Item {
        objectName: "root"
        property color accent: Colors.accent
        property int gap: Spacing.md
      }
      """
    When the theme file "theme.json" is removed
    And a reload happens
    Then the property "accent" of "root" is "#007cb7"
    And the property "gap" of "root" is 16
    And the reload generation is 1

  # Tier 1 runs the binding without the theme's override on the role: the
  # read it keeps must be one the override is heard through. The cross-check
  # is off because it runs the binding again in tier 2, which would hide a
  # read tier 1 did not keep.
  Scenario: A role the theme first sets while the files are watched updates its readers
    Given the tier 1 cross-check is off
    And the QML:
      """qml
      import QtQuick
      import Ljos.Design
      Item {
        objectName: "root"
        property string gap: "gap " + Spacing.md
        property color accent: Colors.accent
      }
      """
    When the theme file "theme.json" is changed to:
      """json
      { "Colors": { "accent": "#9c4dc4" }, "Spacing": { "md": 24 } }
      """
    And a reload happens
    Then the property "gap" of "root" is "gap 24"
    And the property "accent" of "root" is "#9c4dc4"
    When the theme file "theme.json" is changed to:
      """json
      { "Spacing": { "md": 28 } }
      """
    And a reload happens
    Then the property "gap" of "root" is "gap 28"
    And the property "accent" of "root" is "#007cb7"
    And 0 reloads have happened

  # The cross-check compares what the two tiers read as well as what they
  # give: a read only one of them keeps is a difference it reports.
  Scenario: With the tier 1 cross-check on, both tiers read a role alike
    Given the tier 1 cross-check is on
    And the QML:
      """qml
      import QtQuick
      import Ljos.Design
      Item {
        objectName: "root"
        property string gap: "gap " + Spacing.md
      }
      """
    When the theme file "theme.json" is changed to:
      """json
      { "Spacing": { "md": 24 } }
      """
    And a reload happens
    Then the property "gap" of "root" is "gap 24"
    And the log has no warning matching "tier 1 and tier 2 differ"

  Scenario: An edit of the QML keeps the theme
    Given the theme file "theme.json" is:
      """json
      { "Colors": { "accent": "#9c4dc4" } }
      """
    And the QML:
      """qml
      import QtQuick
      import Ljos.Design
      Item {
        objectName: "root"
        property color accent: Colors.accent
      }
      """
    When the file "Scenario.qml" is changed to:
      """qml
      import QtQuick
      import Ljos.Design
      Item {
        objectName: "root"
        property color accent: Colors.accent
        property color fill: Colors.accent
      }
      """
    And a reload happens
    Then the property "fill" of "root" is "#9c4dc4"
    And 1 reload has happened
