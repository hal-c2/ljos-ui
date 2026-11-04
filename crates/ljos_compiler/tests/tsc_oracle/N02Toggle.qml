// expect: accept
// Note 11 §3.2: a value toggled in and out of a list.
import Ljos.Ui
Item {
    property list<string> tags: []
    function toggled(list: readonly string[], value: string): readonly string[] {
        return list.includes(value) ? list.filter(each => each !== value) : [...list, value]
    }
    function toggle(value: string): void { tags = toggled(tags, value) }
}
