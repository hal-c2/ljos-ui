import QtQuick

Item {
    QtObject {
        id: keys
        property bool forward: false
        property bool back: false
    }
    function forwardPressed() { keys.forward = true; keys.back = false }
    function backPressed() { keys.back = true; keys.forward = false }
}
