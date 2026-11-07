// expect: reject-subset ts-boundary
// Boundary: a member read by its name in brackets from a file with no declarations is `any` to TypeScript, and the subset refuses it.
import Ljos.Ui
import "B03Bare.js" as Bare
Item {
    property real total: 0
    function show(): void { total = Bare["add"](1, 2) }
}
