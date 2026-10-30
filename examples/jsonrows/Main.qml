import QtQuick
import Ljos.Models

Rectangle {
    width: 320
    height: 240
    color: "#f4f4f0"

    JsonListModel {
        id: stock
        objectName: "stock"
        source: "rows.json"
    }

    ListView {
        objectName: "list"
        anchors.fill: parent
        anchors.margins: 8
        spacing: 4
        model: stock
        delegate: Text {
            required property string name
            // Not every row has a price: a key a row lacks reads undefined.
            required property var price
            required property bool stocked
            text: name + (price === undefined ? "" : "  " + price.toFixed(2))
            color: stocked ? "#202020" : "#a0a0a0"
            font.pixelSize: 18
        }
    }

    Text {
        anchors.bottom: parent.bottom
        anchors.margins: 8
        x: 8
        width: parent.width - 16
        wrapMode: Text.Wrap
        font.pixelSize: 14
        color: stock.status === JsonListModel.Error ? "#b00020" : "#606060"
        text: stock.status === JsonListModel.Error ? stock.error : stock.count + " rows"
    }
}
