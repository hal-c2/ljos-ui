import QtQuick
import QtQuick.Window

// Starts one example on qml6 as its own program does:
//   qml6 -I … launch.qml -- <entry> [call…]
// Some examples' C++ main does more than load the file: it calls functions of
// the root object once the object is made (QMetaObject::invokeMethod).
// corpus/examples.json lists them under "launcher": {"calls": […]}, and Ljós makes the same
// calls under `--example`. Without them, qml6 shows another window than the
// example's (FX Material Showroom stays on its loading screen).
// A root that is a window is shown as it is. Any other root goes in a window
// of its own size, as qml6 shows a file. A fault exits with 2 and says so on
// stderr.
QtObject {
    id: launch

    property var args: Qt.application.arguments
    property int dashes: args.indexOf("--")
    property var made: null
    property Window host: null

    function fail(text) {
        console.error("launch.qml: " + text)
        Qt.exit(2)
    }

    Component.onCompleted: {
        if (dashes < 0 || dashes + 1 >= args.length) {
            fail("give the entry after --: launch.qml -- <entry> [call…]")
            return
        }
        let entry = args[dashes + 1]
        if (entry.indexOf(":") < 0)
            entry = "file://" + entry
        const component = Qt.createComponent(entry)
        if (component.status !== Component.Ready) {
            fail(component.errorString())
            return
        }
        made = component.createObject(null)
        if (made === null) {
            fail("cannot create " + entry + ": " + component.errorString())
            return
        }
        if (typeof made.contentItem === "undefined" || typeof made.requestActivate !== "function") {
            host = Qt.createQmlObject("import QtQuick; import QtQuick.Window; Window { visible: true }", launch)
            host.width = made.width > 0 ? made.width : 640
            host.height = made.height > 0 ? made.height : 480
            made.parent = host.contentItem
        }
        for (const call of args.slice(dashes + 2)) {
            if (typeof made[call] !== "function") {
                fail(entry + " has no function " + call)
                return
            }
            made[call]()
        }
    }
}
