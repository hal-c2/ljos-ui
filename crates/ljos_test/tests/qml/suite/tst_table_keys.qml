import QtQuick
import QtQml.Models
import QtTest
import Qt.labs.qmlmodels

// Backtab, and Tab with Shift, in a TableView and a TreeView: keys a
// scenario cannot send apart, since a desktop types both as Shift+Tab.
Item {
    width: 400
    height: 300

    TableView {
        id: table
        width: 200
        height: 140
        selectionModel: ItemSelectionModel {}
        model: TableModel {
            TableModelColumn { display: "a" }
            TableModelColumn { display: "b" }
            TableModelColumn { display: "c" }
            rows: [{ a: 1, b: 2, c: 3 }, { a: 1, b: 2, c: 3 }, { a: 1, b: 2, c: 3 },
                   { a: 1, b: 2, c: 3 }, { a: 1, b: 2, c: 3 }, { a: 1, b: 2, c: 3 }]
        }
        delegate: Item { implicitWidth: 50; implicitHeight: 20 }
    }

    TreeView {
        id: tree
        y: 150
        width: 200
        height: 60
        selectionModel: ItemSelectionModel {}
        model: TreeModel {
            TableModelColumn { display: "a" }
            TableModelColumn { display: "b" }
            rows: [{ a: 1, b: 2, rows: [{ a: 1, b: 2 }] }, { a: 1, b: 2 }, { a: 1, b: 2 }]
        }
        delegate: Item { implicitWidth: 50; implicitHeight: 20 }
    }

    TestCase {
        name: "TableKeys"
        when: windowShown

        function cells(view) {
            var s = [], all = view.selectionModel.selectedIndexes
            for (var i = 0; i < all.length; ++i)
                s.push(all[i].row + ":" + all[i].column)
            return view.currentRow + " " + view.currentColumn + " [" + s.sort().join(",") + "]"
        }

        function press(view, keys) {
            view.selectionModel.clear()
            view.forceActiveFocus()
            wait(20)
            for (var i = 0; i < keys.length; ++i) {
                var k = keys[i]
                if (Array.isArray(k))
                    keyClick(k[0], k[1])
                else
                    keyClick(k)
            }
            return cells(view)
        }

        function test_backtab_data() {
            return [
                { tag: "none current", keys: [Qt.Key_Backtab], out: "0 0 []" },
                { tag: "first cell", keys: [Qt.Key_Down, Qt.Key_Backtab], out: "5 2 []" },
                { tag: "within a row", keys: [Qt.Key_Down, Qt.Key_Right, Qt.Key_Backtab], out: "0 0 []" },
                { tag: "a row's start", keys: [Qt.Key_Down, Qt.Key_Down, Qt.Key_Backtab], out: "0 2 []" },
                { tag: "with Shift", keys: [Qt.Key_Down, Qt.Key_Down, Qt.Key_Right,
                                            [Qt.Key_Backtab, Qt.ShiftModifier]], out: "1 0 []" }
            ]
        }

        function test_backtab(data) {
            compare(press(table, data.keys), data.out)
        }

        function test_shift_tab_data() {
            return [
                { tag: "once", keys: [Qt.Key_Down, [Qt.Key_Tab, Qt.ShiftModifier]], out: "0 1 [0:0,0:1]" },
                { tag: "twice", keys: [Qt.Key_Down, [Qt.Key_Tab, Qt.ShiftModifier],
                                       [Qt.Key_Tab, Qt.ShiftModifier]], out: "0 2 [0:0,0:1,0:2]" },
                { tag: "past a row's end", keys: [Qt.Key_Down, Qt.Key_End,
                                                  [Qt.Key_Tab, Qt.ShiftModifier]], out: "1 0 [0:0,0:1,0:2,1:0,1:1,1:2]" }
            ]
        }

        function test_shift_tab(data) {
            compare(press(table, data.keys), data.out)
        }

        function test_tree_data() {
            return [
                { tag: "Backtab", keys: [Qt.Key_Down, Qt.Key_Down, Qt.Key_Backtab], out: "0 1 []" },
                { tag: "Tab with Shift", keys: [Qt.Key_Down, [Qt.Key_Tab, Qt.ShiftModifier]], out: "0 1 [0:0,0:1]" }
            ]
        }

        function test_tree(data) {
            compare(press(tree, data.keys), data.out)
        }
    }
}
