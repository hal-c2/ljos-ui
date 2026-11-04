// expect: accept
// Shape: a click in a child flips a flag of the root by its id.
// After corpus/qtdoc/examples/demos/mediaplayer/MediaControls/PlaybackControl.qml:84.
import Ljos.Ui
Item {
    id: root
    property bool shuffled: false
    PointerArea { onClicked: root.shuffled = !root.shuffled }
}
