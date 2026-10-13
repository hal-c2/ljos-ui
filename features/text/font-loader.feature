# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-fontloader.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#fontFamilies-method
# Verified on Qt 6.11.2 (qml6, offscreen).
# The font files are in features/text/fonts (Noto Serif Test, see
# LICENSE-Noto.txt there).

@qt-parity
Feature: FontLoader
  A font file the application brings with it is loaded by URL. Its family
  name is then usable by any text, and the loader says how loading went.

  @api-QtQuick.FontLoader.status
  Scenario Outline: FontLoader.<name> is <value>
    Given the QML:
      """qml
      import QtQuick
      Item { objectName: "o"; property var v: FontLoader.<name> }
      """
    Then the property "v" of "o" is <value>

    Examples:
      | name    | value |
      | Null    | 0     |
      | Ready   | 1     |
      | Loading | 2     |
      | Error   | 3     |

  @api-QtQuick.FontLoader.name
  Scenario: A loaded font file names its family
    Given the font file "NotoSerifTest-Regular.ttf"
    And the QML:
      """qml
      import QtQuick
      Item { FontLoader { objectName: "f"; source: "NotoSerifTest-Regular.ttf" } }
      """
    Then the property "status" of "f" is 1
    And the property "name" of "f" is "Noto Serif Test"

  @api-QtQuick.FontLoader.font
  Scenario: The loaded font is the family at its own weight
    Given the font file "NotoSerifTest-Bold.ttf"
    And the QML:
      """qml
      import QtQuick
      Item { FontLoader { objectName: "f"; source: "NotoSerifTest-Bold.ttf" } }
      """
    Then the property "font.family" of "f" is "Noto Serif Test"
    And the property "font.bold" of "f" is true
    And the property "font.weight" of "f" is 700

  Scenario: Text names the loaded family
    Given the font file "NotoSerifTest-Regular.ttf"
    And the QML:
      """qml
      import QtQuick
      Item {
        FontLoader { id: loader; source: "NotoSerifTest-Regular.ttf" }
        Text { objectName: "t"; text: "a"; font.family: loader.name }
      }
      """
    Then the property "font.family" of "t" is "Noto Serif Test"

  @api-QtQuick.FontLoader.source
  Scenario: A loader without a source has loaded nothing
    Given the QML:
      """qml
      import QtQuick
      Item { FontLoader { objectName: "f" } }
      """
    Then the property "status" of "f" is 0
    And the property "name" of "f" is ""

  Scenario: A file that is not there is an error
    Given the QML:
      """qml
      import QtQuick
      Item { FontLoader { objectName: "f"; source: "missing.ttf" } }
      """
    Then the property "status" of "f" is 3
    And the property "name" of "f" is ""
    And the log has a warning matching "QML FontLoader: Cannot load font: .*missing\.ttf."

  Scenario: A file that is not a font is an error
    Given a file "bad.ttf" that is not a font
    And the QML:
      """qml
      import QtQuick
      Item { FontLoader { objectName: "f"; source: "bad.ttf" } }
      """
    Then the property "status" of "f" is 3
    And the log has a warning matching "Cannot load font: .*bad\.ttf."

  Scenario: A font is loaded when the source is set later
    Given the font file "NotoSerifTest-Regular.ttf"
    And the QML:
      """qml
      import QtQuick
      Item { FontLoader { objectName: "f" } }
      """
    When the property "source" of "f" is set to "NotoSerifTest-Regular.ttf"
    Then the property "status" of "f" is 1
    And the property "name" of "f" is "Noto Serif Test"

  # A test run has no network; the address refuses at once.
  Scenario: A font from the network is loading first, then an error when it cannot be had
    Given the QML:
      """qml
      import QtQuick
      Item { FontLoader { objectName: "f"; source: "http://127.0.0.1:9/x.ttf" } }
      """
    Then the property "status" of "f" is 2
    When 500 milliseconds pass
    Then the property "status" of "f" is 3
    And the log has a warning matching "Cannot load font: .http://127\.0\.0\.1:9/x\.ttf."

  @api-QtQml.Qt.fontFamilies
  Scenario: The loaded family is among Qt.fontFamilies()
    Given the font file "NotoSerifTest-Regular.ttf"
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "o"
        FontLoader { id: loader; source: "NotoSerifTest-Regular.ttf" }
        property bool listed: loader.status === FontLoader.Ready
                              && Qt.fontFamilies().indexOf("Noto Serif Test") >= 0
      }
      """
    Then the property "listed" of "o" is true
