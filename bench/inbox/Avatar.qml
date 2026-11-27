import Ljos.Ui
import Ljos.Controls

// A person's picture, round, from the fixture's avatars. A clip cuts to the
// bounding box whatever the radius, so the circle is a mask.
Item {
    id: avatar
    property int size: 32
    // As the record gives it: relative to the data file.
    property string source: ""

    width: avatar.size
    height: avatar.size

    Rectangle {
        anchors.fill: parent
        radius: avatar.size / 2
        fill: Theme.surfaceRaised
    }

    Image {
        id: picture
        anchors.fill: parent
        visible: false
        source: avatar.source === "" ? "" : "data/" + avatar.source
        sourceSize.width: avatar.size
        sourceSize.height: avatar.size
        asynchronous: true
        Accessible.ignored: true
    }

    Rectangle {
        id: circle
        anchors.fill: parent
        visible: false
        layer.enabled: true
        radius: avatar.size / 2
        // Only its coverage is read: the colour never shows.
        fill: Theme.text
    }

    MultiEffect {
        anchors.fill: parent
        source: picture
        maskEnabled: true
        maskSource: circle
        Accessible.ignored: true
    }
}
