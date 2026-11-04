// expect: reject-type 7 TS2322
// A property of a union the module exports, given what is not a member.
import Ljos.Ui
import "M02Union.ts" as Logic
Item {
    property Logic.Tag tag: "a"
    function flip(): void { tag = "c" }
}
