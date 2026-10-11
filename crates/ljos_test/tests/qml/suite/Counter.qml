import QtQuick

// Found by the directory import: it sits next to the tests that use it.
Rectangle {
    id: counter
    property int count: 0
    signal bumped(int to, string why)
    function bump(why) {
        count += 1
        bumped(count, why)
    }
    width: 40
    height: 40
    color: "steelblue"
}
