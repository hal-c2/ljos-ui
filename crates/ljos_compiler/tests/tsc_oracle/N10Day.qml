// expect: accept
// Note 11 §3.10: parsing with a sentinel, defaults where an index may miss.
import Ljos.Ui
Item {
    function day(iso: string): real {
        if (iso === "")
            return NaN
        const [y = "", m = "", d = ""] = iso.split("-")
        return Number(y) * 372 + (Number(m) - 1) * 31 + Number(d)
    }
}
