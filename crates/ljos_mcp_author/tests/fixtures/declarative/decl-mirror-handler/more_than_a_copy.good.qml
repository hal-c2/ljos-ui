import QtQuick

Item {
    property real level: 0
    property real shown: 0
    onLevelChanged: shown = level + Date.now()
}
