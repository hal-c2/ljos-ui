// expect: accept
// Note 11 §3.5: a record updated by spreading the changes into a copy.
import Ljos.Ui
Item {
    property bool hideDone: false
    function edited(working: { readonly hideDone: boolean; readonly text: string },
                    changes: { readonly hideDone?: boolean; readonly text?: string }) {
        return { ...working, ...changes }
    }
    function hide(): void { hideDone = edited({ hideDone: hideDone, text: "" }, { hideDone: true }).hideDone }
}
