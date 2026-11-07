// expect: reject-type 7 TS2322
// Shape: a file that imports Ljós's API under an alias is checked by
// TypeScript as one that imports it plainly.
import Ljos.Ui as U
U.Item {
    property real level: 0
    function reset(): void { level = "0" }
}
