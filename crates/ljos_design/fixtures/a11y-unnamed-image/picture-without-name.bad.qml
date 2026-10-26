// expect: a11y-unnamed-image at 7
import QtQuick
import QtQuick.Controls
Rectangle {
    width: 640; height: 480
    color: "white"
    Image {
        width: 64; height: 64
        source: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABAAAAAQCAIAAACQkWg2AAAAIUlEQVR42mN4n8OAFf3/8RArYhjVQBMNuCRwGTSqgSYaAMXJGZ+uT/rTAAAAAElFTkSuQmCC"
    }
}
