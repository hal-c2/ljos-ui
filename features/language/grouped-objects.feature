# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html
#   https://doc.qt.io/qt-6/qtqml-syntax-signals.html
#   https://doc.qt.io/qt-6/qtqml-uiforms.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Members of an object a property holds
  A property that holds an object, an alias of an object's id or a property declared with an
  object type, is a group to whoever uses the component: `form.button.text: ...` binds a property
  of that object and `form.button.onClicked: ...` handles its signal. A `.ui.qml` form hands its
  controls out this way, and the file that uses the form writes their bindings and handlers.

  Background:
    Given the file "Counter.qml":
      """qml
      import QtQuick
      Item {
        property int count: 0
        property string label: "none"
        signal poked(int how)
      }
      """
    And the file "Form.qml":
      """qml
      import QtQuick
      Item {
        property alias counter: inner
        property alias holder: box
        property Item plain: spot
        Counter { id: inner; objectName: "inner" }
        Holder { id: box }
        Item { id: spot; objectName: "plain" }
      }
      """
    And the file "Holder.qml":
      """qml
      import QtQuick
      Item {
        property alias counter: deep
        Counter { id: deep; objectName: "deep" }
      }
      """

  Rule: A binding through the group binds the object's property

    Scenario: A binding through an alias of an object
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int n: 1
          Form { counter.label: "seen " + n }
        }
        """
      Then the property "label" of "inner" is "seen 1"
      When the property "n" of "root" is set to 2
      Then the property "label" of "inner" is "seen 2"

    Scenario: A binding through a property that holds an object
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int n: 4
          Form { plain.width: n * 10 }
        }
        """
      Then the property "width" of "plain" is 40

  Rule: A handler through the group handles the object's signal

    Scenario: A change signal through an alias of an object
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int seen: 0
          Form { counter.onCountChanged: seen = counter.count }
        }
        """
      When the property "count" of "inner" is set to 3
      Then the property "seen" of "root" is 3

    Scenario: A signal with an argument through an alias of an object
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int got: 0
          Form { counter.onPoked: (how) => got = how }
        }
        """
      When the method "poked" of "inner" is called with 7
      Then the property "got" of "root" is 7

    Scenario: A handler through the group names the signal's parameters
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int got: 0
          Form { counter.onPoked: got = how }
        }
        """
      When the method "poked" of "inner" is called with 5
      Then the property "got" of "root" is 5

    Scenario: A handler through an alias of an alias
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int seen: 0
          Form { holder.counter.onCountChanged: seen = holder.counter.count }
        }
        """
      When the property "count" of "deep" is set to 4
      Then the property "seen" of "root" is 4

    Scenario: A handler through a property that holds an object
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property real seen: 0
          Form { plain.onXChanged: seen = plain.x }
        }
        """
      When the property "x" of "plain" is set to 9
      Then the property "seen" of "root" is 9

    Scenario: The handler sees the names of the file it is written in
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string said: ""
          function say(what) { said = what }
          Form { counter.onLabelChanged: say(counter.label) }
        }
        """
      When the property "label" of "inner" is set to "hi"
      Then the property "said" of "root" is "hi"
