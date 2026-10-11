# Sources:
#   https://doc.qt.io/qt-6/qstylehints.html#colorScheme-prop
#   https://doc.qt.io/qt-6/qaccessibilityhints.html
#   https://doc.qt.io/qt-6/qt.html#ColorScheme-enum
#   https://doc.qt.io/qt-6/qt.html#ContrastPreference-enum
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#styleHints-prop
# Verified on Qt 6.11.2 (qml6, offscreen).
# The oracle's platform has a light scheme and no contrast preference; a
# system that prefers otherwise is in system-settings.feature.

@a11y
Feature: Qt.styleHints tells an application what the user prefers
  An application reads the colour scheme and the contrast preference from
  Qt.styleHints, and may ask for a scheme of its own.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int scheme: Qt.styleHints.colorScheme
        property bool dark: Qt.styleHints.colorScheme === Qt.ColorScheme.Dark
        property int contrast: Qt.styleHints.accessibility.contrastPreference
        property var keys: [Qt.ColorScheme.Unknown, Qt.ColorScheme.Light, Qt.ColorScheme.Dark]
        property var contrasts: [Qt.ContrastPreference.NoPreference, Qt.ContrastPreference.HighContrast]
        property bool same: Qt.styleHints.accessibility === Qt.styleHints.accessibility
        function ask(scheme) { Qt.styleHints.colorScheme = scheme }
        function force() {
          try { Qt.styleHints.accessibility.contrastPreference = 1 } catch (e) { return "refused" }
          return "taken"
        }
        property string forced: ""
        function tryForce() { forced = force() }
      }
      """

  @qt-parity
  Scenario: The schemes have Qt's numbers
    Then the property "keys" of "root" is [0, 1, 2]

  @qt-parity
  Scenario: The contrast preferences have Qt's numbers
    Then the property "contrasts" of "root" is [0, 1]

  @qt-parity
  Scenario: The scheme starts out as the system's
    Then the property "scheme" of "root" is 1
    And the property "dark" of "root" is false

  @qt-parity
  Scenario: Nothing is preferred about contrast until the system says so
    Then the property "contrast" of "root" is 0

  @qt-parity @api-QtQml.Qt.styleHints
  Scenario: A scheme the application asks for is the scheme, and bindings follow
    When the method "ask" of "root" is called with 2
    Then the property "scheme" of "root" is 2
    And the property "dark" of "root" is true

  # Qt 6.11.2 on the oracle's platform (offscreen) keeps the scheme the
  # application asked for: the scheme reads 2 after this. Its documentation
  # says that Qt.ColorScheme.Unknown gives the override up and the system's
  # scheme is followed again, which is what an application with a "follow
  # the system" choice relies on, so that is what happens in Ljós.
  @qt-diverges
  Scenario: Asking for no scheme gives the system's back
    When the method "ask" of "root" is called with 2
    And the method "ask" of "root" is called with 0
    Then the property "scheme" of "root" is 1

  @qt-parity
  Scenario: The contrast preference is the system's to say
    When the method "tryForce" of "root" is called
    Then the property "forced" of "root" is "refused"
    And the property "contrast" of "root" is 0

  @qt-parity
  Scenario: The accessibility hints are one object
    Then the property "same" of "root" is true
