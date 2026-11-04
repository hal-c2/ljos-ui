// expect: reject-type
// Shape: a timer hides what it showed, and gives the flag a string.
// After corpus/qtdoc/examples/demos/android/feature-delivery/fdmaploader/Main.qml:113.
import Ljos.Ui
Item {
    Rectangle { id: toast; visible: true }
    Timer { interval: 3000; running: toast.visible; onTriggered: toast.visible = "false" }
}
