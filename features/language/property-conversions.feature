# Sources:
#   https://doc.qt.io/qt-6/qtqml-typesystem-valuetypes.html
#   https://doc.qt.io/qt-6/qtqml-syntax-signals.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (bindings); each scenario names its bucket.

@qt-parity
Feature: A value a binding gives a property of another type
  A binding's value is converted to the type of the property it is bound to. Some conversions
  keep what they can of the value, and some give the property its empty value and warn.

  # Bucket value:string:string-times-one (and value:bool:string-lt, which compares it).
  Scenario Outline: A string property given <number> reads "<text>"
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string s: <number>
      }
      """
    Then the property "s" of "root" is "<text>"

    Examples:
      | number       | text |
      | ("a" * 1)    | nan  |
      | (1 / 0)      | inf  |
      | Math.log(0)  | -inf |

  # Bucket value:list<real>:Qt.tint.
  Scenario: A list of reals given a colour holds one zero
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<real> l: Qt.tint("#000000", "#000000")
      }
      """
    Then the property "l" of "root" is [0]

  # Bucket value:rect:Qt.point.
  Scenario: A rect given a point keeps the point's x and y
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property rect r: Qt.point(3, 7)
        property real rx: r.x
        property real ry: r.y
        property real rw: r.width
      }
      """
    Then the property "rx" of "root" is 3
    And the property "ry" of "root" is 7
    And the property "rw" of "root" is 0

  # Buckets value:string:literal and warning:assign(qt-only):literal.
  # Bucket warning:assign(qt-only):Unable_to_assign_QSizeF_to_QString for a size.
  Scenario Outline: A string given a <type> is empty and warned about
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string s: <value>
      }
      """
    Then the property "s" of "root" is ""
    And the log has a warning matching "Unable to assign <qt> to QString"

    Examples:
      | type  | value          | qt      |
      | point | Qt.point(0, 0) | QPointF |
      | size  | Qt.size(0, 0)  | QSizeF  |

  # Bucket value:var(array):pair.
  Scenario: A real property keeps the sign of zero
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property real zero: -0
        property string inverse: String(1 / zero)
      }
      """
    Then the property "inverse" of "root" is "-Infinity"

  Scenario: A script's -0 keeps its sign in a real, a var and a list of reals
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property real r
        property var v
        property list<real> l
        property string seen
        function write() {
          const zero = -0
          r = zero; v = zero; l = [zero]; l.push(zero)
          seen = [1 / r, 1 / v, 1 / l[0], 1 / l[1]].join(" ")
        }
      }
      """
    When the method "write" of "root" is called
    Then the property "seen" of "root" is "-Infinity -Infinity -Infinity -Infinity"

  # Bucket value:var(number):step-emit(color.hslLightness).
  Scenario: A real parameter of a signal given undefined is NaN
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string seen: ""
        signal moved(real x)
        onMoved: function (x) { seen = typeof x + " " + isNaN(x) }
        function send() { moved(undefined) }
      }
      """
    When the method "send" of "root" is called
    Then the property "seen" of "root" is "number true"

  # Bucket warning:assign(ljos-only):Unable_to_assign_QColor_to_rect is the last row.
  # Buckets warning:assign(ljos-only):Unable_to_assign_QJSValue_to_rect, …_to_point and
  # …_to_color (the colour is in features/language/color-values.feature), and
  # warning:assign(ljos-only):Unable_to_assign_double_to_color and …_bool_to_color.
  Scenario Outline: A <type> given <value> is empty and not warned about
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property <type> v: <value>
        property real vx: <x>
      }
      """
    Then the property "vx" of "root" is 0
    And the log has no warning matching "assign"

    Examples:
      | type  | value     | x       |
      | rect  | []        | v.x     |
      | point | []        | v.x     |
      | color | (1 + 0)   | v.r     |
      | rect  | Qt.tint("#000000", "#000000") | v.x |
      | color | ("" == 0) | v.r     |

  # Bucket warning:assign(qt-only):Unable_to_assign_QUrl_to_double.
  @backlog
  Scenario: A real bound to a url is zero and warned about
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property url address: "a.png"
        property real n: address
      }
      """
    Then the property "n" of "root" is 0
    And the log has a warning matching "Unable to assign QUrl to double"

  # Bucket value:list<real>:step-push(color.r).
  Scenario: undefined pushed onto a list of reals is NaN
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<real> l
        property string seen: ""
        function add() { l.push(undefined); seen = l.length + " " + isNaN(l[0]) }
      }
      """
    When the method "add" of "root" is called
    Then the property "seen" of "root" is "1 true"

  Scenario: An entry of a list of reals never written is 0, and one given undefined NaN
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<real> l: [1]
        property string seen: ""
        function add() {
          l[4] = 5; l[1] = undefined; l.splice(3, 0, undefined)
          seen = l.join(" ")
          l.length = 8
          seen += "; " + l.join(" ")
        }
      }
      """
    When the method "add" of "root" is called
    Then the property "seen" of "root" is "1 NaN 0 NaN 0 5; 1 NaN 0 NaN 0 5 0 0"

  # Bucket value:url:default.
  Scenario: A font given an array is unchanged and told as a change
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property font f: [1 + 0]
        property int changes: 0
        onFChanged: changes++
        property font plain
        property bool same: f.family === plain.family && f.pixelSize === plain.pixelSize
      }
      """
    Then the property "changes" of "root" is 1
    And the property "same" of "root" is true

  Scenario: A declared colour or font tells its first write as a change, even of what it held
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property color c
        property font f
        property font plain
        property int cs: 0
        property int fs: 0
        onCChanged: cs++
        onFChanged: fs++
        property string seen
        function write() {
          f = plain; f = {}; c = []; c = [1]
          seen = [cs, fs, c === Qt.color("#00000000") || !c.valid, f == plain].join(" ")
          c = "transparent"
          seen += "; " + cs
        }
      }
      """
    When the method "write" of "root" is called
    Then the property "seen" of "root" is "1 1 true true; 2"

  # Bucket warning:assign(qt-only):Unable_to_assign_[undefined]_to_int.
  Scenario: An int bound to undefined is zero and warned about
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int n: "#000000".r
      }
      """
    Then the property "n" of "root" is 0
    And the log has a warning matching "Unable to assign .undefined. to int"

  # Bucket warning:assign(qt-only):Unable_to_assign_[undefined]_to_double.
  Scenario: A real bound to undefined is zero and warned about
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property real n: "#000000".r
      }
      """
    Then the property "n" of "root" is 0
    And the log has a warning matching "Unable to assign .undefined. to double"

  # Bucket warning:assign(ljos-only):Unable_to_assign_QString_to_font.
  Scenario: A font given text takes it as its family and is not warned about
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property font f: "a"
        property string family: f.family
      }
      """
    Then the property "family" of "root" is "a"
    And the log has no warning matching "assign"

  # Bucket warning:assign(ljos-only):Unable_to_assign_QColor_to_size.
  Scenario: A size given a colour is invalid and not warned about
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property size s: Qt.darker("#000000", 1.5)
        property real w: s.width
        property real h: s.height
      }
      """
    Then the property "w" of "root" is -1
    And the property "h" of "root" is -1
    And the log has no warning matching "assign"

  # Buckets value:list<string>:Qt.darker and value:var(array):ref.
  Scenario: A list of strings given a colour holds the colour's name
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property color c
        property list<string> names: c
        property string seen: names.length + " " + names[0]
      }
      """
    Then the property "seen" of "root" is "1 #000000"

  # Bucket value:var(array):step-push(literal).
  # Bound to the list itself, the var is given the list's entries without a script, and so
  # without its type: the binding's plan would have to say where the list came from.
  @backlog
  Scenario: A var given a list of strings holds a copy that makes text of what is pushed
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<string> names
        property var held: names
        property string seen: ""
        function add() { held.push(0); seen = typeof held[0] + " " + names.length }
      }
      """
    When the method "add" of "root" is called
    Then the property "seen" of "root" is "string 0"

  Scenario: A var a script gives a typed list holds a copy that makes entries of its type
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<string> names: ["a"]
        property list<real> reals: [1]
        property list<int> ints: [1]
        property var held
        property var again
        property var hr
        property var hi
        property string seen
        function add() {
          held = names; hr = reals; hi = ints
          held.push(0, null, {}, true); held.splice(1, 0, 2)
          again = held; again.push(3)
          hr.push("2", undefined); hi.push(1.5, "x")
          seen = [held.map(x => typeof x + ":" + x).join(","), held.length, again[6],
                  typeof again[6], names.length, hr.join(","), hi.join(","), reals.length].join(" ")
        }
      }
      """
    When the method "add" of "root" is called
    Then the property "seen" of "root" is "string:a,string:2,string:0,string:,string:,string:true 6 3 string 1 1,2,NaN 1,2,0 1"

  # Bucket result:none/none:call(number-plus-string).
  Scenario: An int parameter of a function drops what is after the point
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        function whole(a: int) { return a + "" }
        property string seen: whole(1.75) + " " + whole(-1.75)
      }
      """
    Then the property "seen" of "root" is "1 -1"

  Scenario Outline: A <type> parameter of a function given <given> reads <seen>
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        function read(first: int, v: <type>): string { return typeof v + ":" + v }
        property string seen: read(<given>)
      }
      """
    Then the property "seen" of "root" is "<seen>"

    Examples:
      | type   | given        | seen               |
      | int    | 0, "3"       | number:3           |
      | int    | 0, "x"       | number:0           |
      | int    | 0, null      | number:0           |
      | int    | 0, 3e9       | number:-1294967296 |
      | int    | 0, true      | number:1           |
      | int    | 0            | number:0           |
      | real   | 0, "2"       | number:2           |
      | real   | 0, undefined | number:NaN         |
      | real   | 0, null      | number:0           |
      | double | 0, "x"       | number:NaN         |
      | string | 0, 5         | string:5           |
      | string | 0, undefined | string:undefined   |
      | string | 0, null      | string:null        |
      | bool   | 0, 0         | boolean:false      |
      | bool   | 0, "x"       | boolean:true       |
      | bool   | 0            | boolean:false      |
      | var    | 0, "2"       | string:2           |

  # Buckets value:string:pair and value:string:ref.
  Scenario: A string given a list of numbers is made of those character codes
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<real> codes: [65, 66]
        property string text: codes
      }
      """
    Then the property "text" of "root" is "AB"

  # Bucket value:load:text-rounding.
  Scenario: A number halfway between two shortest texts is written with the even one
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<string> names: [0.50000762939453125]
        property string seen: names[0]
      }
      """
    Then the property "seen" of "root" is "0.5000076293945312"
