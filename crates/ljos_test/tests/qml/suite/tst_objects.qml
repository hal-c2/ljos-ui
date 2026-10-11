import QtQuick
import QtTest
import Things

Item {
    id: root
    width: 200
    height: 200

    Component {
        id: counterComponent
        Counter {}
    }

    Component {
        id: doublerComponent
        Doubler {}
    }

    Item {
        id: tree
        objectName: "tree"
        Item {
            objectName: "branch"
            Rectangle {
                objectName: "leaf"
                width: 5
                height: 5
            }
        }
    }

    property var kept: null

    TestCase {
        name: "Objects"
        when: windowShown

        function test_1_createTemporaryObject() {
            var counter = createTemporaryObject(counterComponent, root, { count: 4, objectName: "made" })
            verify(counter)
            compare(counter.count, 4)
            compare(counter.parent, root)
            counter.bump("test")
            compare(counter.count, 5)
            compare(findChild(root, "made"), counter)
            root.kept = counter
        }

        function test_2_temporary_objects_are_destroyed() {
            compare(findChild(root, "made"), null)
            verify(!root.kept)
        }

        function test_createTemporaryQmlObject() {
            var made = createTemporaryQmlObject(
                "import QtQuick; Rectangle { objectName: \"inline\"; width: 12; height: width * 2 }", root)
            verify(made)
            compare(made.width, 12)
            compare(made.height, 24)
            compare(made.parent, root)
            compare(findChild(root, "inline"), made)
        }

        function test_findChild() {
            var leaf = findChild(tree, "leaf")
            verify(leaf)
            compare(leaf.width, 5)
            compare(findChild(root, "branch").parent, tree)
            compare(findChild(tree, "nothing"), null)
        }

        function test_directory_import() {
            var counter = createTemporaryObject(counterComponent, root)
            compare(counter.width, 40)
            compare(counter.count, 0)
        }

        function test_import_path_module() {
            var doubler = createTemporaryObject(doublerComponent, root, { input: 21 })
            compare(doubler.output, 42)
        }
    }
}
