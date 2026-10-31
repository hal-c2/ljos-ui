# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-row.html
#   https://doc.qt.io/qt-6/qml-qtquick-column.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (geometry).

@qt-parity
Feature: Positioner polish settles

  Scenario: A Row settles when a child height reads its implicit width
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Row {
          id: row
          objectName: "row"
          Rectangle {
            width: 100
            height: -Math.log(row.implicitWidth)
          }
        }
      }
      """
    Then the log has no warning matching "(?i)polish loop"
    And the property "implicitWidth" of "row" is 100

  # Ljós re-lays out a child that follows its own positioner's size on every pass and warns;
  # Qt settles after one. Kept in crates/ljos_fuzz/known/geometry.txt until it does.
  @backlog
  Scenario: A Row and nested Column settle when children follow the Column size
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Row {
          objectName: "row"
          Column {
            id: column
            objectName: "column"
            Item { width: 7.5; height: 100 }
            Item { width: column.width; height: column.height }
          }
        }
      }
      """
    Then the log has no warning matching "(?i)polish loop"
    And the property "implicitWidth" of "column" is 7.5
    And the property "implicitHeight" of "column" is 100

  # Ljós re-lays out a child that follows its own positioner's size on every pass and warns;
  # Qt settles after one. Kept in crates/ljos_fuzz/known/geometry.txt until it does.
  @backlog
  Scenario: Nested Columns settle when a child follows their sizes
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Column {
          id: outer
          objectName: "outer"
          Column {
            id: inner
            objectName: "inner"
            Rectangle { implicitWidth: 42; implicitHeight: 25 }
            Item { width: outer.implicitHeight; implicitHeight: inner.height }
          }
        }
      }
      """
    Then the log has no warning matching "(?i)polish loop"
    And the property "implicitWidth" of "outer" is 42
    And the property "implicitHeight" of "outer" is 25
    And the property "implicitHeight" of "inner" is 25

  # Ljós re-lays out a child that follows its own positioner's size on every pass and warns;
  # Qt settles after one. Kept in crates/ljos_fuzz/known/geometry.txt until it does.
  @backlog
  Scenario: A Column settles when completion changes its spacing
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Column {
          id: outer
          objectName: "outer"
          Column {
            id: inner
            objectName: "inner"
            Rectangle {
              implicitWidth: 42
              implicitHeight: 25
              Component.onCompleted: { inner.spacing = 42; }
            }
            Item { width: outer.implicitHeight; implicitHeight: inner.height }
          }
        }
      }
      """
    Then the log has no warning matching "(?i)polish loop"
    And the property "spacing" of "inner" is 42
    And the property "implicitHeight" of "inner" is 25

  Scenario: A Column lays out spacing changed in its own completion
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Column {
          id: col
          objectName: "col"
          Component.onCompleted: spacing = 20
          Rectangle { width: 10; height: 10 }
          Rectangle { objectName: "b"; width: 10; height: 10 }
        }
      }
      """
    When 50 milliseconds pass
    Then the property "y" of "b" is 30
    And the property "implicitHeight" of "col" is 40

  Scenario: A Column lays out spacing changed in a child completion
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Column {
          id: col
          objectName: "col"
          Rectangle { width: 10; height: 10; Component.onCompleted: col.spacing = 20 }
          Rectangle { objectName: "b"; width: 10; height: 10 }
        }
      }
      """
    When 50 milliseconds pass
    Then the property "y" of "b" is 30
    And the property "implicitHeight" of "col" is 40

  Scenario: A Column lays out a bound child height changed on completion
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property real h: 10
        Component.onCompleted: h = 30
        Column {
          objectName: "col"
          Rectangle { width: 10; height: root.h }
          Rectangle { objectName: "b"; width: 10; height: 10 }
        }
      }
      """
    When 50 milliseconds pass
    Then the property "y" of "b" is 30
    And the property "implicitHeight" of "col" is 40

  Scenario: A Row lays out a finite child size produced by its own pass
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Row {
          id: row
          objectName: "row"
          Item { width: 10; height: 5 }
          Item { objectName: "child"; width: row.implicitWidth > 0 ? 5 : 20; height: 5 }
        }
      }
      """
    When 50 milliseconds pass
    Then the log has no warning matching "(?i)polish loop"
    And the property "implicitWidth" of "row" is 15
    And the property "width" of "child" is 5

  Scenario: A zero-width child that gets a width joins nested Rows
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        function go() { o3.width = Qt.binding(function () { return Math.sign(o1.height); }) }
        Row {
          id: o1
          objectName: "o1"
          Row {
            Rectangle { id: o3; height: Math.atan2(100, o1.width) }
            Item { width: 1.25; height: 100 }
          }
        }
      }
      """
    When the method "go" of "root" is called
    And 50 milliseconds pass
    Then the property "width" of "o1" is 2.25

  # Found by ljos-fuzz (geometry), buckets value:step-positioner:number and
  # value:step-in-positioner:rounding.
  @backlog
  Scenario: A child given a size in a method joins its Column at the next polish
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property real sameTurn: -1
        function give() { child.height = 40; sameTurn = column.implicitHeight }
        Column {
          id: column
          objectName: "column"
          Item { id: child; width: 10 }
        }
      }
      """
    When the method "give" of "root" is called
    Then the property "sameTurn" of "root" is 0
    When 50 milliseconds pass
    Then the property "implicitHeight" of "column" is 40
