// expect: reject-type
// Note 11 §3.5: a change that sets an optional member to undefined, which
// exactOptionalPropertyTypes forbids.
import Ljos.Ui
Item {
    property bool hideDone: false
    function edited(working: { readonly hideDone: boolean; readonly text: string },
                    changes: { readonly hideDone?: boolean; readonly text?: string }) {
        return { ...working, ...changes }
    }
    function hide(): void { hideDone = edited({ hideDone: hideDone, text: "" }, { hideDone: undefined }).hideDone }
}
