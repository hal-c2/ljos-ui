// expect: reject-type 3 TS2322
// Decoder: in a module too, a member found with `in` is unknown until it is checked.
import Ljos.Ui
import "D12ModuleReject.ts" as Decode
Item {
    property string shown: Decode.title(JSON.parse("{\"title\":\"a\"}"))
}
