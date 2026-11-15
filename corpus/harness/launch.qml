import QtQuick
import QtQuick.Window

// Starts one example on qml6 as its own program does:
//   qml6 -I … launch.qml -- <entry> [call…]
// Some examples' C++ main does more than load the file: it calls functions of
// the root object once the object is made (QMetaObject::invokeMethod).
// corpus/examples.json lists them under "launcher": {"calls": […]}, and Ljós makes the same
// calls under `--example`. Without them, qml6 shows another window than the
// example's (FX Material Showroom stays on its loading screen).
// The entry is a URL or an absolute path: QML cannot read the working
// directory, so a relative path has nothing to be resolved against.
// A root that is a window is shown as it is. Any other root goes in a window
// of its own size, as qml6 shows a file. A fault, a call that throws among
// them, exits with 2 and says so on stderr.
QtObject {
    id: launch

    property var args: Qt.application.arguments
    property int dashes: args.indexOf("--")
    property var made: null
    // Held here so the garbage collector keeps a loading component.
    property Component component: null
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
        // A URL starts with its scheme; a path may hold a `:` of its own.
        if (!/^[A-Za-z][A-Za-z0-9+.-]*:/.test(entry)) {
            if (!entry.startsWith("/")) {
                fail("the entry " + entry + " is relative: give an absolute path or a URL")
                return
            }
            // `#` and `?` in a path would start a fragment or a query.
            entry = "file://" + encodeURI(entry).replace(/#/g, "%23").replace(/\?/g, "%3F")
        }
        component = Qt.createComponent(entry)
        // A network entry is still loading when it is made.
        if (component.status === Component.Loading) {
            component.statusChanged.connect(() => {
                if (component.status !== Component.Loading)
                    start(entry)
            })
        } else {
            start(entry)
        }
    }

    function start(entry) {
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
            try {
                made[call]()
            } catch (e) {
                fail(entry + ": " + call + "() threw " + e)
                return
            }
        }
    }
}
