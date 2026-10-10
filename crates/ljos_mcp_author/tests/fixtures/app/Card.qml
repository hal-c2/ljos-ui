import QtQuick

Rectangle {
    id: card
    property string title: ""
    property alias label: caption.text
    signal opened(string reason)
    default property alias content: inner.data

    Text {
        id: caption
        text: card.title
        Accessible.name: text
    }
    Item {
        id: inner
    }
}
