# Sources:
#   https://doc.qt.io/qt-6/qml-qtmultimedia-mediaplayer.html#error-prop
#   docs/ARCHITECTURE.md

@ljos-only
Feature: Incomplete media pictures have a byte budget
  A picture that never ends fails once its pending data exceeds 16 MiB.
  Completed pictures are delivered before a later terminal budget error,
  regardless of transport chunk boundaries, including live delivery.
  JPEG scans and H264 access units retain their framing and exact budgets.

  Scenario Outline: An incomplete <kind> picture reports an error
    Given the web server streams 16777217 zero bytes after hex "<prefix>" at "<file>"
    And the QML:
      """qml
      import QtMultimedia
      MediaPlayer { objectName: "player"; source: "$SERVER/<file>" }
      """
    When 1 second passes
    Then the property "errorString" of "player" is "media unit exceeds byte or NAL budget"

    Examples:
      | kind | prefix       | file         |
      | JPEG | ffd8ffda0002 | endless.mjpeg |
      | H264 | 0000016701   | endless.h264 |
