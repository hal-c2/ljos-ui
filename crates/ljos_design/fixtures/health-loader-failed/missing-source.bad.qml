// expect: health-loader-failed at 6
import QtQuick

Item {
    width: 200; height: 100
    Loader {
        anchors.fill: parent
        source: "RowThatIsNotThere.qml"
    }
}
