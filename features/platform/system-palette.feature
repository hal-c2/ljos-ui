# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-systempalette.html
#   https://doc.qt.io/qt-6/qpalette.html#ColorRole-enum
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: SystemPalette
  The colours of the system's theme, for an application that draws its own
  controls to match the ones around it. Each colour group (active, inactive,
  disabled) has its own.

  @qt-parity
  Scenario Outline: SystemPalette.<name> is <value>
    Given the QML:
      """qml
      import QtQuick
      Item { objectName: "o"; property var v: SystemPalette.<name> }
      """
    Then the property "v" of "o" is <value>

    Examples:
      | name     | value |
      | Active   | 0     |
      | Disabled | 1     |
      | Inactive | 2     |

  @qt-parity @api-QtQuick.SystemPalette.colorGroup
  Scenario: A palette is the active group's unless it says otherwise
    Given the QML:
      """qml
      import QtQuick
      Item { SystemPalette { objectName: "p" } }
      """
    Then the property "colorGroup" of "p" is 0

  # Qt reads these from the desktop's theme, so the oracle's answer depends
  # on the desktop it runs on. The values are what Qt 6.11.2 gave on a GNOME
  # desktop with the light scheme, which Ljós's light palette matches.
  @ljos-only
  Rule: The light palette

    Scenario Outline: In the <group> group <role> is "<colour>"
      Given the QML:
        """qml
        import QtQuick
        Item { SystemPalette { objectName: "p"; colorGroup: SystemPalette.<group> } }
        """
      Then the property "<role>" of "p" is "<colour>"

      Examples:
        | group    | role            | colour  |
        | Active   | window          | #faf9f8 |
        | Active   | windowText      | #000000 |
        | Active   | base            | #faf9f8 |
        | Active   | alternateBase   | #eae9e8 |
        | Active   | text            | #000000 |
        | Active   | button          | #faf9f8 |
        | Active   | highlight       | #3584e4 |
        | Active   | highlightedText | #ffffff |
        | Active   | placeholderText | #646464 |
        | Active   | dark            | #dbdada |
        | Inactive | windowText      | #929595 |
        | Disabled | text            | #7d7c7c |
        | Disabled | buttonText      | #7d7c7c |

  # What Qt 6.11.2 gave on the same desktop with GTK_THEME=Adwaita:dark.
  @ljos-only
  Rule: The dark palette

    Scenario: A palette is dark when the system prefers dark
      Given the system setting "color scheme" is "dark"
      And the QML:
        """qml
        import QtQuick
        Item { SystemPalette { objectName: "p" } }
        """
      Then the property "window" of "p" is "#323232"
      And the property "windowText" of "p" is "#ffffff"
      And the property "highlight" of "p" is "#15539e"

    Scenario: A palette follows the system to dark while the application runs
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "o"
          SystemPalette { id: p }
          property color fill: p.window
        }
        """
      When the system setting "color scheme" changes to "dark"
      Then the property "fill" of "o" is "#323232"
