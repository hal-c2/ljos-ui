// expect: reject-type 3 TS2322
// Boundary: a module's call is wrong where its declarations say, at the module's line.
import Ljos.Ui
import "B04ModuleReject.ts" as Steps
Item {
    property string label: Steps.twice(2)
}
