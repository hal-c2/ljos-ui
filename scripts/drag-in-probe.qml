// What a drag from another application carries and is answered with, for
// checking a platform's drags in by hand (`mise run desktop:drag-in`).
// Each line it logs starts with "drag-in:".
import QtQuick

Item {
    id: root
    width: 640
    height: 360

    function actions(a) {
        const names = []
        if (a & Qt.CopyAction) names.push("copy")
        if (a & Qt.MoveAction) names.push("move")
        if (a & Qt.LinkAction) names.push("link")
        return names.length ? names.join("|") : "none"
    }

    function show(what, area, drag) {
        console.log("drag-in: " + what + " " + area
            + " at=" + Math.round(drag.x) + "," + Math.round(drag.y)
            + " keys=[" + drag.keys + "]"
            + " supported=" + actions(drag.supportedActions)
            + " proposed=" + actions(drag.proposedAction))
    }

    function dropped(area, drop) {
        show("dropped", area, drop)
        console.log("drag-in:   urls=[" + drop.urls + "]")
        console.log("drag-in:   text=" + JSON.stringify(drop.hasText ? drop.text : null))
        console.log("drag-in:   html=" + JSON.stringify(drop.hasHtml ? drop.html : null))
        for (const f of drop.formats)
            console.log("drag-in:   " + f + ": " + JSON.stringify(String(drop.getDataAsString(f)).slice(0, 200)))
    }

    // Takes anything, with the action the source proposes.
    DropArea {
        id: any
        width: parent.width / 2
        height: parent.height
        onEntered: (drag) => root.show("entered", "any", drag)
        onExited: console.log("drag-in: exited any")
        onDropped: (drop) => {
            root.dropped("any", drop)
            drop.accept()
            console.log("drag-in:   answered " + root.actions(drop.action))
        }
        Rectangle {
            anchors.fill: parent
            anchors.margins: 8
            color: any.containsDrag ? palette.highlight : palette.base
            Text {
                anchors.centerIn: parent
                color: palette.text
                text: "anything: proposed action"
            }
        }
    }

    // Takes files and links alone, as a link: text is refused here.
    DropArea {
        id: links
        x: parent.width / 2
        width: parent.width / 2
        height: parent.height
        keys: ["text/uri-list"]
        onEntered: (drag) => {
            root.show("entered", "links", drag)
            drag.accept(Qt.LinkAction)
        }
        onExited: console.log("drag-in: exited links")
        onDropped: (drop) => {
            root.dropped("links", drop)
            drop.accept(Qt.LinkAction)
            console.log("drag-in:   answered " + root.actions(drop.action))
        }
        Rectangle {
            anchors.fill: parent
            anchors.margins: 8
            color: links.containsDrag ? palette.highlight : palette.base
            Text {
                anchors.centerIn: parent
                color: palette.text
                text: "text/uri-list only: link"
            }
        }
    }
}
