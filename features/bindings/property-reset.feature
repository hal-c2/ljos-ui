# Sources:
#   https://doc.qt.io/qt-6/qtqml-cppintegration-exposecppattributes.html#exposing-properties
#   https://doc.qt.io/qt-6/properties.html
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#implicitWidth-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#anchors.margins-prop
#   https://doc.qt.io/qt-6/qtquick-positioning-anchors.html
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#sourceComponent-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Resetting a property with undefined
  Some properties can be taken back to the state they had before anything
  gave them a value: a size follows the implicit size again, an anchor is
  gone, a side margin follows the margins again. Assigning undefined does
  that. A property that has no such state refuses undefined and keeps
  its value.

  Rule: A size goes back to the implicit size

    Scenario: A script takes a given size back
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "box"
          implicitWidth: 40; implicitHeight: 30
          width: 80; height: 60
          function loosen() { width = undefined; height = undefined }
        }
        """
      When the method "loosen" of "box" is called
      Then the property "width" of "box" is 40
      And the property "height" of "box" is 30

    Scenario: A size that was taken back follows the implicit size from then on
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "box"
          implicitWidth: 40
          width: 80
          function loosen() { width = undefined }
        }
        """
      When the method "loosen" of "box" is called
      And the property "implicitWidth" of "box" is set to 55
      Then the property "width" of "box" is 55

    Scenario: A binding that yields undefined gives the implicit size, and its next value the given size
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property bool loose: false
          Rectangle {
            objectName: "box"
            implicitWidth: 33
            width: root.loose ? undefined : 70
          }
        }
        """
      Then the property "width" of "box" is 70
      When the property "loose" of "root" is set to true
      Then the property "width" of "box" is 33
      And the log has no warnings
      When the property "loose" of "root" is set to false
      Then the property "width" of "box" is 70

    Scenario: Taking a bound size back ends the binding
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property real wanted: 60
          Item {
            objectName: "box"
            implicitWidth: 11
            width: root.wanted
            function loosen() { width = undefined }
          }
        }
        """
      When the method "loosen" of "box" is called
      And the property "wanted" of "root" is set to 99
      Then the property "width" of "box" is 11

    Scenario: An alias takes back what it names
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property alias inner: box.width
          Rectangle { id: box; objectName: "box"; implicitWidth: 40; width: 80 }
          function loosen() { inner = undefined }
        }
        """
      When the method "loosen" of "root" is called
      Then the property "width" of "box" is 40
      And the property "inner" of "root" is 40

    Scenario: A text is as wide as its words again
      Given the QML:
        """qml
        import QtQuick
        Text {
          objectName: "words"
          text: "hello"
          width: 300
          property bool natural: width === implicitWidth
          function loosen() { width = undefined }
        }
        """
      Then the property "natural" of "words" is false
      When the method "loosen" of "words" is called
      Then the property "natural" of "words" is true

  Rule: An anchor is taken away

    Scenario: A filling item keeps its size where the anchor left it
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 200; height: 100
          Rectangle {
            objectName: "box"
            anchors.fill: parent
            anchors.margins: 5
            property bool filling: anchors.fill !== null
            function release() { anchors.fill = undefined }
          }
        }
        """
      Then the property "filling" of "box" is true
      When the method "release" of "box" is called
      Then the property "filling" of "box" is false
      And the property "width" of "box" is 190
      When the property "anchors.margins" of "box" is set to 20
      Then the property "width" of "box" is 190
      And the property "x" of "box" is 5

    Scenario Outline: <anchor> is released by undefined
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          width: 200; height: 100
          Rectangle {
            objectName: "box"
            width: 20; height: 10
            <anchor>: <to>
            function release() { <anchor> = undefined }
          }
        }
        """
      Then the property "<moved>" of "box" is <placed>
      When the method "release" of "box" is called
      And the property "width" of "root" is set to 400
      And the property "height" of "root" is set to 300
      Then the property "<moved>" of "box" is <placed>

      Examples:
        | anchor                   | to                     | moved | placed |
        | anchors.left             | parent.horizontalCenter | x     | 100    |
        | anchors.right            | parent.right           | x     | 180    |
        | anchors.horizontalCenter | parent.horizontalCenter | x     | 90     |
        | anchors.top              | parent.verticalCenter  | y     | 50     |
        | anchors.bottom           | parent.bottom          | y     | 90     |
        | anchors.verticalCenter   | parent.verticalCenter  | y     | 45     |
        | anchors.centerIn         | parent                 | x     | 90     |

    Scenario: An anchor bound to undefined is not there, and comes back with the next value
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          width: 200; height: 100
          property bool free: false
          Rectangle {
            objectName: "box"
            width: 50; height: 10
            anchors.right: root.free ? undefined : root.right
          }
        }
        """
      Then the property "x" of "box" is 150
      When the property "free" of "root" is set to true
      And the property "width" of "root" is set to 300
      Then the property "x" of "box" is 150
      And the log has no warnings
      When the property "free" of "root" is set to false
      Then the property "x" of "box" is 250

  Rule: A side margin follows the margins again

    Scenario: A side margin of its own is taken back
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 200; height: 100
          Rectangle {
            objectName: "box"
            width: 50; height: 10
            anchors.left: parent.left
            anchors.margins: 5
            anchors.leftMargin: 9
            function follow() { anchors.leftMargin = undefined }
          }
        }
        """
      Then the property "x" of "box" is 9
      When the method "follow" of "box" is called
      Then the property "anchors.leftMargin" of "box" is 5
      And the property "x" of "box" is 5
      When the property "anchors.margins" of "box" is set to 12
      Then the property "x" of "box" is 12

    Scenario Outline: <side> follows the margins when its binding yields undefined
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          width: 200; height: 100
          property bool own: true
          Rectangle {
            objectName: "box"
            width: 50; height: 10
            anchors.fill: parent
            anchors.margins: 3
            <side>: root.own ? 20 : undefined
          }
        }
        """
      Then the property "<side>" of "box" is 20
      When the property "own" of "root" is set to false
      Then the property "<side>" of "box" is 3
      And the log has no warnings
      When the property "own" of "root" is set to true
      Then the property "<side>" of "box" is 20

      Examples:
        | side                 |
        | anchors.leftMargin   |
        | anchors.rightMargin  |
        | anchors.topMargin    |
        | anchors.bottomMargin |

  Rule: Other properties with a reset go back to what they were

    Scenario Outline: <property> of <type> after undefined is <after>
      Given the QML:
        """qml
        import QtQuick
        <type> {
          objectName: "it"
          <given>
          function clear() { <property> = undefined }
        }
        """
      Then the property "<property>" of "it" is <before>
      When the method "clear" of "it" is called
      Then the property "<property>" of "it" is <after>

      Examples:
        | type      | given                                      | property             | before | after      |
        | Text      | horizontalAlignment: Text.AlignRight       | horizontalAlignment  | 2      | 1          |
        | Text      | maximumLineCount: 2                        | maximumLineCount     | 2      | 2147483647 |
        | TextInput | horizontalAlignment: TextInput.AlignHCenter | horizontalAlignment | 4      | 1          |
        | TextInput | padding: 4; topPadding: 10                 | topPadding           | 10     | 4          |
        | TextEdit  | horizontalAlignment: TextEdit.AlignRight   | horizontalAlignment  | 2      | 1          |
        | TextEdit  | padding: 4; rightPadding: 10               | rightPadding         | 10     | 4          |
        | Column    | padding: 5; topPadding: 9                  | topPadding           | 9      | 5          |
        | Row       | padding: 5; leftPadding: 9                 | leftPadding          | 9      | 5          |
        | Grid      | padding: 5                                 | padding              | 5      | 0          |
        | Flow      | padding: 5; rightPadding: 9                | rightPadding         | 9      | 5          |
        | Rectangle | radius: 4; antialiasing: false             | antialiasing         | false  | true       |
        | Item      | antialiasing: true                         | antialiasing         | true   | false      |
        | TextInput | passwordMaskDelay: 300                     | passwordMaskDelay    | 300    | 0          |
        | Text      | padding: 4; leftPadding: 10                | leftPadding          | 10     | 4          |
        | Text      | padding: 4; bottomPadding: 10              | bottomPadding        | 10     | 4          |
        | Text      | padding: 4                                 | padding              | 4      | 0          |
        | Rectangle | radius: 4; topLeftRadius: 9                | topLeftRadius        | 9      | 4          |
        | Rectangle | radius: 4; bottomRightRadius: 9            | bottomRightRadius    | 9      | 4          |

      # Properties that are not there yet: the cursor shape of a MouseArea.
      @backlog
      Examples:
        | type      | given                                      | property             | before | after      |
        | MouseArea | cursorShape: Qt.PointingHandCursor         | cursorShape          | 13     | 0          |

    Scenario: A side padding that was taken back follows the padding from then on
      Given the QML:
        """qml
        import QtQuick
        TextInput {
          objectName: "words"
          text: "hello"
          padding: 4
          leftPadding: 10
          function follow() { leftPadding = undefined }
        }
        """
      When the method "follow" of "words" is called
      And the property "padding" of "words" is set to 7
      Then the property "leftPadding" of "words" is 7

    Scenario: A Loader with its component taken back shows nothing
      Given the QML:
        """qml
        import QtQuick
        Loader {
          objectName: "loader"
          sourceComponent: Component { Item {} }
          property bool shows: item !== null
          function unload() { sourceComponent = undefined }
        }
        """
      Then the property "shows" of "loader" is true
      When the method "unload" of "loader" is called
      Then the property "shows" of "loader" is false
      And the property "status" of "loader" is 0

    # Qt lays the grid out again only with the next change, so the places
    # are looked at after one.
    Scenario: A grid lays out with its spacing again and its row spacing reads as it was
      Given the QML:
        """qml
        import QtQuick
        Grid {
          objectName: "grid"
          columns: 2; spacing: 4; rowSpacing: 9; columnSpacing: 7
          property real across: last.x
          property real down: last.y
          Repeater { model: 3; Item { width: 10; height: 10 } }
          Item { id: last; width: 10; height: 10 }
          function follow() { rowSpacing = undefined; columnSpacing = undefined }
        }
        """
      Then the property "across" of "grid" is 17
      And the property "down" of "grid" is 19
      When the method "follow" of "grid" is called
      And the property "spacing" of "grid" is set to 5
      And 50 milliseconds pass
      Then the property "across" of "grid" is 15
      And the property "down" of "grid" is 15
      And the property "rowSpacing" of "grid" is 9
      And the property "columnSpacing" of "grid" is 7

    Scenario: A cell's own margin is taken back
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          objectName: "row"
          property real seen: cell.Layout.leftMargin
          Rectangle { id: cell; Layout.margins: 6; Layout.leftMargin: 2 }
          function follow() { cell.Layout.leftMargin = undefined }
        }
        """
      Then the property "seen" of "row" is 2
      When the method "follow" of "row" is called
      Then the property "seen" of "row" is 6

  Rule: A property without a reset refuses undefined

    Scenario Outline: <property> of <type> keeps its value and the script gets an error
      Given the QML:
        """qml
        import QtQuick
        <type> {
          objectName: "it"
          property string said
          <property>: <given>
          function clear() {
            try { <property> = undefined } catch (e) { said = String(e) }
          }
        }
        """
      When the method "clear" of "it" is called
      Then the property "said" of "it" is "Error: Cannot assign [undefined] to <class>"
      And the property "<property>" of "it" is <kept>

      Examples:
        | type      | property        | given  | class   | kept      |
        | Item      | x               | 7      | double  | 7         |
        | Item      | z               | 2      | double  | 2         |
        | Item      | opacity         | 0.5    | double  | 0.5       |
        | Item      | visible         | false  | bool    | false     |
        | Item      | clip            | true   | bool    | true      |
        | Item      | implicitWidth   | 12     | double  | 12        |
        | Item      | anchors.margins | 4      | double  | 4         |
        | Rectangle | color           | "red"  | QColor  | "#ff0000" |
        | Rectangle | radius          | 3      | double  | 3         |
        | Rectangle | border.width    | 2      | double  | 2         |
        | Text      | text            | "hi"   | QString | "hi"      |
        | Loader    | source          | ""     | QUrl    | ""        |
        | Grid      | spacing         | 5      | double  | 5         |
        | Item      | state           | "on"   | QString | "on"      |

    Scenario: A font is not taken back whole
      Given the QML:
        """qml
        import QtQuick
        Text {
          objectName: "words"
          property string said
          font.pixelSize: 30
          function clear() {
            try { font = undefined } catch (e) { said = String(e) }
          }
        }
        """
      When the method "clear" of "words" is called
      Then the property "said" of "words" is "Error: Cannot assign [undefined] to QFont"
      And the property "font.pixelSize" of "words" is 30

    Scenario Outline: A property declared as <kind> in QML refuses undefined
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "it"
          property string said
          property <kind> own: <given>
          function clear() {
            try { own = undefined } catch (e) { said = String(e) }
          }
        }
        """
      When the method "clear" of "it" is called
      Then the property "said" of "it" is "Error: Cannot assign [undefined] to <class>"
      And the property "own" of "it" is <kept>

      Examples:
        | kind   | given  | class   | kept      |
        | int    | 5      | int     | 5         |
        | real   | 2.5    | double  | 2.5       |
        | bool   | true   | bool    | true      |
        | string | "x"    | QString | "x"       |
        | color  | "red"  | QColor  | "#ff0000" |
        | url    | "a.b"  | QUrl    | "a.b"     |

    Scenario: A var property holds undefined as a value
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "it"
          property var own: 3
          property bool empty: own === undefined
          function clear() { own = undefined }
        }
        """
      Then the property "empty" of "it" is false
      When the method "clear" of "it" is called
      Then the property "empty" of "it" is true

    Scenario Outline: A binding on <property> that yields undefined keeps the value and warns
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property bool gone: false
          Rectangle {
            objectName: "box"
            <property>: root.gone ? undefined : <given>
          }
        }
        """
      When the property "gone" of "root" is set to true
      Then the property "<property>" of "box" is <kept>
      And the log has a warning matching "Scenario.qml:7:\d+: .*Unable to assign \[undefined\] to "

      Examples:
        | property | given | kept      |
        | x        | 7     | 7         |
        | opacity  | 0.5   | 0.5       |
        | color    | "red" | "#ff0000" |

  Rule: A property that holds an object refuses undefined and takes null

    Scenario Outline: A property declared as <kind> keeps its object
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "it"
          property bool refused
          property <kind> own: <given>
          property bool holds: own !== null
          Rectangle { id: box }
          Component { id: part; Item {} }
          function clear() {
            try { own = undefined } catch (e) {
              refused = String(e).indexOf("Error: Cannot assign [undefined] to ") === 0
            }
          }
        }
        """
      When the method "clear" of "it" is called
      Then the property "refused" of "it" is true
      And the property "holds" of "it" is true
      When the property "own" of "it" is set to null
      Then the property "holds" of "it" is false

      Examples:
        | kind      | given |
        | Item      | box   |
        | Rectangle | box   |
        | QtObject  | root  |
        | Component | part  |

    Scenario: An item keeps its parent
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property bool refused
          property bool inside: box.parent === root
          Rectangle { id: box }
          function orphan() {
            try { box.parent = undefined } catch (e) {
              refused = String(e).indexOf("Error: Cannot assign [undefined] to ") === 0
            }
          }
          function release() { box.parent = null }
        }
        """
      When the method "orphan" of "root" is called
      Then the property "refused" of "root" is true
      And the property "inside" of "root" is true
      When the method "release" of "root" is called
      Then the property "inside" of "root" is false

    Scenario Outline: A binding on a property declared as <kind> that yields undefined keeps the object and warns
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property bool gone: false
          property <kind> own: gone ? undefined : <given>
          property bool same: own === <given>
          Rectangle { id: box }
          Component { id: part; Item {} }
        }
        """
      When the property "gone" of "root" is set to true
      Then the property "same" of "root" is true
      And the log has a warning matching "Scenario.qml:5:\d+: .*Unable to assign \[undefined\] to "

      Examples:
        | kind      | given |
        | Item      | box   |
        | QtObject  | box   |
        | Component | part  |

    Scenario: An object property that starts as undefined holds nothing
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "it"
          property Item own: undefined
          property bool holds: own !== null
        }
        """
      Then the property "holds" of "it" is false
      And the log has a warning matching "Scenario.qml:4:\d+: .*Unable to assign \[undefined\] to "
