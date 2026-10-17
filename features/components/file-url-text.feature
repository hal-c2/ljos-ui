# Sources:
#   https://doc.qt.io/qt-6/qurl.html#toString (QUrl::PrettyDecoded)
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#resolvedUrl-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A file's address as a script sees it
  The address of a file in a directory whose name has a space in it
  keeps the space as it is, as Qt writes a URL for a person to read;
  only what would change what the address means is escaped.

  Scenario: A directory with a space keeps the space in a file's address
    Given the file "my files/Panel.qml":
      """qml
      import QtQuick
      Item { property string here: Qt.resolvedUrl(".") }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string got: loader.item ? loader.item.here.replace(/^.*\/([^\/]*\/)$/, "$1") : ""
        Loader { id: loader; source: "my files/Panel.qml" }
      }
      """
    Then the property "got" of "root" is "my files/"
