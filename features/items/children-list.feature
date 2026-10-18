# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#children-prop
#   https://doc.qt.io/qt-6/qml-list.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A script reads an item's children through to the item
  What a script is given of children, resources or data is not an array of its own but the
  item's list: its length and entries are what the item holds whenever they are read, pushing an
  item to it makes that item a child, and setting its length lets the rest go. Each read gives
  a new list object; the entries cannot be replaced or reordered in place.

  Rule: The list is the item's, read whenever it is read

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property string seen: ""
          Item { objectName: "a" }
          Item { objectName: "b" }
          Item { objectName: "other" }
          Component { id: comp; Item {} }
          function held() {
            var l = root.children
            comp.createObject(root, { objectName: "c" })
            seen = l.length + " " + l[3].objectName
          }
          function what() {
            seen = (root.children === root.children) + " " + Array.isArray(root.children)
              + " " + Object.prototype.toString.call(root.children)
          }
          function expando() { var l = root.children; l.mark = 1; seen = l.mark + " " + root.children.mark }
        }
        """

    Scenario: A list a script holds reads the children added later
      When the method "held" of "root" is called
      Then the property "seen" of "root" is "4 c"

    Scenario: Each read is a new list object that is not an array
      When the method "what" of "root" is called
      Then the property "seen" of "root" is "false false [object QML List]"

    Scenario: A property set on the list is that list object's alone
      When the method "expando" of "root" is called
      Then the property "seen" of "root" is "1 undefined"

  Rule: The array functions read the list as an array

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property string seen: ""
          Item { objectName: "a" }
          Item { id: b; objectName: "b" }
          Item { objectName: "other" }
          function names(l) { return l.map(function (o) { return o.objectName }).join(" ") }
          function walk() {
            var of = []
            for (var o of root.children) of.push(o.objectName)
            var each = 0
            root.children.forEach(function () { each++ })
            seen = names(root.children) + "|" + of.join(" ") + "|" + each + "|"
              + [...root.children].length + " " + Object.keys(root.children).join(",") + "|"
              + root.children[7] + " " + root.children.indexOf(b) + " " + Array.isArray(root.children.slice())
          }
        }
        """

    Scenario: The array functions, iteration and indexing read the children
      When the method "walk" of "root" is called
      Then the property "seen" of "root" is "a b other|a b other|3|3 0,1,2|undefined 1 true"

  Rule: An entry is not replaced, and the order stays

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property string seen: ""
          Item { id: a; objectName: "a" }
          Item { objectName: "b" }
          Item { id: other; objectName: "other" }
          function names(l) { return l.map(function (o) { return o.objectName }).join(" ") }
          function push() {
            var l = other.children
            l.push(a)
            seen = (a.parent === other) + " " + root.children.length + " " + other.children.length + " " + l.length
          }
          function sort() {
            try { root.children.sort(); seen = "sorted" } catch (e) { seen = (e instanceof TypeError) + "" }
            seen += " " + names(root.children)
          }
          function replace() { root.children[0] = other; seen = names(root.children) }
        }
        """

    Scenario: Pushing an item to another item's children makes it that item's child
      When the method "push" of "root" is called
      Then the property "seen" of "root" is "true 2 1 1"

    Scenario: Sorting the children throws a TypeError and leaves their order
      When the method "sort" of "root" is called
      Then the property "seen" of "root" is "true a b other"

    Scenario: Assigning an entry leaves the children as they are
      When the method "replace" of "root" is called
      Then the property "seen" of "root" is "a b other"

  Rule: Shortening the list lets the children after it go

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property string seen: ""
          property int kids: children.length
          Item { objectName: "a" }
          Item { id: b; objectName: "b" }
          Item { id: other; objectName: "other" }
          function names(l) { return l.map(function (o) { return o.objectName }).join(" ") }
          function cut() {
            root.children.length = 1
            seen = names(root.children) + " " + (b.parent === null) + " " + (other.parent === null)
          }
          function pop() {
            var last = root.children.pop()
            seen = last.objectName + " " + root.children.length + " " + (last.parent === null)
          }
        }
        """

    Scenario: Setting the length lets the children after it go
      When the method "cut" of "root" is called
      Then the property "seen" of "root" is "a true true"
      And the property "kids" of "root" is 1

    Scenario: pop takes the last child away
      When the method "pop" of "root" is called
      Then the property "seen" of "root" is "other 2 true"
