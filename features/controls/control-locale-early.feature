# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#locale-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#locale-prop
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).

@qt-parity @api-QtQuick.Controls.Control.locale @api-QtQuick.Controls.Popup.locale
Feature: A control has a locale while it is being made
  A control's or a popup's `locale` is a locale from the moment it exists,
  not only once it is complete: a Tumbler's delegates are made before the
  control around them is complete, and may already read its locale.

  Scenario: Delegates made before completion find a locale
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        id: root; objectName: "root"
        property string seen: ""
        Control {
          id: c
          contentItem: Tumbler {
            model: 1
            delegate: Text { Component.onCompleted: root.seen += " control:" + typeof c.locale }
          }
        }
        Popup {
          id: p
          contentItem: Tumbler {
            model: 1
            delegate: Text { Component.onCompleted: root.seen += " popup:" + typeof p.locale }
          }
        }
      }
      """
    Then the property "seen" of "root" is " popup:object popup:object control:object control:object"
    And the log has no warnings
