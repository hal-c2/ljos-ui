# Sources:
#   https://doc.qt.io/qt-6/qtqml-javascript-hostenvironment.html
#   https://doc.qt.io/qt-6/qtquick-debugging.html
#   https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/ReferenceError
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: An exception is logged with the name of its kind
  What the log says of an exception nothing caught starts with the file
  and the line, then the name of the error and its message. A name that
  nothing declares is a ReferenceError a script can catch like any other;
  asking for the type of such a name is not an error.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string seen: ""
        property string kind: typeof nowhere
        property string run: ""
        onRunChanged: root[run]()
        function plain() { throw new Error("boom") }
        function typed() { throw new TypeError("wrong kind") }
        function ranged() { throw new RangeError("too far") }
        function reads() { return absent + 1 }
        function calls() { seen += "in "; reads(); seen += "out " }
        function caught() {
          try { absent + 1 } catch (e) {
            seen = (e instanceof ReferenceError) + "|" + e.name + "|" + e.message
          }
        }
        function asked() { seen = typeof absent + "|" + (typeof absent === "undefined") }
        function nulled() { var nothing = null; return nothing.size }
        function writes() { seen += "in "; unheard = 5; seen += "out " }
      }
      """

  Scenario: A thrown Error keeps its name
    When the property "run" of "root" is set to "plain"
    Then the log has a warning matching "Scenario.qml:9: Error: boom"

  Scenario: A thrown TypeError keeps its name
    When the property "run" of "root" is set to "typed"
    Then the log has a warning matching "Scenario.qml:10: TypeError: wrong kind"

  Scenario: A thrown RangeError keeps its name
    When the property "run" of "root" is set to "ranged"
    Then the log has a warning matching "Scenario.qml:11: RangeError: too far"

  Scenario: A function that reads an unknown name is logged at its own line
    When the property "run" of "root" is set to "calls"
    Then the log has a warning matching "Scenario.qml:12: ReferenceError: absent is not defined"
    And the property "seen" of "root" is "in "

  Scenario: Reading a property of null is a TypeError
    When the property "run" of "root" is set to "nulled"
    Then the log has a warning matching "Scenario.qml:20: TypeError: Cannot read property 'size' of null"

  Scenario: Assigning to an unknown name is an error
    When the property "run" of "root" is set to "writes"
    Then the log has a warning matching "Scenario.qml:21: Error: Invalid write to global property .unheard."
    And the property "seen" of "root" is "in "

  Scenario: A script catches the ReferenceError
    When the method "caught" of "root" is called
    Then the property "seen" of "root" is "true|ReferenceError|absent is not defined"
    And the log has no warnings

  Scenario: The type of an unknown name is undefined in a function
    When the method "asked" of "root" is called
    Then the property "seen" of "root" is "undefined|true"
    And the log has no warnings

  Scenario: The type of an unknown name is undefined in a binding
    Then the property "kind" of "root" is "undefined"
    And the log has no warnings
