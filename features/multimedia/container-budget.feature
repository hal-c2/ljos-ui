# Sources:
#   https://doc.qt.io/qt-6/qml-qtmultimedia-mediaplayer.html#error-prop
#   docs/ARCHITECTURE.md

@ljos-only
Feature: Whole media containers have a byte budget
  Whole containers fetched from a server are limited to 16 MiB.
  Live picture streams can continue beyond that total.

  Scenario: A streamed container exceeding 16 MiB reports a network error
    Given the web server streams 16777217 bytes at "oversized.mp4"
    And the QML:
      """qml
      import QtMultimedia
      MediaPlayer { objectName: "player"; source: "$SERVER/oversized.mp4" }
      """
    When 1 second passes
    Then the property "error" of "player" is 3
    And the property "errorString" of "player" is "media container exceeds byte budget"
