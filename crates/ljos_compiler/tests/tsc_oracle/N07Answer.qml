// expect: accept
// Note 11 §3.7: a callback that digs into a declared answer.
import Ljos.Ui
Item {
    property string selectedKey: ""
    function added(place: string, result: { readonly item?: { readonly id?: string } } | undefined,
                   error: string | undefined): void {
        const id = result?.item?.id
        if (error === undefined && id !== undefined)
            selectedKey = `${place}|${id}`
    }
}
