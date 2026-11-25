# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html
# Verified on Qt 6.12.0 (qmltestrunner, offscreen).
# A final member is one no type deriving from the type that has it may declare a member of the
# name of: Qt refuses to make such a type ("Cannot override FINAL property"). Some members of the
# built-in types are final (Item's geometry, anchor lines and focus, Rectangle's corner radii);
# a QML file makes one of its own with `final property` (Qt 6.10). Only a property declared
# over one is refused: a binding to it, or a function or signal of its name, is not.

@qt-parity
Feature: A type cannot redeclare a final member of the type it derives from
  A component that declares a property, or an alias, of the name of a final
  member of its type does not load, and whatever makes an object of its file
  is told where the declaration is. `final property` declares a property
  that is final in this way.

  Rule: A built-in type's final member cannot be declared again

    Scenario Outline: <declaration> on <type> does not load
      Given the file "Final.qml":
        """qml
        import QtQuick
        <type> {
          Item { id: other; property int sample }
          <declaration>
        }
        """
      Then loading "Final.qml" is rejected

      Examples:
        | type      | declaration                       |
        | Item      | property bool left: false         |
        | Item      | property real x                   |
        | Item      | property real width               |
        | Item      | property var anchors              |
        | Item      | property bool focus               |
        | Item      | property bool activeFocus         |
        | Item      | property var parent               |
        | Item      | property var layer                |
        | Item      | property real opacity             |
        | Item      | property alias left: other.sample |
        | Item      | final property int x              |
        | Item      | readonly property int baseline    |
        | Rectangle | property bool left                |
        | Rectangle | property real topLeftRadius       |
        | MouseArea | property real x                   |
        | Text      | property real width               |

    Scenario Outline: <declaration> on <type> loads
      Given the QML:
        """qml
        import QtQuick
        <type> {
          objectName: "root"
          <declaration>
          property bool loaded: true
        }
        """
      Then the property "loaded" of "root" is true

      Examples:
        | type      | declaration                |
        | Item      | property string objectName |
        | Item      | property bool clip         |
        | Item      | property string state      |
        | Item      | property real rotation     |
        | Rectangle | property real radius       |
        | Item      | function left() { }        |
        | Item      | signal width()             |
        | Item      | x: 2                       |

    Scenario Outline: A base's <member> called left does not hide Item's final left
      Given the file "Base.qml":
        """qml
        import QtQuick
        Item {
          <member>
        }
        """
      And the file "Final.qml":
        """qml
        import QtQuick
        Base {
          property bool left
        }
        """
      Then loading "Final.qml" is rejected

      Examples:
        | member              |
        | function left() { } |
        | signal left()       |

  Rule: `final property` declares a property no derived type may declare again

    Scenario Outline: `<declaration>` declares a property
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          <declaration>
        }
        """
      Then the property "count" of "root" is 2

      Examples:
        | declaration                          |
        | final property int count: 2          |
        | readonly final property int count: 2 |
        | final readonly property int count: 2 |
        | default final property int count: 2  |

    Scenario: A property called final is a property like any other
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int final
          final: 2
          final property int count: final
        }
        """
      Then the property "count" of "root" is 2

    Scenario Outline: A file that redeclares <what> of its QML base does not load
      Given the file "Base.qml":
        """qml
        import QtQuick
        Item {
          final property int count: 1
        }
        """
      And the file "Final.qml":
        """qml
        import QtQuick
        Base {
          Item { id: other; property int sample }
          <declaration>
        }
        """
      Then loading "Final.qml" is rejected

      Examples:
        | what              | declaration                        |
        | a final property  | property int count                 |
        | it as final again | final property int count           |
        | it as an alias    | property alias count: other.sample |

    Scenario Outline: A file may declare again <what> of its QML base
      Given the file "Base.qml":
        """qml
        import QtQuick
        Item {
          id: base
          property int n: 1
          <alias>
        }
        """
      And the QML:
        """qml
        import QtQuick
        Base {
          objectName: "root"
          property int count: 2
        }
        """
      Then the property "count" of "root" is 2

      Examples:
        | what                       | alias                              |
        | an alias it calls final    | final property alias count: base.n |
        | an alias of Item's final x | property alias count: base.x       |

    Scenario: A file may declare again a property its QML base does not make final
      Given the file "Base.qml":
        """qml
        import QtQuick
        Item {
          property int count: 1
        }
        """
      And the QML:
        """qml
        import QtQuick
        Base {
          objectName: "root"
          property int count: 2
          function bump() { }
        }
        """
      Then the property "count" of "root" is 2

  Rule: What makes an object of the file is told where the declaration is

    Scenario: A Loader of a file that declares Item's left says where
      Given the file "Final.qml":
        """qml
        import QtQuick
        Item {
          property bool left: false
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Loader { objectName: "loader"; source: "Final.qml" }
        }
        """
      Then the property "status" of "loader" is 3
      And the log has a warning matching "Final\.qml:3:11: Cannot override FINAL property"
