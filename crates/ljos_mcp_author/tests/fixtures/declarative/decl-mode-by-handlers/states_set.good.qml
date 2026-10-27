import QtQuick
import QtQuick.Controls

Column {
    Button { text: qsTr("Price"); onClicked: { tab.state = "price"; chart.state = "price" } }
    Button { text: qsTr("Volume"); onClicked: { tab.state = "volume"; chart.state = "volume" } }
    Item { id: tab }
    Item { id: chart }
}
