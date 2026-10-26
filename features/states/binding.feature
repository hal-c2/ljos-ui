# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-binding.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# Backlog: what is written to a property, or bound to it, after the Binding last gave it a value
# is kept when the Binding lets go, and a binding given so goes on; moving the held layer's own
# binding down when it is taken off is the graph's (bindings infrastructure). Found by ljos-fuzz
# (connections), seed 303, bucket warning:assign(ljos-only):Unable_to_assign_QPointF_to_int.

@qt-parity
Feature: The Binding element
  A Binding gives a property of another object a value for as long as its
  condition holds, and says what the property is left with afterwards.

  Rule: A Binding holds its target's property at its value

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property int src: 5
          property int base: 10
          property bool on: false
          Rectangle { id: bound; objectName: "bound"; width: root.base; border.width: 1 }
          Rectangle { id: plain; objectName: "plain"; width: 10 }
          Rectangle { id: always; objectName: "always"; width: 3 }
          Binding { target: bound; property: "width"; value: root.src; when: root.on }
          Binding { target: plain; property: "width"; value: root.src; when: root.on }
          Binding { target: always; property: "width"; value: root.src * 2 }
          Binding { target: bound; property: "border.width"; value: root.src }
          Binding { target: plain; property: "nosuch"; value: 1 }
        }
        """

    @api-QtQml.Binding.value
    Scenario: A Binding without a condition holds from the start
      Then the property "width" of "always" is 10

    @api-QtQml.Binding.when
    Scenario: A Binding whose condition does not hold leaves the property alone
      Then the property "width" of "bound" is 10

    @api-QtQml.Binding.when
    Scenario Outline: The "<object>" property takes the value when the condition holds
      When the property "on" of "root" is set to true
      Then the property "width" of "<object>" is 5

      Examples:
        | object |
        | bound  |
        | plain  |

    Scenario: The property follows the value
      When the property "on" of "root" is set to true
      And the property "src" of "root" is set to 7
      Then the property "width" of "bound" is 7

    @api-QtQml.Binding.property
    Scenario: The property may be one of a group
      Then the property "border.width" of "bound" is 5

    Scenario: A property the target does not have is reported
      Then the log has a warning matching "Binding: Property 'nosuch' does not exist on Rectangle"

    Scenario: A value written to the property shows until the Binding's value changes
      When the property "on" of "root" is set to true
      And the property "width" of "bound" is set to 99
      Then the property "width" of "bound" is 99

    Scenario: The Binding's next value replaces a value written to the property
      When the property "on" of "root" is set to true
      And the property "width" of "bound" is set to 99
      And the property "src" of "root" is set to 8
      Then the property "width" of "bound" is 8

    Scenario: What the property was bound to does not show while the condition holds
      When the property "on" of "root" is set to true
      And the property "base" of "root" is set to 20
      Then the property "width" of "bound" is 5

    Scenario Outline: When the condition no longer holds the "<object>" property has what it had
      When the property "on" of "root" is set to true
      And the property "base" of "root" is set to 20
      And the property "on" of "root" is set to false
      Then the property "width" of "<object>" is <width>

      Examples:
        | object | width |
        | bound  | 20    |
        | plain  | 10    |

    Scenario: The binding that comes back follows what it reads
      When the property "on" of "root" is set to true
      And the property "on" of "root" is set to false
      And the property "base" of "root" is set to 30
      Then the property "width" of "bound" is 30

  Rule: The restore mode says what comes back

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property int base: 10
          property bool on: false
          component Held: Binding { property: "width"; value: 5; when: root.on }
          Rectangle { id: noneBound; objectName: "noneBound"; width: root.base }
          Rectangle { id: nonePlain; objectName: "nonePlain"; width: 10 }
          Rectangle { id: bindingBound; objectName: "bindingBound"; width: root.base }
          Rectangle { id: bindingPlain; objectName: "bindingPlain"; width: 10 }
          Rectangle { id: valueBound; objectName: "valueBound"; width: root.base }
          Rectangle { id: valuePlain; objectName: "valuePlain"; width: 10 }
          Held { target: noneBound; restoreMode: Binding.RestoreNone }
          Held { target: nonePlain; restoreMode: Binding.RestoreNone }
          Held { target: bindingBound; restoreMode: Binding.RestoreBinding }
          Held { target: bindingPlain; restoreMode: Binding.RestoreBinding }
          Held { target: valueBound; restoreMode: Binding.RestoreValue }
          Held { target: valuePlain; restoreMode: Binding.RestoreValue }
        }
        """

    @api-QtQml.Binding.restoreMode
    Scenario Outline: After the condition held, "<object>" is <width> wide
      When the property "on" of "root" is set to true
      And the property "on" of "root" is set to false
      Then the property "width" of "<object>" is <width>

      Examples: `Binding.RestoreNone` puts nothing back
        | object    | width |
        | noneBound | 5     |
        | nonePlain | 5     |

      Examples: `Binding.RestoreBinding` puts a binding back and leaves a plain value replaced
        | object       | width |
        | bindingBound | 10    |
        | bindingPlain | 5     |

      Examples: `Binding.RestoreValue` puts a plain value back and leaves a binding replaced
        | object     | width |
        | valueBound | 5     |
        | valuePlain | 10    |

    @api-QtQml.Binding.restoreMode
    Scenario Outline: Afterwards "<object>" <follows> what it was bound to
      When the property "on" of "root" is set to true
      And the property "on" of "root" is set to false
      And the property "base" of "root" is set to 30
      Then the property "width" of "<object>" is <width>

      Examples:
        | object       | follows         | width |
        | noneBound    | does not follow | 5     |
        | bindingBound | follows         | 30    |
        | valueBound   | does not follow | 5     |

  Rule: A delayed Binding passes its value on when everything else has been done

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property int src: 5
          property string log: ""
          property int seen: 0
          function twice() {
            src = 11; src = 12; seen = late.width
            log += "now:" + late.width + " "
          }
          Rectangle {
            id: late; objectName: "late"; width: 1
            onWidthChanged: root.log += "width:" + width + " "
          }
          Binding { target: late; property: "width"; value: root.src; delayed: true }
        }
        """

    @api-QtQml.Binding.delayed
    Scenario: The property has the value from the start
      Then the property "width" of "late" is 5

    @api-QtQml.Binding.delayed
    Scenario: A new value is not passed on before the code that gave it has returned
      When the method "twice" of "root" is called
      Then the property "seen" of "root" is 5

    @api-QtQml.Binding.delayed
    Scenario: Of several new values in a row only the last is passed on
      When the method "twice" of "root" is called
      And 100 milliseconds pass
      Then the property "log" of "root" is "width:5 now:5 width:12 "

  Rule: A Binding may name the properties of other objects in its body

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property int src: 5
          property bool on: true
          Item {
            id: box; objectName: "box"; width: 1
            property alias shown: inner.visible
            Item { id: inner; objectName: "inner" }
          }
          Binding { when: root.on; box.width: root.src; box.shown: root.src > 6 }
        }
        """

    Scenario: The properties named have the values from the start
      Then the property "width" of "box" is 5
      And the property "visible" of "inner" is false

    Scenario: The properties named follow the values while the condition holds
      When the property "src" of "root" is set to 7
      Then the property "width" of "box" is 7
      And the property "visible" of "inner" is true

    Scenario: When the condition no longer holds they have what they had
      When the property "on" of "root" is set to false
      Then the property "width" of "box" is 1
      And the property "visible" of "inner" is true
      And the log has no warnings

  Rule: A Binding takes no property until it is given a value

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property var a: 1
          property var b: 1
          property int n: 4
          property int w: width
          property string seen
          Binding { id: b1; target: root; property: "a" }
          Binding { id: b2; target: root; property: "b"; value: undefined }
          Binding { id: b3; target: root; property: "n" }
          Binding { id: b4; target: root; property: "w" }
          function look() { seen += [String(a), String(b), n, w].join("/") + ";" }
          function run() {
            look(); b1.value = 7; b3.value = 8
            look(); width = 50
            look(); b1.when = false; b2.when = false; b3.when = false; b4.when = false
            look()
          }
          function write() {
            a = 2; n = 3; b4.when = false; b1.when = false; b3.when = false
            look()
          }
        }
        """

    Scenario: A value given later takes the property, and an undefined one is a value
      When the method "run" of "root" is called
      Then the property "seen" of "root" is "1/undefined/4/0;7/undefined/8/0;7/undefined/8/50;1/1/4/50;"
      And the log has no warnings

    Scenario: A Binding without a value leaves what is written to the property when it is turned off
      When the method "write" of "root" is called
      Then the property "seen" of "root" is "2/undefined/3/0;"

  @backlog
  Rule: What is given a property after the Binding last gave it a value stays when it lets go

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property int k: 3
          property int v: 0
          property int bound: k * 10
          property int plain: 7
          property int rebound: k * 10
          property string seen
          Binding { id: b1; target: root; property: "bound"; value: root.v }
          Binding { id: b2; target: root; property: "plain"; value: root.v }
          Binding { id: b3; target: root; property: "rebound"; value: root.v }
          function look() { seen += [bound, plain, rebound].join(",") + ";" }
          function run(again) {
            bound = 55; plain = 55; rebound = Qt.binding(function () { return k + 100 })
            if (again) v = 1
            look(); b1.when = false; b2.when = false; b3.when = false
            look(); k = 4
            look()
          }
        }
        """

    Scenario: A value written and a binding given while it holds stay, and the binding goes on
      When the method "run" of "root" is called with false
      Then the property "seen" of "root" is "55,55,103;55,55,103;55,55,104;"

    Scenario: Once its value changes after them, what it holds comes back as before
      When the method "run" of "root" is called with true
      Then the property "seen" of "root" is "1,1,1;30,7,30;40,7,40;"
