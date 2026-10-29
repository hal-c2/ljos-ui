# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (bindings), bucket value:int:literal.
# Seed 303 (states), bucket value:onChanged:text: the handlers of the bindings made while the
# objects are made run in Ljós once all are made, in the order the handlers were, not as each
# binding is made. When a handler runs is the reactive graph's scheduling (backlog).

@qt-parity
Feature: A change handler run while the object is made
  The bindings of an object are made one after another. When one is made its change handler
  runs at once, before the bindings that come after it: what the handler writes to one of those
  replaces it before it is made. The objects' bindings are made children first, the last child
  first, each child with its own children before it; then the object's own, then the objects its
  properties hold, in the order they are written.

  @backlog
  Scenario: A handler that writes a property whose binding is not made yet replaces the binding
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string label: (0).toPrecision(3)
        height: Text.AlignRight
        property int runs: 0
        onLabelChanged: { runs++; root.height = root.height }
      }
      """
    Then the property "runs" of "root" is 1
    And the property "height" of "root" is 0

  Scenario: A property given a plain value runs no handler while the object is made
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string label: "a"
        height: Text.AlignRight
        property int runs: 0
        onLabelChanged: { runs++; root.height = root.height }
      }
      """
    Then the property "runs" of "root" is 0
    And the property "height" of "root" is 2

  @backlog
  Scenario: The bindings of the objects are made children first, the last child first
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        property string p0: String("r0")
        onP0Changed: log += "root;"
        property QtObject zo: QtObject {
          property string q: String("z"); onQChanged: root.log += "Z;"
          property QtObject zz: QtObject { property string q: String("zz"); onQChanged: root.log += "ZZ;" }
        }
        Item {
          property string p: String("a"); onPChanged: root.log += "A;"
          property QtObject xo: QtObject { property string q: String("x"); onQChanged: root.log += "X;" }
          Item {
            property string p: String("a1"); onPChanged: root.log += "A1;"
            property QtObject w: QtObject { property string q: String("w"); onQChanged: root.log += "W;" }
          }
        }
        Item {
          property string p: String("b"); onPChanged: root.log += "B;"
          property QtObject yo: QtObject { property string q: String("y"); onQChanged: root.log += "Y;" }
          Item { property string p: String("b1"); onPChanged: root.log += "B1;" }
        }
        property QtObject v: QtObject { property string q: String("v"); onQChanged: root.log += "V;" }
      }
      """
    Then the property "log" of "root" is "B1;B;Y;A1;W;A;X;root;Z;ZZ;V;"

  # Found by ljos-fuzz (bindings), bucket value:onChanged+onCompleted:text.
  @backlog
  Scenario: A built-in property given an enum runs its handler while the object is made
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string log: ""
        width: Text.AlignLeft
        onWidthChanged: log += "width;"
        Component.onCompleted: log += "completed;"
      }
      """
    Then the property "log" of "root" is "width;completed;"
