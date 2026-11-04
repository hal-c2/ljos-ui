// expect: reject-subset ts-condition-boolean
// Note 11 §3.1, with a count where the choice needs a boolean.
import Ljos.Ui
Item {
    id: editor
    property int pending: 0
    property string sent: ""
    function interrupt(): void { sent = "interrupt" }
    function submit(mode: string): void { sent = mode }
    PointerArea { onClicked: editor.pending ? editor.interrupt() : editor.submit("foreground") }
}
