// expect: accept
// A property of a union the module exports, given one of its members.
import Ljos.Ui
import "M02Union.ts" as Logic
Item {
    property Logic.Tag tag: "a"
    function flip(): void { tag = tag === "a" ? "b" : "a" }
}
