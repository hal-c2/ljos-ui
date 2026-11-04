// expect: reject-subset ts-condition-boolean
// Shape: a click flips a flag, written with `!` on a count.
// After corpus/qtdoc/examples/demos/lightningviewer/LightningViewerModule/SwitchMap.qml:70.
import Ljos.Ui
PointerArea {
    property bool opened: false
    property int clicks: 0
    onClicked: opened = !clicks
}
