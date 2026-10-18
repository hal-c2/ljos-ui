# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-customize.html#customizing-checkbox
#   https://doc.qt.io/qt-6/qtqml-documents-scope.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The parts Fusion draws its controls with
  `QtQuick.Controls.Fusion.impl` has the parts Fusion's controls are made
  of, and a control that customises one may use them. A part is an item:
  a name it has, written bare in one of its bindings, is its own, not the
  file's root's.

  Scenario: A part's own size is what a bare name in its bindings reads
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      import QtQuick.Controls.Fusion.impl
      Column {
        objectName: "root"
        CheckBox {
          id: check
          text: "Check"
          indicator: CheckIndicator {
            id: tick
            control: check
            y: check.topPadding + (check.availableHeight - height) / 2
          }
        }
        Switch {
          id: toggle
          indicator: SwitchIndicator { id: knob; control: toggle; x: toggle.width - width }
        }
        RadioButton { id: radio; indicator: RadioIndicator { id: ring; control: radio; y: radio.topPadding + (radio.availableHeight - height) / 2 } }
        property bool centred: tick.y === check.topPadding + (check.availableHeight - tick.height) / 2
                               && ring.y === radio.topPadding + (radio.availableHeight - ring.height) / 2
        property bool atTheEnd: knob.x === toggle.width - knob.width
        property bool sized: tick.height > 0 && tick.height < 40 && ring.height > 0 && ring.height < 40 && knob.width > 0 && knob.width < 100
      }
      """
    Then the property "centred" of "root" is true
    And the property "atTheEnd" of "root" is true
    And the property "sized" of "root" is true
    And the log has no warnings
