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
  @backlog
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

  # Bucket value:list<real>:Qt.tint.
  @backlog
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
  @backlog
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
  @backlog
  Scenario: A string given a point is empty and warned about
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string s: Qt.point(0, 0)
      }
      """
    Then the property "s" of "root" is ""
    And the log has a warning matching "Unable to assign QPointF to QString"

  # Bucket value:var(array):pair.
  @backlog
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

  # Bucket value:var(number):step-emit(color.hslLightness).
  @backlog
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
  @backlog
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
  @backlog
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

  # Bucket warning:assign(ljos-only):Unable_to_assign_QString_to_font.
  @backlog
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
  @backlog
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
  @backlog
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
  @backlog
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
