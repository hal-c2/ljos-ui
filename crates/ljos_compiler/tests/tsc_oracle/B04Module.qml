// expect: accept
// Boundary: a TypeScript module calls a JavaScript file by its declarations.
import Ljos.Ui
import "B04Module.ts" as Steps
Item {
    property real total: Steps.twice(2)
}
