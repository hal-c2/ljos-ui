# Sources:
#   https://doc.qt.io/qt-6/qtqml-javascript-hostenvironment.html
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (loaders), bucket throws:Error/none:call: a script that writes a property
# the object a Loader made does not have (see crates/ljos_fuzz/known/loaders.txt).

@qt-parity
Feature: A script writing a property an object does not have
  A QML object has the properties its type and declaration give it and no others: a script
  that writes another is thrown an Error that says the property does not exist, and the
  object is left as it was.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property QtObject o: QtObject { }
        property Item i: Item { }
        Component { id: c; QtObject { } }
        Loader { id: l; sourceComponent: c }
        property string seen
        function write(t) {
          try { t.q = 1; return "kept " + t.q } catch (e) { return e.name + ": " + e.message.replace(/"/g, "'") }
        }
        function run() { seen = write(o) + " | " + write(i) + " | " + write(l.item) }
      }
      """

  @backlog
  Scenario: Each object is thrown an Error, whatever its type and however it was made
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "Error: Cannot assign to non-existent property 'q' | Error: Cannot assign to non-existent property 'q' | Error: Cannot assign to non-existent property 'q'"
