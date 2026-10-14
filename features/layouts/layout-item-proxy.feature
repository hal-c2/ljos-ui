# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layoutitemproxy.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: LayoutItemProxy
  A LayoutItemProxy stands in for a target Item inside a layout. When the
  proxy is effectively visible it reparents the target to itself, turns the
  target visible, and keeps the target at (0, 0) with the proxy's width and
  height. Implicit sizes and Layout attached properties on the target are
  forwarded to the proxy unless overridden on the proxy itself, so the same
  target can live in alternative layouts that switch by visibility.

  Scenario: An empty proxy has a null target and zero implicit size
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      Item {
        width: 200; height: 100
        LayoutItemProxy { objectName: "proxy" }
      }
      """
    Then the property "target" of "proxy" is null
    And the property "implicitWidth" of "proxy" is 0
    And the property "implicitHeight" of "proxy" is 0

  Scenario: Implicit size follows the target and updates when the target changes
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      Item {
        id: root; objectName: "root"
        width: 200; height: 100
        Rectangle { id: box; objectName: "box"; implicitWidth: 45; implicitHeight: 35 }
        LayoutItemProxy { objectName: "proxy"; target: box }
        function grow() { box.implicitWidth = 80; box.implicitHeight = 60 }
      }
      """
    Then the property "implicitWidth" of "proxy" is 45
    And the property "implicitHeight" of "proxy" is 35
    When the method "grow" of "root" is called
    Then the property "implicitWidth" of "proxy" is 80
    And the property "implicitHeight" of "proxy" is 60

  Scenario: Target Layout properties are forwarded and proxy overrides win
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      ColumnLayout {
        id: root; objectName: "root"
        width: 300; height: 200; spacing: 10
        Rectangle {
          id: box; objectName: "box"; implicitWidth: 60; implicitHeight: 30
          Layout.minimumWidth: 100; Layout.minimumHeight: 40; Layout.fillWidth: true
          Layout.margins: 8; Layout.bottomMargin: 0
        }
        LayoutItemProxy {
          id: proxy; objectName: "proxy"; target: box
          Layout.minimumHeight: 70
        }
        Rectangle {
          objectName: "filler"; implicitWidth: 50; implicitHeight: 20
          Layout.fillWidth: true; Layout.fillHeight: true
        }
      }
      """
    When 100 milliseconds pass
    Then the property "x" of "proxy" is 8
    And the property "y" of "proxy" is 8
    And the property "width" of "proxy" is 284
    And the property "height" of "proxy" is 70
    And the property "x" of "box" is 0
    And the property "y" of "box" is 0
    And the property "width" of "box" is 284
    And the property "height" of "box" is 70
    And the property "visible" of "box" is true
    And the property "y" of "filler" is 88
    And the property "height" of "filler" is 112

  Scenario: Switching visibility between two layouts transfers the target between proxies
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      Item {
        id: root; objectName: "root"; width: 300; height: 480
        property bool isPortrait: width < height
        Rectangle { id: display; objectName: "display"; implicitWidth: 80; implicitHeight: 60
          Layout.minimumWidth: 120; Layout.fillWidth: true; Layout.fillHeight: true; Layout.margins: 8; Layout.bottomMargin: 0 }
        Rectangle { id: pad; objectName: "pad"; implicitWidth: 180; implicitHeight: 220; Layout.margins: 8 }
        ColumnLayout {
          id: portrait; anchors.fill: parent; spacing: 0; visible: root.isPortrait
          LayoutItemProxy { id: pDisplay; objectName: "pDisplay"; target: display; Layout.minimumHeight: 110 }
          LayoutItemProxy { id: pPad; objectName: "pPad"; target: pad; Layout.alignment: Qt.AlignHCenter }
        }
        RowLayout {
          id: landscape; anchors.fill: parent; spacing: 0; visible: !root.isPortrait
          LayoutItemProxy { id: lDisplay; objectName: "lDisplay"; target: display }
          LayoutItemProxy { id: lPad; objectName: "lPad"; target: pad; Layout.alignment: Qt.AlignVCenter }
        }
        property bool displayInPortrait: display.parent === pDisplay; property bool displayInLandscape: display.parent === lDisplay
        property bool displayOrphaned: display.parent === null; property bool displayVisible: display.visible
        function toLandscape() { root.width = 600 }
        function hideBoth() { portrait.visible = false; landscape.visible = false }
        function showPortrait() { portrait.visible = true }
      }
      """
    When 100 milliseconds pass
    Then the property "displayInPortrait" of "root" is true
    And the property "displayInLandscape" of "root" is false
    And the property "visible" of "display" is true
    And the property "width" of "display" is 284
    And the property "height" of "display" is 236
    And the property "x" of "pPad" is 60
    And the property "y" of "pPad" is 252
    When the method "toLandscape" of "root" is called
    And 100 milliseconds pass
    Then the property "displayInPortrait" of "root" is false
    And the property "displayInLandscape" of "root" is true
    And the property "visible" of "display" is true
    And the property "width" of "display" is 388
    And the property "height" of "display" is 472
    And the property "x" of "lPad" is 412
    And the property "y" of "lPad" is 130
    When the method "hideBoth" of "root" is called
    And 100 milliseconds pass
    Then the property "displayVisible" of "root" is false
    And the property "displayOrphaned" of "root" is true
    When the method "showPortrait" of "root" is called
    And 100 milliseconds pass
    Then the property "displayInPortrait" of "root" is true
    And the property "displayVisible" of "root" is true

  Scenario: Retargeting a proxy sizes the new target
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      Item {
        id: root; objectName: "root"; width: 240; height: 120
        Rectangle { id: first; objectName: "first"; implicitWidth: 40; implicitHeight: 25 }
        Rectangle { id: second; objectName: "second"; implicitWidth: 90; implicitHeight: 55 }
        LayoutItemProxy { id: proxy; objectName: "proxy"; width: 140; height: 75; target: first }
        property bool firstOwned: first.parent === proxy
        property bool secondOwned: second.parent === proxy
        function switchTarget() { proxy.target = second }
      }
      """
    When 100 milliseconds pass
    Then the property "firstOwned" of "root" is true
    And the property "width" of "first" is 140
    And the property "height" of "first" is 75
    When the method "switchTarget" of "root" is called
    And 100 milliseconds pass
    Then the property "secondOwned" of "root" is true
    And the property "implicitWidth" of "proxy" is 90
    And the property "implicitHeight" of "proxy" is 55
    And the property "width" of "second" is 140
    And the property "height" of "second" is 75
    # The old target is not given back: it stays a visible child of the
    # proxy, at the size it had.
    And the property "firstOwned" of "root" is true
    And the property "visible" of "first" is true
    And the property "width" of "first" is 140

  Scenario: Clearing the target leaves the item where it is
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      Item {
        id: root; objectName: "root"; width: 240; height: 120
        Rectangle { id: box; objectName: "box"; implicitWidth: 40; implicitHeight: 25 }
        LayoutItemProxy { id: proxy; objectName: "proxy"; width: 140; height: 75; target: box }
        property bool boxOwned: box.parent === proxy
        function clear() { proxy.target = null }
      }
      """
    When 100 milliseconds pass
    Then the property "boxOwned" of "root" is true
    When the method "clear" of "root" is called
    And 100 milliseconds pass
    Then the property "boxOwned" of "root" is true
    And the property "visible" of "box" is true
    # The proxy keeps the size of the target it lost.
    And the property "implicitWidth" of "proxy" is 40

  Scenario: A proxy given a target that a visible proxy holds does not take it
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        Rectangle { id: shared; objectName: "shared"; implicitWidth: 30; implicitHeight: 20 }
        LayoutItemProxy { id: first; objectName: "first"; width: 80; height: 60; target: shared }
        LayoutItemProxy { id: second; objectName: "second"; width: 120; height: 90 }
        property bool ownedByFirst: shared.parent === first
        property bool ownedBySecond: shared.parent === second
        function point() { second.target = shared }
        function hideFirst() { first.visible = false }
      }
      """
    When 100 milliseconds pass
    Then the property "ownedByFirst" of "root" is true
    When the method "point" of "root" is called
    And 100 milliseconds pass
    Then the property "ownedByFirst" of "root" is true
    And the property "width" of "shared" is 80
    When the method "hideFirst" of "root" is called
    And 100 milliseconds pass
    Then the property "ownedBySecond" of "root" is true
    And the property "width" of "shared" is 120

  Scenario: A target whose proxies are all hidden stays where it is, hidden, until one is shown
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        Rectangle { id: box; objectName: "box"; implicitWidth: 30; implicitHeight: 20 }
        LayoutItemProxy { id: first; objectName: "first"; width: 10; height: 10; target: box; visible: false }
        Item {
          id: hidden; visible: false
          LayoutItemProxy { id: second; objectName: "second"; width: 50; height: 40; target: box }
        }
        property bool atRoot: box.parent === root
        property bool ownedBySecond: box.parent === second
        function showHidden() { hidden.visible = true }
      }
      """
    When 100 milliseconds pass
    Then the property "atRoot" of "root" is true
    And the property "visible" of "box" is false
    When the method "showHidden" of "root" is called
    And 100 milliseconds pass
    Then the property "ownedBySecond" of "root" is true
    And the property "visible" of "box" is true
    And the property "width" of "box" is 50
    And the property "height" of "box" is 40

  Scenario: A proxy does not pull back a target moved elsewhere until it is shown again
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        Rectangle { id: box; objectName: "box"; implicitWidth: 30; implicitHeight: 20 }
        LayoutItemProxy { id: proxy; objectName: "proxy"; width: 80; height: 60; target: box }
        property bool atRoot: box.parent === root
        property bool owned: box.parent === proxy
        function moveAway() { box.parent = root; box.x = 7; box.width = 5 }
        function resizeProxy() { proxy.width = 100 }
        function toggle() { proxy.visible = false; proxy.visible = true }
      }
      """
    When 100 milliseconds pass
    Then the property "owned" of "root" is true
    When the method "moveAway" of "root" is called
    And 100 milliseconds pass
    Then the property "atRoot" of "root" is true
    And the property "x" of "box" is 7
    And the property "width" of "box" is 5
    When the method "toggle" of "root" is called
    And 100 milliseconds pass
    Then the property "owned" of "root" is true
    And the property "x" of "box" is 0
    And the property "width" of "box" is 80
    When the method "resizeProxy" of "root" is called
    Then the property "width" of "box" is 100

  Scenario: Attached Layout properties on the proxy reflect the target unless set on the proxy
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      RowLayout {
        id: root; objectName: "root"; width: 300; height: 100; spacing: 0
        Rectangle {
          id: box; objectName: "box"; implicitWidth: 30; implicitHeight: 20
          Layout.minimumWidth: 50; Layout.preferredWidth: 70; Layout.maximumWidth: 180
          Layout.minimumHeight: 25; Layout.margins: 6; Layout.bottomMargin: 12
        }
        LayoutItemProxy {
          id: proxy; objectName: "proxy"; target: box
          Layout.minimumHeight: 45; Layout.topMargin: 10
        }
        Rectangle { objectName: "rest"; implicitWidth: 20; implicitHeight: 20; Layout.fillWidth: true; Layout.fillHeight: true }
        property real proxyMinW: proxy.Layout.minimumWidth
        property real proxyPrefW: proxy.Layout.preferredWidth
        property real proxyMaxW: proxy.Layout.maximumWidth
        property real proxyMinH: proxy.Layout.minimumHeight
        property real proxyMargins: proxy.Layout.margins
        property real proxyTopMargin: proxy.Layout.topMargin
        property real proxyBottomMargin: proxy.Layout.bottomMargin
        function updateTarget() { box.Layout.preferredWidth = 95 }
      }
      """
    When 100 milliseconds pass
    Then the property "proxyMinW" of "root" is 50
    And the property "proxyPrefW" of "root" is 70
    And the property "proxyMaxW" of "root" is 180
    And the property "proxyMinH" of "root" is 45
    And the property "proxyMargins" of "root" is 6
    And the property "proxyTopMargin" of "root" is 10
    And the property "proxyBottomMargin" of "root" is 12
    And the property "width" of "proxy" is 70
    And the property "height" of "proxy" is 45
    When the method "updateTarget" of "root" is called
    And 100 milliseconds pass
    Then the property "proxyPrefW" of "root" is 95
    And the property "width" of "proxy" is 95
    And the property "width" of "box" is 95

  Scenario: Hiding the active proxy hands the target to another visible proxy
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        Rectangle { id: shared; objectName: "shared"; implicitWidth: 30; implicitHeight: 20 }
        LayoutItemProxy { id: first; objectName: "first"; width: 80; height: 60; target: shared }
        LayoutItemProxy { id: second; objectName: "second"; width: 120; height: 90; target: shared }
        property bool ownedByFirst: shared.parent === first
        property bool ownedBySecond: shared.parent === second
        function hideSecond() { second.visible = false }
      }
      """
    When 100 milliseconds pass
    Then the property "ownedBySecond" of "root" is true
    And the property "width" of "shared" is 120
    And the property "height" of "shared" is 90
    When the method "hideSecond" of "root" is called
    And 100 milliseconds pass
    Then the property "ownedByFirst" of "root" is true
    And the property "width" of "shared" is 80
    And the property "height" of "shared" is 60
    And the property "visible" of "shared" is true
