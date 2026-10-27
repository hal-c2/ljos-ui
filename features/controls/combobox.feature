# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-combobox.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-itemdelegate.html
#   https://doc.qt.io/qt-6/qt.html#MatchFlag-enum
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: ComboBox
  A combo box shows one row of a model and lets the user pick another from
  a popup, with the pointer or with the keys.

  Rule: What a combo box is before it is given anything

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 400; height: 400
          ComboBox { objectName: "c"; property string value: "" + currentValue }
        }
        """

    @api-QtQuick.Controls.ComboBox
    Scenario Outline: An empty combo box starts with <property> <value>
      Then the property "<property>" of "c" is <value>

      Examples:
        | property                   | value       |
        | count                      | 0           |
        | currentIndex               | -1          |
        | currentText                | ""          |
        | value                      | "undefined" |
        | displayText                | ""          |
        | highlightedIndex           | -1          |
        | textRole                   | ""          |
        | valueRole                  | ""          |
        | pressed                    | false       |
        | down                       | false       |
        | flat                       | false       |
        | editable                   | false       |
        | editText                   | ""          |
        | acceptableInput            | true        |
        | inputMethodComposing       | false       |
        | selectTextByMouse          | false       |
        | implicitContentWidthPolicy | 0           |
        | focusPolicy                | 11          |
        | activeFocusOnTab           | true        |
        | hoverEnabled               | true        |
        | wheelEnabled               | false       |
        | padding                    | 0           |
        | leftPadding                | 0           |
        | rightPadding               | 40          |
        | spacing                    | 0           |
        | implicitWidth              | 140         |
        | implicitHeight             | 40          |
        | width                      | 140         |
        | height                     | 40          |

    @api-QtQuick.Controls.ComboBox.popup
    Scenario Outline: Its popup starts with <property> <value>
      Then the property "popup.<property>" of "c" is <value>

      Examples:
        | property     | value |
        | visible      | false |
        | modal        | false |
        | focus        | false |
        | closePolicy  | 18    |
        | padding      | 0     |
        | topMargin    | 6     |
        | bottomMargin | 6     |
        | x            | 0     |
        | width        | 140   |

    @api-QtQuick.Controls.ComboBox.indicator
    Scenario Outline: Its <part> starts with <property> <value>
      Then the property "<part>.<property>" of "c" is <value>

      Examples:
        | part        | property       | value     |
        | background  | implicitWidth  | 140       |
        | background  | implicitHeight | 40        |
        | background  | width          | 140       |
        | background  | height         | 40        |
        | background  | color          | "#e0e0e0" |
        | background  | visible        | true      |
        | indicator   | x              | 100       |
        | indicator   | y              | 6         |
        | indicator   | width          | 40        |
        | indicator   | height         | 28        |
        | contentItem | x              | 0         |
        | contentItem | y              | 0         |
        | contentItem | width          | 100       |
        | contentItem | height         | 40        |
        | contentItem | text           | ""        |
        | contentItem | color          | "#26282a" |

    Scenario: The size of the indicator is what the combo box reads as its implicit size
      Then the property "implicitIndicatorWidth" of "c" is 40
      And the property "implicitIndicatorHeight" of "c" is 28

  Rule: The rows come from the model

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          width: 400; height: 400
          function say(c) { return c.count + " " + c.currentIndex + " [" + c.currentText + "] " + c.currentValue }
          ListModel { id: names; ListElement { name: "Alpha" } ListElement { name: "Beta" } }
          ListModel { id: rows; ListElement { name: "Alpha"; code: 10 } ListElement { name: "Beta"; code: 20 } }
          ComboBox { objectName: "none"; property string seen: root.say(this) }
          ComboBox { objectName: "number"; model: 3; property string seen: root.say(this) }
          ComboBox { objectName: "strings"; model: ["One", "Two"]; property string seen: root.say(this) }
          ComboBox { objectName: "mixed"; model: [1.5, true, "s"]; property string seen: root.say(this) }
          ComboBox { objectName: "names"; model: names; property string seen: root.say(this) }
          ComboBox { objectName: "plain"; model: rows; property string seen: count + " [" + currentText + "]" }
          ComboBox { objectName: "roles"; model: rows; textRole: "name"; valueRole: "code"
                     property string seen: root.say(this) }
          ComboBox { objectName: "objects"; model: [{ label: "A", v: 1 }, { label: "B", v: 2 }]
                     textRole: "label"; valueRole: "v"; property string seen: root.say(this) }
          ComboBox { objectName: "beyond"; model: ["p", "q"]; currentIndex: 5; property string seen: root.say(this) }
          ComboBox { objectName: "second"; model: rows; textRole: "name"; valueRole: "code"; currentIndex: 1
                     displayText: "Pick: " + currentText; property string seen: root.say(this) }
        }
        """

    @api-QtQuick.Controls.ComboBox.model
    Scenario Outline: A combo box over <model> has its count, current index, text and value
      Then the property "seen" of "<combo>" is <seen>

      Examples:
        | model                                    | combo   | seen                 |
        | nothing                                  | none    | "0 -1 [] undefined"  |
        | a number                                 | number  | "3 0 [0] 0"          |
        | a list of strings                        | strings | "2 0 [One] One"      |
        | a list of values                         | mixed   | "3 0 [1.5] 1.5"      |
        | a list model with one role               | names   | "2 0 [Alpha] Alpha"  |
        | a list model with roles and no text role | plain   | "2 []"               |
        | a list model with a text and value role  | roles   | "2 0 [Alpha] 10"     |
        | a list of objects with roles             | objects | "2 0 [A] 1"          |
        | a list and an index beyond it            | beyond  | "2 5 [] undefined"   |
        | a list and a declared index              | second  | "2 1 [Beta] 20"      |

    @api-QtQuick.Controls.ComboBox.displayText
    Scenario: The text shown follows the current text until it is given
      Then the property "displayText" of "strings" is "One"
      And the property "contentItem.text" of "strings" is "One"
      And the property "displayText" of "second" is "Pick: Beta"
      And the property "contentItem.text" of "second" is "Pick: Beta"

    Scenario: A text that was given stays when the current row changes
      When the property "displayText" of "strings" is set to "Shown"
      And the property "currentIndex" of "strings" is set to 1
      Then the property "displayText" of "strings" is "Shown"
      And the property "currentText" of "strings" is "Two"

    @api-QtQuick.Controls.ComboBox.currentIndex
    Scenario Outline: Setting the current index to <index> shows <seen>
      When the property "currentIndex" of "roles" is set to <index>
      Then the property "seen" of "roles" is <seen>

      Examples:
        | index | seen                 |
        | 1     | "2 1 [Beta] 20"      |
        | -1    | "2 -1 [] undefined"  |
        | 9     | "2 9 [] undefined"   |

    Scenario Outline: A new model of <rows> makes the current index <index>
      When the property "currentIndex" of "strings" is set to 1
      And the property "model" of "strings" is set to <model>
      Then the property "seen" of "strings" is <seen>

      Examples:
        | rows       | model           | index | seen                |
        | three rows | ["k", "l", "m"] | 0     | "3 0 [k] k"         |
        | no rows    | []              | -1    | "0 -1 [] undefined" |
        | a number   | 2               | 0     | "2 0 [0] 0"         |
        | nothing    | null            | -1    | "0 -1 [] undefined" |

    @api-QtQuick.Controls.ComboBox.textRole
    Scenario: Another text role shows another role of the same row
      When the property "textRole" of "roles" is set to "code"
      Then the property "seen" of "roles" is "2 0 [10] 10"

    @api-QtQuick.Controls.ComboBox.valueRole
    Scenario: Another value role gives another role of the same row
      When the property "valueRole" of "roles" is set to "name"
      Then the property "seen" of "roles" is "2 0 [Alpha] Alpha"

    @api-QtQuick.Controls.ComboBox.currentValue
    Scenario: Setting the current value makes the row that has it current
      When the property "currentValue" of "roles" is set to 20
      Then the property "seen" of "roles" is "2 1 [Beta] 20"

  Rule: The combo box follows a model that changes

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          ListModel {
            id: rows
            ListElement { name: "Alpha"; code: 10 }
            ListElement { name: "Beta"; code: 20 }
            ListElement { name: "Gamma"; code: 30 }
          }
          function insert() { rows.insert(0, { name: "Zero", code: 0 }) }
          function remove(i) { rows.remove(i) }
          function rename(i, name) { rows.setProperty(i, "name", name) }
          function recode(i, code) { rows.setProperty(i, "code", code) }
          function move(from, to) { rows.move(from, to, 1) }
          function clear() { rows.clear() }
          function append(name) { rows.append({ name: name, code: 99 }) }
          ComboBox { id: c; objectName: "c"; model: rows; textRole: "name"; valueRole: "code"; currentIndex: 1
            property string seen: count + " " + currentIndex + " [" + currentText + "] " + currentValue }
        }
        """

    Scenario: A row put in before the current one leaves the index and changes the row
      When the method "insert" of "root" is called
      Then the property "seen" of "c" is "4 1 [Alpha] 10"

    Scenario: Taking another row out changes only the count
      When the method "remove" of "root" is called with 2
      Then the property "seen" of "c" is "2 1 [Beta] 20"

    Scenario: Taking the last row out while it is current leaves the index with no row
      When the property "currentIndex" of "c" is set to 2
      And the method "remove" of "root" is called with 2
      Then the property "seen" of "c" is "2 2 [] undefined"

    Scenario: A change to the current row is shown
      When the method "rename" of "root" is called with 1, "Bravo"
      And the method "recode" of "root" is called with 1, 21
      Then the property "seen" of "c" is "3 1 [Bravo] 21"
      And the property "displayText" of "c" is "Bravo"

    Scenario: A change to another row is not
      When the method "rename" of "root" is called with 0, "Able"
      Then the property "seen" of "c" is "3 1 [Beta] 20"

    Scenario: A row moved away leaves the index and changes the row
      When the method "move" of "root" is called with 1, 0
      Then the property "seen" of "c" is "3 1 [Alpha] 10"

    Scenario: Clearing the model leaves no current row
      When the method "clear" of "root" is called
      Then the property "seen" of "c" is "0 -1 [] undefined"

    Scenario: A row added to an empty model is not made current
      When the method "clear" of "root" is called
      And the method "append" of "root" is called with "New"
      Then the property "seen" of "c" is "1 -1 [] undefined"

  Rule: A new model and a bound current index

    A new model makes the first row current. A binding of the current index
    that the same change runs again is evaluated after it, wherever it is
    written, and keeps its row; one the change does not run is set over and
    still follows what it reads.

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          function abc(value) { return { value: value, choices: [{ id: "a" }, { id: "b" }, { id: "c" }] } }
          function choose(value) { both.opt = abc(value); first.opt = abc(value) }
          function offerXY(value) { both.opt = { value: value, choices: [{ id: "x" }, { id: "y" }] } }
          function wantThenList() { apart.want = 2; apart.list = ["x", "y", "z"] }
          function listThenWant() { apart.list = ["p", "q", "r"]; apart.want = 1 }
          ComboBox { id: both; objectName: "both"; property var opt: root.abc("a")
            model: opt.choices.map(c => c.id)
            currentIndex: opt.choices.findIndex(c => c.id === opt.value)
            property string seen: currentIndex + " [" + currentText + "] [" + displayText + "]" }
          ComboBox { id: first; objectName: "first"; property var opt: root.abc("a")
            currentIndex: opt.choices.findIndex(c => c.id === opt.value)
            model: opt.choices.map(c => c.id)
            property string seen: currentIndex + " [" + currentText + "]" }
          ComboBox { id: apart; objectName: "apart"; property var list: ["a", "b", "c"]; property int want: 2
            model: list; currentIndex: want
            property string seen: currentIndex + " [" + currentText + "]" }
        }
        """

    Scenario: One change that gives both a new model and a new index keeps the index
      When the method "choose" of "root" is called with "c"
      Then the property "seen" of "both" is "2 [c] [c]"
      And the property "seen" of "first" is "2 [c]"

    Scenario: The index follows its binding into a model of other rows
      When the method "offerXY" of "root" is called with "y"
      Then the property "seen" of "both" is "1 [y] [y]"

    Scenario: A new model the index binding does not read makes the first row current
      When the property "list" of "apart" is set to ["x", "y", "z"]
      Then the property "seen" of "apart" is "0 [x]"

    Scenario: The binding that was set over still follows what it reads
      When the property "list" of "apart" is set to ["x", "y", "z"]
      And the property "want" of "apart" is set to 1
      Then the property "seen" of "apart" is "1 [y]"

    Scenario: An index given before the new model is set over
      When the method "wantThenList" of "root" is called
      Then the property "seen" of "apart" is "0 [x]"

    Scenario: An index given after the new model stays
      When the method "listThenWant" of "root" is called
      Then the property "seen" of "apart" is "1 [q]"

  Rule: A bound current index over a model that grows and shrinks

    Rows put in, taken out or cleared change the count and leave a bound
    index where it is, as an index that was given; a binding that reads the
    rows runs again and gives the index it then names.

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property int want: 1
          ListModel {
            id: rows
            ListElement { name: "Alpha" }
            ListElement { name: "Beta" }
            ListElement { name: "Gamma" }
          }
          function insert() { rows.insert(0, { name: "Zero" }) }
          function remove(i) { rows.remove(i) }
          function clear() { rows.clear() }
          function append(name) { rows.append({ name: name }) }
          ComboBox { id: wanted; objectName: "wanted"; model: rows; textRole: "name"; currentIndex: root.want
            property string seen: count + " " + currentIndex + " [" + currentText + "]" }
          ComboBox { id: last; objectName: "last"; model: rows; textRole: "name"; currentIndex: rows.count - 1
            property string seen: count + " " + currentIndex + " [" + currentText + "]" }
        }
        """

    Scenario: A row put in before keeps the bound index and changes its row
      When the method "insert" of "root" is called
      Then the property "seen" of "wanted" is "4 1 [Alpha]"
      And the property "seen" of "last" is "4 3 [Gamma]"

    Scenario: A row taken out keeps the bound index
      When the method "remove" of "root" is called with 2
      Then the property "seen" of "wanted" is "2 1 [Beta]"
      And the property "seen" of "last" is "2 1 [Beta]"

    Scenario: Clearing the model leaves a bound index with no row
      When the method "clear" of "root" is called
      Then the property "seen" of "wanted" is "0 -1 []"
      And the property "seen" of "last" is "0 -1 []"

    Scenario: A row added to a cleared model is current only where the binding says so
      When the method "clear" of "root" is called
      And the method "append" of "root" is called with "New"
      Then the property "seen" of "wanted" is "1 -1 []"
      And the property "seen" of "last" is "1 0 [New]"

    Scenario: The binding cleared over still follows what it reads
      When the method "clear" of "root" is called
      And the method "append" of "root" is called with "New"
      And the property "want" of "root" is set to 0
      Then the property "seen" of "wanted" is "1 0 [New]"

  Rule: The rows can be asked for

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string got: ""
          property string flags: [Qt.MatchExactly, Qt.MatchContains, Qt.MatchStartsWith, Qt.MatchEndsWith,
            Qt.MatchWildcard, Qt.MatchFixedString, Qt.MatchCaseSensitive].join(" ")
          function find(text, flags) { got = "" + c.find(text, flags) }
          function findPlain(text) { got = "" + c.find(text) }
          function textAt(i) { got = "[" + c.textAt(i) + "]" }
          function valueAt(i) { got = "" + r.valueAt(i) }
          function indexOfValue(v) { got = "" + r.indexOfValue(v) }
          ComboBox { id: c; model: ["One", "Two", "Three", "Tw*", "two"] }
          ComboBox { id: r; y: 50; textRole: "label"; valueRole: "v"
                     model: [{ label: "A", v: 1 }, { label: "B", v: 2 }, { label: "C", v: "x" }] }
        }
        """

    @api-QtQuick.Controls.ComboBox.find
    Scenario Outline: Looking for "<text>" <how> finds row <row>
      When the method "find" of "root" is called with "<text>", <flags>
      Then the property "got" of "root" is "<row>"

      Examples:
        | text | how                                    | flags | row |
        | Two  | exactly                                | 0     | 1   |
        | two  | exactly                                | 0     | 4   |
        | TWO  | exactly                                | 0     | -1  |
        | TWO  | as a string                            | 8     | 1   |
        | two  | as a string in the same case           | 24    | 4   |
        | TWO  | as a string in the same case           | 24    | -1  |
        | one  | in the same case                       | 16    | -1  |
        | t    | at the start                           | 2     | 1   |
        | t    | at the start in the same case          | 18    | 4   |
        | HRE  | anywhere                               | 1     | 2   |
        | HRE  | anywhere in the same case              | 17    | -1  |
        | E    | at the end                             | 3     | 0   |
        | E    | at the end in the same case            | 19    | -1  |
        | T*   | as a pattern                           | 5     | 1   |
        | t*   | as a pattern in the same case          | 21    | 4   |
        | Tw?  | as a pattern                           | 5     | 1   |
        | Tw*  | exactly                                | 0     | 3   |
        | X    | anywhere                               | 1     | -1  |

    Scenario: Without flags the text is looked for exactly
      When the method "findPlain" of "root" is called with "two"
      Then the property "got" of "root" is "4"

    Scenario: The ways of looking have Qt's numbers
      Then the property "flags" of "root" is "0 1 2 3 5 8 16"

    @api-QtQuick.Controls.ComboBox.textAt
    Scenario Outline: The text of row <row> is <text>
      When the method "textAt" of "root" is called with <row>
      Then the property "got" of "root" is <text>

      Examples:
        | row | text      |
        | 0   | "[One]"   |
        | 2   | "[Three]" |
        | 9   | "[]"      |
        | -1  | "[]"      |

    @api-QtQuick.Controls.ComboBox.valueAt
    Scenario Outline: The value of row <row> is <value>
      When the method "valueAt" of "root" is called with <row>
      Then the property "got" of "root" is <value>

      Examples:
        | row | value       |
        | 0   | "1"         |
        | 2   | "x"         |
        | 9   | "undefined" |

    @api-QtQuick.Controls.ComboBox.indexOfValue
    Scenario Outline: The row with the value <value> is <row>
      When the method "indexOfValue" of "root" is called with <value>
      Then the property "got" of "root" is "<row>"

      Examples:
        | value | row |
        | 2     | 1   |
        | "x"   | 2   |
        | 7     | -1  |

  Rule: The pointer opens the popup and picks a row

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string log: ""
          property string lit: ""
          ComboBox {
            id: c; objectName: "c"
            x: 20; y: 20; width: 100; height: 30
            model: ["One", "Two", "Three"]
            delegate: ItemDelegate {
              objectName: "d" + index; width: 100; height: 30; text: modelData
              highlighted: c.highlightedIndex === index
              property string row: index + " " + modelData + " " + typeof model
            }
            onActivated: (i) => root.log += "act" + i + ";"
            onCurrentIndexChanged: root.log += "ci" + currentIndex + ";"
            onHighlighted: (i) => root.lit += i
          }
          Item { objectName: "next"; activeFocusOnTab: true; width: 10; height: 10 }
        }
        """

    Scenario: A click on the combo box opens its popup
      When the user touches 70, 35
      Then the property "popup.visible" of "c" is true
      And the property "popup.opened" of "c" is true
      And the property "highlightedIndex" of "c" is 0
      And the property "down" of "c" is true
      And the property "pressed" of "c" is false
      And the property "activeFocus" of "c" is true
      And the property "popup.activeFocus" of "c" is false

    Scenario: The popup is under the combo box, as wide as it and as high as its rows
      When the user touches 70, 35
      Then the property "popup.x" of "c" is 0
      And the property "popup.y" of "c" is 30
      And the property "popup.width" of "c" is 100
      And the property "popup.height" of "c" is 90

    @api-QtQuick.Controls.ComboBox.delegate
    Scenario Outline: The delegate of row <row> is at <y> and knows its row
      When the user touches 70, 35
      Then the property "y" of "<delegate>" is <y>
      And the property "row" of "<delegate>" is <knows>
      And the property "highlighted" of "<delegate>" is <highlighted>

      Examples:
        | row | delegate | y  | knows            | highlighted |
        | 0   | d0       | 0  | "0 One object"   | true        |
        | 1   | d1       | 30 | "1 Two object"   | false       |
        | 2   | d2       | 60 | "2 Three object" | false       |

    Scenario: A second click on the combo box closes the popup
      When the user touches 70, 35
      And the user touches 70, 35
      Then the property "popup.visible" of "c" is false
      And the property "highlightedIndex" of "c" is -1
      And the property "down" of "c" is false
      And the property "log" of "root" is "ci0;"

    @api-QtQuick.Controls.ComboBox.activated
    Scenario: A click on a row makes it current and closes the popup
      When the user touches 70, 35
      And the user touches 70, 95
      Then the property "currentIndex" of "c" is 1
      And the property "currentText" of "c" is "Two"
      And the property "displayText" of "c" is "Two"
      And the property "log" of "root" is "ci0;ci1;act1;"
      And the property "popup.visible" of "c" is false
      And the property "highlightedIndex" of "c" is -1
      And the property "down" of "c" is false

    Scenario: A click on the current row says it was activated and changes nothing
      When the user touches 70, 35
      And the user touches 70, 65
      Then the property "log" of "root" is "ci0;act0;"
      And the property "popup.visible" of "c" is false

    Scenario: A click beside the popup closes it and picks nothing
      When the user touches 70, 35
      And the user touches 300, 300
      Then the property "popup.visible" of "c" is false
      And the property "log" of "root" is "ci0;"

    Scenario: A combo box that is not enabled does not open
      When the property "enabled" of "c" is set to false
      And the user touches 70, 35
      Then the property "popup.visible" of "c" is false
      And the property "activeFocus" of "c" is false

    Scenario: The popup opened by the application shows the current row as highlighted
      When the property "currentIndex" of "c" is set to 2
      And the property "popup.visible" of "c" is set to true
      Then the property "highlightedIndex" of "c" is 2
      And the property "down" of "c" is true
      And the property "lit" of "root" is ""

    Scenario: The popup closes when the combo box is hidden
      When the property "popup.visible" of "c" is set to true
      And the property "visible" of "c" is set to false
      Then the property "popup.visible" of "c" is false

    Scenario: Setting the current index while the popup is open leaves the highlight
      When the property "popup.visible" of "c" is set to true
      And the property "currentIndex" of "c" is set to 2
      Then the property "highlightedIndex" of "c" is 0
      And the property "popup.visible" of "c" is true

    Scenario: The popup grows with the model while it is open
      When the property "popup.visible" of "c" is set to true
      And the property "model" of "c" is set to ["a", "b", "c", "d"]
      Then the property "popup.height" of "c" is 120
      And the property "popup.visible" of "c" is true

  Rule: The keys pick a row

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string log: ""
          property string lit: ""
          ComboBox {
            id: c; objectName: "c"
            x: 20; y: 20; width: 100; height: 30
            focus: true
            model: ["One", "Two", "Three", "Four"]
            delegate: ItemDelegate { width: 100; height: 30; text: modelData }
            onActivated: (i) => root.log += "act" + i + ";"
            onCurrentIndexChanged: root.log += "ci" + currentIndex + ";"
            onHighlighted: (i) => root.lit += i
          }
          Item { objectName: "next"; activeFocusOnTab: true; width: 10; height: 10 }
        }
        """

    Scenario Outline: With the popup closed, <keys> makes row <row> current
      When the user presses "<first>"
      And the user presses "<second>"
      Then the property "currentIndex" of "c" is <row>
      And the property "log" of "root" is "ci0;<log>"
      And the property "popup.visible" of "c" is false

      Examples:
        | keys                | first | second | row | log                |
        | Down twice          | Down  | Down   | 2   | ci1;act1;ci2;act2; |
        | Down and Up         | Down  | Up     | 0   | ci1;act1;ci0;act0; |
        | Up at the first row | Up    | Up     | 0   |                    |
        | End and Down        | End   | Down   | 3   | ci3;act3;          |
        | End and Home        | End   | Home   | 0   | ci3;act3;ci0;act0; |
        | the letter of a row | T     | Home   | 0   | ci1;act1;ci0;act0; |
        | a letter twice      | T     | T      | 2   | ci1;act1;ci2;act2; |
        | a letter of no row  | X     | X      | 0   |                    |
        | a small letter      | f     | Up     | 2   | ci3;act3;ci2;act2; |

    Scenario: A letter goes round to the first row that starts with it
      When the user presses "T"
      And the user presses "T"
      And the user presses "T"
      Then the property "currentIndex" of "c" is 1

    Scenario Outline: <key> opens the popup
      When the user presses "<key>"
      Then the property "popup.visible" of "c" is true
      And the property "highlightedIndex" of "c" is 0
      And the property "down" of "c" is true
      And the property "pressed" of "c" is false
      And the property "lit" of "root" is ""

      Examples:
        | key    |
        | Space  |
        | Return |
        | Enter  |

    @api-QtQuick.Controls.ComboBox.highlighted
    Scenario Outline: With the popup open, <keys> highlights row <row>
      When the user presses "Space"
      And the user presses "<first>"
      And the user presses "<second>"
      Then the property "highlightedIndex" of "c" is <row>
      And the property "lit" of "root" is "<lit>"
      And the property "currentIndex" of "c" is 0
      And the property "log" of "root" is "ci0;"

      Examples:
        | keys                | first | second | row | lit |
        | Down twice          | Down  | Down   | 2   | 12  |
        | Down and Up         | Down  | Up     | 0   | 10  |
        | Up at the first row | Up    | Up     | 0   |     |
        | End and Down        | End   | Down   | 3   | 3   |
        | End and Home        | End   | Home   | 0   | 30  |
        | a letter twice      | T     | T      | 2   | 12  |

    @api-QtQuick.Controls.ComboBox.activated
    Scenario Outline: <key> picks the highlighted row and closes the popup
      When the user presses "Space"
      And the user presses "Down"
      And the user presses "Down"
      And the user presses "<key>"
      Then the property "currentIndex" of "c" is 2
      And the property "log" of "root" is "ci0;ci2;act2;"
      And the property "popup.visible" of "c" is false
      And the property "highlightedIndex" of "c" is -1
      And the property "down" of "c" is false

      Examples:
        | key    |
        | Space  |
        | Return |
        | Enter  |

    Scenario: Picking the current row says it was activated
      When the user presses "Space"
      And the user presses "Space"
      Then the property "log" of "root" is "ci0;act0;"
      And the property "popup.visible" of "c" is false

    Scenario: Escape closes the popup and picks nothing
      When the user presses "Space"
      And the user presses "Down"
      And the user presses "Escape"
      Then the property "popup.visible" of "c" is false
      And the property "currentIndex" of "c" is 0
      And the property "log" of "root" is "ci0;"
      And the property "activeFocus" of "c" is true

    Scenario: Tab leaves the combo box and its popup closes
      When the user presses "Space"
      And the user presses "Tab"
      Then the property "activeFocus" of "next" is true
      And the property "activeFocus" of "c" is false
      And the property "popup.visible" of "c" is false

    Scenario: The keys do nothing to a combo box that does not have the keyboard
      When the method "forceActiveFocus" of "next" is called
      And the user presses "Down"
      Then the property "currentIndex" of "c" is 0

    @api-QtQuick.Controls.ComboBox.incrementCurrentIndex
    Scenario: The current index can be moved on by the application
      When the method "incrementCurrentIndex" of "c" is called
      And the method "incrementCurrentIndex" of "c" is called
      And the method "decrementCurrentIndex" of "c" is called
      Then the property "currentIndex" of "c" is 1
      And the property "log" of "root" is "ci0;ci1;act1;ci2;act2;ci1;act1;"

    Scenario: Moving on stops at the last row
      When the property "currentIndex" of "c" is set to 3
      And the method "incrementCurrentIndex" of "c" is called
      Then the property "currentIndex" of "c" is 3
      And the property "log" of "root" is "ci0;ci3;"

    Scenario: With the popup open, moving on moves the highlight
      When the property "popup.visible" of "c" is set to true
      And the method "incrementCurrentIndex" of "c" is called
      Then the property "highlightedIndex" of "c" is 1
      And the property "currentIndex" of "c" is 0

  Rule: The parts of a combo box

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 400; height: 400
          ComboBox {
            objectName: "own"
            model: ["x", "y"]
            contentItem: Item { implicitWidth: 50; implicitHeight: 20 }
            indicator: Item { implicitWidth: 10; implicitHeight: 12 }
            background: Item { implicitWidth: 30; implicitHeight: 10 }
          }
          ComboBox { objectName: "mirrored"; y: 100; model: ["a"]; LayoutMirroring.enabled: true }
          ComboBox { objectName: "flat"; y: 200; model: ["a"]; flat: true }
        }
        """

    Scenario Outline: With its own parts the combo box has <property> <value>
      Then the property "<property>" of "own" is <value>

      Examples:
        | property                | value |
        | implicitWidth           | 60    |
        | implicitHeight          | 20    |
        | rightPadding            | 10    |
        | implicitIndicatorWidth  | 10    |
        | implicitIndicatorHeight | 12    |
        | contentItem.x           | 0     |
        | contentItem.y           | 0     |
        | contentItem.width       | 50    |
        | contentItem.height      | 20    |
        | indicator.x             | 0     |
        | indicator.width         | 10    |

    Scenario Outline: Mirrored, the combo box has <property> <value>
      Then the property "<property>" of "mirrored" is <value>

      Examples:
        | property          | value |
        | mirrored          | true  |
        | leftPadding       | 40    |
        | rightPadding      | 0     |
        | indicator.x       | 0     |
        | contentItem.x     | 40    |
        | contentItem.width | 100   |

    @api-QtQuick.Controls.ComboBox.flat
    Scenario: A flat combo box shows its background only while it is down
      Then the property "background.visible" of "flat" is false
      When the property "down" of "flat" is set to true
      Then the property "background.visible" of "flat" is true

    @api-QtQuick.Controls.ComboBox.down
    Scenario: The background is darker while the combo box is down
      When the property "down" of "mirrored" is set to true
      Then the property "background.color" of "mirrored" is "#bdbdbd"

  Rule: ItemDelegate is the row of a list

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 400; height: 400
          ItemDelegate { objectName: "d" }
          ItemDelegate {
            objectName: "sized"
            y: 100; width: 200; height: 60
            highlighted: true
            contentItem: Item { implicitWidth: 30; implicitHeight: 20 }
          }
        }
        """

    @api-QtQuick.Controls.ItemDelegate
    Scenario Outline: An item delegate starts with <property> <value>
      Then the property "<property>" of "d" is <value>

      Examples:
        | property                  | value     |
        | highlighted               | false     |
        | text                      | ""        |
        | checkable                 | false     |
        | padding                   | 12        |
        | spacing                   | 8         |
        | focusPolicy               | 0         |
        | activeFocusOnTab          | false     |
        | hoverEnabled              | true      |
        | display                   | 2         |
        | icon.width                | 24        |
        | icon.height               | 24        |
        | implicitWidth             | 100       |
        | implicitHeight            | 40        |
        | background.implicitWidth  | 100       |
        | background.implicitHeight | 40        |
        | background.visible        | false     |
        | background.color          | "#f6f6f6" |
        | contentItem.x             | 12        |
        | contentItem.color         | "#353637" |

    Scenario Outline: A highlighted item delegate has <property> <value>
      Then the property "<property>" of "sized" is <value>

      Examples:
        | property           | value |
        | implicitWidth      | 100   |
        | implicitHeight     | 44    |
        | background.visible | true  |
        | background.width   | 200   |
        | background.height  | 60    |
        | contentItem.x      | 12    |
        | contentItem.y      | 12    |
        | contentItem.width  | 176   |
        | contentItem.height | 36    |

    @api-QtQuick.Controls.ItemDelegate.highlighted
    Scenario: The text of a highlighted item delegate is darker
      When the property "highlighted" of "d" is set to true
      Then the property "contentItem.color" of "d" is "#090909"
      And the property "background.visible" of "d" is true

    Scenario: The background of an item delegate that is down is darker
      When the property "down" of "d" is set to true
      Then the property "background.visible" of "d" is true
      And the property "background.color" of "d" is "#e4e4e4"

  # More of the field is in combobox-editable.feature.
  @api-QtQuick.Controls.ComboBox.editable
  Rule: An editable combo box takes text

    Scenario: The text typed into an editable combo box is its edit text
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 400; height: 400
          ComboBox { objectName: "c"; editable: true; focus: true; model: ["One", "Two"] }
        }
        """
      When the user touches 95, 20
      And the user types "x"
      Then the property "editText" of "c" is "Onex"
      And the property "currentIndex" of "c" is 0

    Scenario: Return in an editable combo box says the text was accepted
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 400; height: 400
          ComboBox {
            objectName: "c"; editable: true; focus: true; model: ["One", "Two"]
            property int times: 0
            onAccepted: times++
          }
        }
        """
      When the user touches 30, 20
      And the user presses "Return"
      Then the property "times" of "c" is 1

  Rule: A popup with more rows than fit scrolls

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 400; height: 400
          ComboBox {
            objectName: "c"; width: 100; height: 30; model: 40; focus: true
            delegate: ItemDelegate { width: 100; height: 30; text: modelData }
          }
        }
        """

    Scenario: The popup is no taller than the window less its margins
      When the property "popup.visible" of "c" is set to true
      Then the property "popup.height" of "c" is 388
      And the property "popup.y" of "c" is 6

    Scenario: The rows are in a view that is as tall as the popup
      When the property "popup.visible" of "c" is set to true
      Then the property "popup.contentItem.height" of "c" is 388
      And the property "popup.contentItem.contentHeight" of "c" is 1200
      And the property "popup.contentItem.contentY" of "c" is 0
      And the property "popup.contentItem.clip" of "c" is true

    Scenario Outline: The popup opens with the current row <row> at the top, as far as the rows go
      When the property "currentIndex" of "c" is set to <row>
      And the property "popup.visible" of "c" is set to true
      Then the property "popup.contentItem.contentY" of "c" is <at>

      Examples:
        | row | at  |
        | 2   | 60  |
        | 13  | 390 |
        | 20  | 600 |
        | 30  | 812 |
        | 39  | 812 |

    Scenario: The row a key highlights is brought into view
      When the property "popup.visible" of "c" is set to true
      And the user presses "End"
      And 1 second passes
      Then the property "highlightedIndex" of "c" is 39
      And the property "popup.contentItem.contentY" of "c" is 812

    Scenario: A row that is in view moves nothing
      When the property "popup.visible" of "c" is set to true
      And the user presses "End"
      And the user presses "Up"
      And 1 second passes
      Then the property "highlightedIndex" of "c" is 38
      And the property "popup.contentItem.contentY" of "c" is 812

    Scenario: The first row is brought back into view
      When the property "popup.visible" of "c" is set to true
      And the user presses "End"
      And 1 second passes
      And the user presses "Home"
      And 1 second passes
      Then the property "popup.contentItem.contentY" of "c" is 0

    Scenario: A touch chooses the row that is seen there
      When the property "currentIndex" of "c" is set to 20
      And the property "popup.visible" of "c" is set to true
      And the user touches 50, 100
      Then the property "currentIndex" of "c" is 23
      And the property "popup.visible" of "c" is false

  Rule: A popup that does not fit below the combo box opens above it

    Issue #152. The popup is mirrored around the combo box when less of
    it runs past the window, or past a margin it has, there; then, when
    it is no taller than the room, it is pushed inside.

    @api-QtQuick.Controls.ComboBox.popup
    Scenario Outline: A popup <h> high under a combo box at <cy>, margins <m>, is at <y>
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Window {
          width: 400; height: 400; visible: true
          ComboBox {
            id: c; objectName: "c"
            x: 20; y: <cy>; width: 160; height: 40
            model: ["One", "Two"]
            popup: Popup { y: c.height + 4; width: 160; height: <h>; margins: <m> }
            Component.onCompleted: popup.open()
          }
        }
        """
      When 100 milliseconds pass
      Then the property "popup.y" of "c" is <y>

      Examples:
        | cy  | h   | m  | y    |
        | 340 | 204 | -1 | -208 |
        | 180 | 304 | -1 | -84  |
        | 190 | 304 | -1 | -190 |
        | 300 | 56  | -1 | 44   |
        | 300 | 57  | -1 | -61  |
        | 10  | 204 | -1 | 44   |
        | 10  | 390 | -1 | 0    |
        | 225 | 204 | 20 | -205 |
        | 180 | 304 | 20 | -104 |
        | 300 | 370 | 20 | -280 |

    Scenario: A popup above a combo box near the top opens below it
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Window {
          width: 400; height: 400; visible: true
          ComboBox {
            id: c; objectName: "c"
            x: 20; y: 10; width: 160; height: 40
            model: ["One", "Two"]
            popup: Popup { y: -64; width: 160; height: 60 }
            Component.onCompleted: popup.open()
          }
        }
        """
      When 100 milliseconds pass
      Then the property "popup.y" of "c" is 44

    Scenario: A plain popup that does not fit stays where it was put
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Window {
          width: 400; height: 400; visible: true
          Item {
            objectName: "i"
            x: 20; y: 340; width: 160; height: 40
            Popup {
              objectName: "p"; y: 44; width: 160; height: 204
              Component.onCompleted: open()
            }
          }
        }
        """
      When 100 milliseconds pass
      Then the property "y" of "p" is 44
