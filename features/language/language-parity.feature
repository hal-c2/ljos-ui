# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#default-properties
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#data-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#children-prop
#   https://doc.qt.io/qt-6/qml-list.html
#   https://doc.qt.io/qt-6/qtqml-syntax-signals.html
#   https://doc.qt.io/qt-6/qml-qtqml-connections.html
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#property-aliases
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Language parity

  Scenario: Declarative and imperative additions to data split visual items from non-visual objects
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 100; height: 100
        property int dataCount: data.length
        property int childCount: children.length
        property string childNames: ""
        Item { objectName: "first" }
        QtObject { id: nonVisual; objectName: "nonVisual" }
        Item { objectName: "second" }
        property QtObject extraObj: QtObject { objectName: "extraObj" }
        property Item extraItem: Item { objectName: "extraItem" }
        function record() {
          var out = []
          for (var i = 0; i < children.length; ++i) out.push(children[i].objectName)
          childNames = out.join(",")
        }
        function pushBoth() {
          data.push(extraObj)
          data.push(extraItem)
          record()
        }
        Component.onCompleted: record()
      }
      """
    Then the property "dataCount" of "root" is 3
    And the property "childCount" of "root" is 2
    And the property "childNames" of "root" is "first,second"
    When the method "pushBoth" of "root" is called
    Then the property "dataCount" of "root" is 3
    And the property "childCount" of "root" is 3
    And the property "childNames" of "root" is "first,second,extraItem"

# Verified on Qt 6.11.2 (qml6, offscreen).
  Scenario: children.push notifies children; resources.push does not notify resources
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 100; height: 100
        property int childCount: children.length
        property int resCount: resources.length
        Component { id: c; Item {} }
        Component { id: q; QtObject {} }
        function go() {
          root.children.push(c.createObject(null))
          root.resources.push(q.createObject(null))
        }
      }
      """
    Then the property "childCount" of "root" is 0
    And the property "resCount" of "root" is 2
    When the method "go" of "root" is called
    Then the property "childCount" of "root" is 1
    And the property "resCount" of "root" is 2

  Scenario: Overriding the default property in a derived component keeps base children in base data
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 100; height: 100
        component Frame : Item {
          default property QtObject contentSlot
          Item { objectName: "chrome" }
        }
        Frame {
          id: frame; objectName: "frame"
          QtObject { objectName: "payload" }
        }
        property int frameChildren: frame.children.length
        property string chromeName: frame.children[0].objectName
        property string slotName: frame.contentSlot.objectName
      }
      """
    Then the property "frameChildren" of "root" is 1
    And the property "chromeName" of "root" is "chrome"
    And the property "slotName" of "root" is "payload"

  Scenario: Multi-argument push and splice preserve order and update length
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 100; height: 100
        property QtObject a: QtObject { objectName: "a" }
        property QtObject b: QtObject { objectName: "b" }
        property QtObject c: QtObject { objectName: "c" }
        property QtObject d: QtObject { objectName: "d" }
        property list<QtObject> items: [a]
        property int changes: 0
        property string names: ""
        onItemsChanged: changes++
        function dump() {
          var s = []
          for (var i = 0; i < items.length; ++i) s.push(items[i] ? items[i].objectName : "null")
          names = s.join(",")
        }
        function pushTwo() { items.push(b, c); dump() }
        function spliceMiddle() { items.splice(1, 1, d); dump() }
      }
      """
    When the method "pushTwo" of "root" is called
    Then the property "names" of "root" is "a,b,c"
    And the property "changes" of "root" is 3
    When the method "spliceMiddle" of "root" is called
    Then the property "names" of "root" is "a,d,c"

  Scenario: Expanding and truncating a list property's length pads with null and drops tail entries
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 100; height: 100
        property QtObject a: QtObject { objectName: "a" }
        property QtObject b: QtObject { objectName: "b" }
        property list<QtObject> items: [a, b]
        property string summary: ""
        function resizeAndTruncate() {
          items.length = 4
          var expanded = items.length + ":" + (items[2] === null) + ":" + (items[3] === null)
          items.length = 1
          summary = expanded + "|after=" + items.length + ":" + items[0].objectName
        }
      }
      """
    When the method "resizeAndTruncate" of "root" is called
    Then the property "summary" of "root" is "4:true:true|after=1:a"

  Scenario: Signal-to-signal connection drops trailing parameters and can be disconnected
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 100; height: 100
        property string log: ""
        signal fullEvent(int code, string label, bool extra)
        signal shortEvent(int code)
        onShortEvent: (code) => { log += "short(" + code + ");" }
        function zeroArg() { log += "zero;" }
        Component.onCompleted: {
          fullEvent.connect(shortEvent)
          fullEvent.connect(zeroArg)
        }
        function fireAndUnwire() {
          fullEvent(42, "ignored", true)
          fullEvent.disconnect(shortEvent)
          fullEvent(7, "ignored", false)
        }
      }
      """
    When the method "fireAndUnwire" of "root" is called
    Then the property "log" of "root" is "short(42);zero;zero;"

  Scenario: Disconnecting a second handler from inside the first handler prevents the second from running
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 100; height: 100
        property string log: ""
        signal step(int n)
        function first(n) {
          log += "first(" + n + ");"
          step.disconnect(second)
        }
        function second(n) {
          log += "second(" + n + ");"
        }
        Component.onCompleted: {
          step.connect(first)
          step.connect(second)
        }
        function trigger() { step(1); step(2) }
      }
      """
    When the method "trigger" of "root" is called
    Then the property "log" of "root" is "first(1);first(2);"

  Scenario: Disabling or retargeting a second Connections from the first handler during emission stops the second handler
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 100; height: 100
        property string log: ""
        signal ping(int v)
        Item { id: other; signal ping(int v) }
        Connections {
          id: c1; target: root
          function onPing(v) {
            root.log += "c1(" + v + ");"
            if (v === 1) c2.enabled = false
            if (v === 2) c2.target = other
          }
        }
        Connections {
          id: c2; target: root
          function onPing(v) { root.log += "c2(" + v + ");" }
        }
        function testDisable() { log = ""; c2.enabled = true; c2.target = root; ping(1) }
        function testRetarget() { log = ""; c2.enabled = true; c2.target = root; ping(2); other.ping(9) }
      }
      """
    When the method "testDisable" of "root" is called
    Then the property "log" of "root" is "c2(1);c1(1);"
    When the method "testRetarget" of "root" is called
    Then the property "log" of "root" is "c2(2);c1(2);c2(9);"

  Scenario: A Connections with target explicitly set to null logs no warning for handlers target would not have
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 100; height: 100
        property bool isNullTarget: conn.target === null
        Connections {
          id: conn
          target: null
          ignoreUnknownSignals: false
          function onNonExistentSignal() {}
        }
      }
      """
    Then the property "isNullTarget" of "root" is true
    And the log has no warnings
