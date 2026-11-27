import Ljos.Ui
import Ljos.Controls

// A label on a record.
Rectangle {
    id: chip
    property string text: ""

    width: label.implicitWidth + 12
    height: 18
    radius: 9
    fill: Theme.surfaceRaised
    border.color: Theme.border
    border.width: 1

    Text {
        id: label
        anchors.centerIn: parent
        text: chip.text
        color: Theme.textMuted
        font.size: 11
    }
}
