// expect: reject-type
// Note 11 §3.7 reading through members that may be missing.
import Ljos.Ui
Item {
    property string selectedKey: ""
    function added(place: string, result: { readonly item?: { readonly id?: string } } | undefined,
                   error: string | undefined): void {
        if (error === undefined)
            selectedKey = place + "|" + result.item.id
    }
}
