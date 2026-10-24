# Sources:
#   https://doc.qt.io/qt-6/qml-url.html
#   https://doc.qt.io/qt-6/qtqml-typesystem-valuetypes.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A url as a script sees it
  A property of type url keeps the address as it was written. A script
  that reads it gets a value that turns into that text, and assigning
  null to it is refused.

  Scenario: A url turns into the text it was given
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property url address: "http://example.org/a b"
        property url relative: "pictures/dot.png"
        property url none
        property string seen: address + "|" + relative + "|[" + none + "]"
        property bool alike: address == "http://example.org/a b"
      }
      """
    Then the property "seen" of "root" is "http://example.org/a b|pictures/dot.png|[]"
    And the property "alike" of "root" is true

  Scenario: null is not an address
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property url address: "http://example.org/"
        property string said
        function clear() {
          try { address = null } catch (e) { said = String(e) }
          said += "|" + address
        }
      }
      """
    When the method "clear" of "root" is called
    Then the property "said" of "root" is "Error: Cannot assign std::nullptr_t to QUrl|http://example.org/"

  # On Ljós a script gets the address as a string: `typeof` says "string"
  # and it is strictly equal to the same text.
  @backlog
  Scenario: A url is an object to a script, not a string
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property url address: "http://example.org/"
        property string kind: typeof address
        property bool same: address === "http://example.org/"
      }
      """
    Then the property "kind" of "root" is "object"
    And the property "same" of "root" is false

  # Found by ljos-fuzz (bindings), bucket value:url:string-as-url.
  @backlog
  Scenario Outline: A url given <text>, which is no address, is empty
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property url address: <text>
        property string seen: address
      }
      """
    Then the property "seen" of "root" is ""

    Examples:
      | text           |
      | ("0" + ":0")   |
      | ("a b" + ":c") |
