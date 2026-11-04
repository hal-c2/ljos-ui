// expect: reject-type
// Note 11 §3.10 indexing as if every part were there.
import Ljos.Ui
Item {
    function day(iso: string): real {
        if (iso === "")
            return NaN
        const parts = iso.split("-")
        return parts[0].length + parts[1].length
    }
}
