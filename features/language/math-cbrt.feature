# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-javascript-functionlist.html
#   https://tc39.es/ecma262/#sec-math.cbrt
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (builtins), bucket value:onChanged:text.

@qt-parity
Feature: Math.cbrt

  # Qt calls the platform's cbrt (glibc's here). Ljós's QuickJS links Rust's compiler_builtins
  # cbrt instead, the only inexact libm function it takes from there, which is an ulp below
  # glibc's for this input; writing the number out shows the ulp.
  @backlog
  Scenario: Math.cbrt(255) matches Qt when written out
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string seen: String(Math.cbrt(255))
      }
      """
    Then the property "seen" of "root" is "6.341325705384998"
