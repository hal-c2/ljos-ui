// expect: accept
// Shape: a repeating timer counts its steps and stops itself.
// After corpus/qtdoc/examples/demos/android/splashscreen/quick/Splash.qml:46.
import Ljos.Ui
Item {
    property real progress: 0
    Timer {
        id: ticker
        interval: 50
        repeat: true
        running: true
        onTriggered: {
            progress = Math.min(1, progress + 0.05)
            if (progress >= 1)
                ticker.stop()
        }
    }
}
