// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Templates as T

// A page that is pushed comes in from the right and pushes the one under
// it out to the left; popping goes the other way. A replace looks like a
// push.
T.StackView {
    id: control

    pushEnter: Transition {
        NumberAnimation {
            property: "x"
            from: (control.mirrored ? -1 : 1) * control.width
            to: 0
            duration: 400
            easing.type: Easing.OutCubic
        }
    }
    pushExit: Transition {
        NumberAnimation {
            property: "x"
            from: 0
            to: (control.mirrored ? -1 : 1) * -control.width
            duration: 400
            easing.type: Easing.OutCubic
        }
    }
    popEnter: Transition {
        NumberAnimation {
            property: "x"
            from: (control.mirrored ? -1 : 1) * -control.width
            to: 0
            duration: 400
            easing.type: Easing.OutCubic
        }
    }
    popExit: Transition {
        NumberAnimation {
            property: "x"
            from: 0
            to: (control.mirrored ? -1 : 1) * control.width
            duration: 400
            easing.type: Easing.OutCubic
        }
    }
    replaceEnter: Transition {
        NumberAnimation {
            property: "x"
            from: (control.mirrored ? -1 : 1) * control.width
            to: 0
            duration: 400
            easing.type: Easing.OutCubic
        }
    }
    replaceExit: Transition {
        NumberAnimation {
            property: "x"
            from: 0
            to: (control.mirrored ? -1 : 1) * -control.width
            duration: 400
            easing.type: Easing.OutCubic
        }
    }
}
