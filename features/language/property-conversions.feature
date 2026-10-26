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
  @backlog
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

  # Bucket result:none/none:call(number-plus-string).
  @backlog
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
  @backlog
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
