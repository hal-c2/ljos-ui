import Ljos.Ui
import Ljos.Core
import Ljos.Layout
import "logic/car.ts" as Car

// Every page of the touchscreen, as large tiles.
Item {
    id: panel

    required property Theme theme
    required property Cockpit car

    Grid {
        anchors.centerIn: parent
        columns: 3
        columnSpacing: 24
        rowSpacing: 24
        Repeater {
            model: Car.apps.filter((app: Car.App) => app.page !== "apps")
            delegate: Item {
                id: tile
                required property Car.App modelData
                width: 280
                height: 150
                Chip {
                    anchors.fill: parent
                    theme: panel.theme
                    help: panel.car.pageTitle(tile.modelData.page)
                    active: panel.car.page === tile.modelData.page
                    onPressed: panel.car.page = tile.modelData.page
                }
                Icon {
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: 30
                    width: 48
                    height: 48
                    glyph: tile.modelData.glyph
                    color: panel.theme.cyan
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: 96
                    text: panel.car.pageTitle(tile.modelData.page)
                    color: panel.theme.text
                    font.size: 19
                }
            }
        }
    }
}
