# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#openUrlExternally-method
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: Qt.openUrlExternally
  An application hands a URL to the system, which opens it in the
  application the user chose for it: a link in the browser, a mailto:
  address in the mail program, a file in its viewer. The answer is whether
  the system took it.

  @qt-parity
  Scenario: Qt.openUrlExternally is a function
    Given the QML:
      """qml
      import QtQuick
      Item { objectName: "o"; property string v: typeof Qt.openUrlExternally }
      """
    Then the property "v" of "o" is "function"

  # The oracle would open the link on the machine it runs on.
  @ljos-only
  Scenario: A link is handed to the system
    Given the QML:
      """qml
      import QtQuick
      Item {
          objectName: "o"
          property var answer: "not asked"
          Component.onCompleted: answer = Qt.openUrlExternally("https://example.com/a?b=1")
      }
      """
    Then the system was asked to open "https://example.com/a?b=1"
    And the property "answer" of "o" is false

  @ljos-only
  Scenario: An empty URL is not handed to the system
    Given the QML:
      """qml
      import QtQuick
      Item {
          objectName: "o"
          property var answer: "not asked"
          Component.onCompleted: answer = Qt.openUrlExternally("")
      }
      """
    Then the property "answer" of "o" is false
