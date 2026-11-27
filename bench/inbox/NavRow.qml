import Ljos.Ui
import Ljos.Controls

// One row of the sidebar: a mark, a name, raised while it is the filter.
Rectangle {
    id: row
    property string iconName: ""
    property string label: ""
    property bool active: false
    signal picked()

    radius: Theme.radiusMedium
    fill: row.active ? Theme.accentSurface : hover.hovered ? Theme.surfaceRaised : "transparent"
    Accessible.role: Accessible.Button
    Accessible.name: row.label
    Accessible.checked: row.active

    Icon {
        x: 12
        anchors.verticalCenter: parent.verticalCenter
        name: row.iconName
        color: row.active ? Theme.text : Theme.iconMuted
    }

    Text {
        x: 40
        width: row.width - 48
        height: row.height
        verticalAlignment: Text.AlignVCenter
        text: row.label
        color: Theme.text
        font.size: 13
        font.weight: row.active ? Font.DemiBold : Font.Normal
        elide: Text.ElideRight
    }

    Hover {
        id: hover
    }

    Tap {
        onTapped: row.picked()
    }
}
