// expect: layout-overlap at 9
import QtQuick
import QtQuick.Controls

Item {
    width: 400; height: 200
    // Each label is placed by a fixed position that ignores the other's width.
    Label { x: 20; y: 20; text: "Destination folder" }
    Label { x: 60; y: 22; text: "Downloads" }
}
