// expect: reject-subset ts-boundary
// Boundary: with no declarations, TypeScript sees `any`, and the subset refuses each use.
import Ljos.Ui
import "B03Bare.js" as Bare
Item {
    property real total: 0
    function show(): void { total = Bare.add(1, 2) }
}
