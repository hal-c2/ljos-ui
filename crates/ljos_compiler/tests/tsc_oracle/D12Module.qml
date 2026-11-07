// expect: accept
// Decoder: a module narrows what it is given the way a script does.
import Ljos.Ui
import "D12Module.ts" as Decode
Item {
    property string shown: Decode.title(JSON.parse("{\"title\":\"a\"}"))
    property int count: Decode.titles(JSON.parse("[]")).length
}
