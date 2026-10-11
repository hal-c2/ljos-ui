# Sources:
#   https://doc.qt.io/qt-6/qstylehints.html#colorScheme-prop
#   https://doc.qt.io/qt-6/qaccessibilityhints.html
#   https://doc.qt.io/qt-6/qt.html#MotionPreference-enum
#   docs/ARCHITECTURE.md (section 14, SystemPrefs)
# The oracle cannot change what its platform prefers, so these scenarios run
# on Ljós only. motionPreference and Qt.MotionPreference are documented for
# Qt 6.12. fontScale and screenReaderActive are ours: Qt has no property for
# either.

@ljos-only @a11y
Feature: What the system prefers reaches the application
  The colour scheme, the contrast, reduced motion, the text size and whether a
  screen reader runs are settings of the system. An application reads them
  from Qt.styleHints and its bindings follow when the user changes one.

  Rule: A setting is there when the application starts

    Scenario Outline: A setting the system has is what the application reads
      Given the system setting "<setting>" is <given>
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property var scheme: Qt.styleHints.colorScheme
          property var contrast: Qt.styleHints.accessibility.contrastPreference
          property var motion: Qt.styleHints.accessibility.motionPreference
          property var scale: Qt.styleHints.accessibility.fontScale
          property var reader: Qt.styleHints.accessibility.screenReaderActive
        }
        """
      Then the property "<property>" of "root" is <read>

      Examples:
        | setting        | given   | property | read  |
        | color scheme   | "dark"  | scheme   | 2     |
        | color scheme   | "light" | scheme   | 1     |
        | contrast       | "high"  | contrast | 1     |
        | reduced motion | true    | motion   | 1     |
        | font scale     | 1.5     | scale    | 1.5   |
        | screen reader  | true    | reader   | true  |

    Scenario Outline: A system that says nothing prefers nothing
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property var scheme: Qt.styleHints.colorScheme
          property var contrast: Qt.styleHints.accessibility.contrastPreference
          property var motion: Qt.styleHints.accessibility.motionPreference
          property var scale: Qt.styleHints.accessibility.fontScale
          property var reader: Qt.styleHints.accessibility.screenReaderActive
        }
        """
      Then the property "<property>" of "root" is <read>

      Examples:
        | property | read  |
        | scheme   | 1     |
        | contrast | 0     |
        | motion   | 0     |
        | scale    | 1     |
        | reader   | false |

  Rule: A binding follows a setting that changes

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property var scheme: Qt.styleHints.colorScheme
          property bool dark: Qt.styleHints.colorScheme === Qt.ColorScheme.Dark
          property var contrast: Qt.styleHints.accessibility.contrastPreference
          property bool calm: Qt.styleHints.accessibility.motionPreference
                              === Qt.MotionPreference.ReducedMotion
          property var scale: Qt.styleHints.accessibility.fontScale
          property var reader: Qt.styleHints.accessibility.screenReaderActive
          property var motions: [Qt.MotionPreference.NoPreference, Qt.MotionPreference.ReducedMotion]
          function ask(scheme) { Qt.styleHints.colorScheme = scheme }
        }
        """

    Scenario Outline: A changed setting is read at once
      When the system setting "<setting>" changes to <given>
      Then the property "<property>" of "root" is <read>

      Examples:
        | setting        | given  | property | read |
        | color scheme   | "dark" | scheme   | 2    |
        | color scheme   | "dark" | dark     | true |
        | contrast       | "high" | contrast | 1    |
        | reduced motion | true   | calm     | true |
        | font scale     | 2      | scale    | 2    |
        | screen reader  | true   | reader   | true |

    Scenario Outline: A setting that changes back is read again
      When the system setting "<setting>" changes to <given>
      And the system setting "<setting>" changes to <back>
      Then the property "<property>" of "root" is <read>

      Examples:
        | setting        | given  | back     | property | read  |
        | color scheme   | "dark" | "light"  | scheme   | 1     |
        | contrast       | "high" | "normal" | contrast | 0     |
        | reduced motion | true   | false    | calm     | false |
        | font scale     | 2      | 1        | scale    | 1     |
        | screen reader  | true   | false    | reader   | false |

    Scenario: The motion preferences have Qt's numbers
      Then the property "motions" of "root" is [0, 1]

    Scenario: A scheme the application asked for stays when the system's changes
      When the method "ask" of "root" is called with 1
      And the system setting "color scheme" changes to "dark"
      Then the property "scheme" of "root" is 1

    Scenario: Asking for no scheme gives the system's new scheme
      When the method "ask" of "root" is called with 1
      And the system setting "color scheme" changes to "dark"
      And the method "ask" of "root" is called with 0
      Then the property "scheme" of "root" is 2

    Scenario: A screen reader that starts to listen is a screen reader that runs
      Given an assistive technology is listening
      Then the property "reader" of "root" is true
