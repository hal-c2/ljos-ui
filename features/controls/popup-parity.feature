# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#parent-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#aboutToShow-signal
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#opened-signal
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#visible-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Popup parent and open order parity

  Scenario: A Popup whose assigned parent Item is destroyed resets its parent property to null
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        id: root; objectName: "root"; width: 200; height: 200
        property int parentChanges: 0
        Item { id: host; objectName: "host"; width: 100; height: 100 }
        Popup {
          id: pop; objectName: "pop"
          parent: host; enter: null; exit: null
          onParentChanged: root.parentChanges++
        }
      }
      """
    When the method "open" of "pop" is called
    And the object "host" is destroyed
    And 50 milliseconds pass
    Then the object "pop" exists
    And the property "parent" of "pop" is null
    And the property "parentChanges" of "root" is 2

  Scenario: Calling open emits aboutToShow while visible is false, then visibleChanged, then opened
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        id: root; objectName: "root"; width: 200; height: 200
        property string log: ""
        Popup {
          id: pop; objectName: "pop"
          enter: null; exit: null
          onAboutToShow: root.log += "show:" + visible + ":" + opened + ";"
          onVisibleChanged: root.log += "vis:" + visible + ";"
          onOpened: root.log += "opened:" + visible + ":" + opened + ";"
        }
      }
      """
    When the method "open" of "pop" is called
    Then the property "log" of "root" is "show:true:true;vis:true;opened:true:true;"

  # Verified on Qt 6.11.2 (qml6, vnc for pointer input).
  @api-QtQuick.Controls.Popup.visible
  Scenario: A closed popup's content does not take a click, and opening makes it hittable
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 400; height: 300
        property bool open: false
        property string last: ""
        Column {
          x: 20; y: 10
          Button { objectName: "save"; width: 80; height: 40; text: "Save"; onClicked: root.last = "save" }
        }
        Popup {
          objectName: "p"; x: 150; y: 100; visible: root.open
          Button { objectName: "ok"; width: 80; height: 40; text: "OK"; onClicked: root.last = "ok" }
        }
      }
      """
    When the user touches 60, 30
    Then the property "last" of "root" is "save"
    When the property "open" of "root" is set to true
    And the user touches 202, 132
    Then the property "last" of "root" is "ok"
