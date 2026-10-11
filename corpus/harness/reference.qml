import QtQuick
import QtQuick.Window

// Takes one picture of one example, for the gallery (mise run gallery:reference).
// It is arranged by command line arguments after the file name:
//   <entry url> <width> <height> <settle ms> <png path>
// An example whose root is a window is shown as it is; any other root is
// put in a white window of the stated size. After the settle time the
// window's content is saved and the program ends. A fault exits with 2 and
// says so on stderr.
Window {
    id: host
    property var args: Qt.application.arguments
    property string entry: args[args.length - 5]
    property int wide: parseInt(args[args.length - 4])
    property int high: parseInt(args[args.length - 3])
    property int settle: parseInt(args[args.length - 2])
    property string target: args[args.length - 1]
    property var shown: null
    property Item grabbed: null

    width: wide
    height: high
    visible: true
    color: "white"

    // The picture is taken of the content item, which is drawn over the
    // window's colour and does not include it.
    Rectangle {
        anchors.fill: parent
        color: host.color
        z: -1
    }

    function fail(text) {
        console.error(text)
        Qt.exit(2)
    }

    Component.onCompleted: {
        var component = Qt.createComponent(entry)
        if (component.status !== Component.Ready) {
            fail(component.errorString())
            return
        }
        var made = component.createObject(null)
        if (!made) {
            fail("cannot create " + entry + ": " + component.errorString())
            return
        }
        shown = made
        if (typeof made.contentItem !== "undefined" && typeof made.requestActivate === "function") {
            // A window: ApplicationWindow's content item sits below the
            // window's own root, so walk up to it.
            made.visible = true
            var top = made.contentItem
            while (top.parent) top = top.parent
            grabbed = top
            host.visible = false
        } else {
            made.parent = host.contentItem
            grabbed = host.contentItem
        }
        wait.interval = settle
        wait.start()
    }

    Timer {
        id: wait
        onTriggered: {
            var ok = host.grabbed.grabToImage(function (result) {
                if (!result.saveToFile(host.target)) host.fail("cannot write " + host.target)
                else {
                    console.log("size " + result.image.width + "x" + result.image.height)
                    Qt.quit()
                }
            })
            if (!ok) host.fail("the picture could not be taken")
        }
    }
}
