# Sources:
#   https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html
#   https://doc.qt.io/qt-6/qaccessibilityhints.html
#   https://doc.qt.io/qt-6/qt.html#MotionPreference-enum
#   docs/ARCHITECTURE.md (section 14, SystemPrefs)
# A Linux desktop says what its user prefers through the settings portal
# (org.freedesktop.portal.Settings): what the portal's own namespace
# (org.freedesktop.appearance) has, and the keys of the desktop's own
# settings it passes on. qml6 6.11 has contrastPreference and no
# motionPreference (Qt documents it for 6.12), and the oracle cannot be
# given a desktop, so the scenarios run on Ljós only. A setting a test or
# the application set itself stays over the desktop's.

@ljos-only @a11y
Feature: The desktop's settings reach the application
  The shell reads the desktop's settings when the application starts and
  hears each change. Less motion, more contrast and larger text, said in
  any of the keys a desktop has, reach Qt.styleHints.accessibility. The
  portal's colour scheme is the system's scheme Qt.styleHints.colorScheme
  reads.

  Rule: Each key a desktop has says what the user prefers

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property bool calm: Qt.styleHints.accessibility.motionPreference
                              === Qt.MotionPreference.ReducedMotion
          property var contrast: Qt.styleHints.accessibility.contrastPreference
          property var scale: Qt.styleHints.accessibility.fontScale
        }
        """

    Scenario Outline: A desktop's key is read
      When the desktop setting "<namespace>" "<key>" changes to <value>
      Then the property "<property>" of "root" is <read>

      Examples:
        | namespace                        | key                   | value           | property | read  |
        | org.freedesktop.appearance       | reduced-motion        | uint32 1        | calm     | true  |
        | org.freedesktop.appearance       | contrast              | uint32 1        | contrast | 1     |
        | org.gnome.desktop.interface      | enable-animations     | false           | calm     | true  |
        | org.gnome.desktop.a11y.interface | reduced-motion        | "reduce"        | calm     | true  |
        | org.gnome.desktop.a11y.interface | reduced-motion        | "no-preference" | calm     | false |
        | org.gnome.desktop.a11y.interface | high-contrast         | true            | contrast | 1     |
        | org.gnome.desktop.interface      | text-scaling-factor   | 1.25            | scale    | 1.25  |
        | org.kde.kdeglobals.KDE           | AnimationDurationFactor | 0.0           | calm     | true  |
        | org.kde.kdeglobals.KDE           | AnimationDurationFactor | 0.5           | calm     | false |

    Scenario: A desktop that turns animations back on is calm no longer
      When the desktop setting "org.gnome.desktop.interface" "enable-animations" changes to false
      And the desktop setting "org.gnome.desktop.interface" "enable-animations" changes to true
      Then the property "calm" of "root" is false

    Scenario: Less motion in any one key is less motion
      When the desktop setting "org.gnome.desktop.a11y.interface" "reduced-motion" changes to "no-preference"
      And the desktop setting "org.gnome.desktop.interface" "enable-animations" changes to false
      Then the property "calm" of "root" is true

    Scenario: A key the application does not read changes nothing
      When the desktop setting "org.gnome.desktop.interface" "cursor-size" changes to 48
      And the desktop setting "org.gnome.desktop.interface" "enable-animations" changes to "not a flag"
      Then the property "calm" of "root" is false
      And the property "contrast" of "root" is 0
      And the property "scale" of "root" is 1

  Rule: The portal's colour scheme is the system's

    # The window system says no scheme on X11, nor on Wayland unless the
    # application chose one, so the portal is where a Linux desktop says it.
    # The portal's 0 is no preference, which desktops show light.

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property var scheme: Qt.styleHints.colorScheme
          function ask(scheme) { Qt.styleHints.colorScheme = scheme }
        }
        """

    Scenario Outline: The portal's colour scheme is read
      When the desktop setting "org.freedesktop.appearance" "color-scheme" changes to <value>
      Then the property "scheme" of "root" is <scheme>

      Examples:
        | value    | scheme |
        | uint32 1 | 2      |
        | uint32 2 | 1      |

    Scenario: A desktop that goes from dark to no preference is light
      When the desktop setting "org.freedesktop.appearance" "color-scheme" changes to uint32 1
      And the desktop setting "org.freedesktop.appearance" "color-scheme" changes to uint32 0
      Then the property "scheme" of "root" is 1

    Scenario: A scheme the application asked for stays when the desktop's changes
      When the method "ask" of "root" is called with 1
      And the desktop setting "org.freedesktop.appearance" "color-scheme" changes to uint32 1
      Then the property "scheme" of "root" is 1

  Rule: What the application set stays over the desktop's

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property bool calm: Qt.styleHints.accessibility.motionPreference
                              === Qt.MotionPreference.ReducedMotion
          property var contrast: Qt.styleHints.accessibility.contrastPreference
          property var scale: Qt.styleHints.accessibility.fontScale
        }
        """

    Scenario: A setting the application made is kept when the desktop's changes
      When the application sets the system setting "reduced motion" to true
      And the desktop setting "org.gnome.desktop.interface" "enable-animations" changes to true
      Then the property "calm" of "root" is true

    Scenario: The other settings still follow the desktop
      When the application sets the system setting "reduced motion" to true
      And the desktop setting "org.gnome.desktop.interface" "text-scaling-factor" changes to 1.5
      Then the property "scale" of "root" is 1.5

    Scenario: A setting the application gives back the desktop's value follows the desktop again
      When the application sets the system setting "reduced motion" to true
      And the application sets the system setting "reduced motion" to false
      And the desktop setting "org.gnome.desktop.interface" "enable-animations" changes to false
      Then the property "calm" of "root" is true

  Rule: What the desktop said before the application started is there at its start

    Scenario: Settings the desktop had before the application started are read
      Given the desktop setting "org.freedesktop.appearance" "contrast" is uint32 1
      And the desktop setting "org.freedesktop.appearance" "color-scheme" is uint32 1
      And the desktop setting "org.gnome.desktop.interface" "text-scaling-factor" is 2.0
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "started"
          property var contrast: Qt.styleHints.accessibility.contrastPreference
          property var scale: Qt.styleHints.accessibility.fontScale
          property var scheme: Qt.styleHints.colorScheme
        }
        """
      Then the property "contrast" of "started" is 1
      And the property "scale" of "started" is 2
      And the property "scheme" of "started" is 2
