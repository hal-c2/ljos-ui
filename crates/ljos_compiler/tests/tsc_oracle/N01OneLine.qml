// expect: accept
// Note 11 §3.1: a one-line handler that chooses between two calls.
import Ljos.Ui
Item {
    id: editor
    property bool stopMode: false
    property string sent: ""
    function interrupt(): void { sent = "interrupt" }
    function submit(mode: string): void { sent = mode }
    PointerArea { onClicked: editor.stopMode ? editor.interrupt() : editor.submit("foreground") }
}
