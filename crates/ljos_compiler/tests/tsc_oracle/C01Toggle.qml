// expect: accept
// Shape: a click flips a flag of its own item.
// After corpus/qtdoc/examples/demos/lightningviewer/LightningViewerModule/SwitchMap.qml:70.
import Ljos.Ui
PointerArea {
    property bool opened: false
    onClicked: opened = !opened
}
