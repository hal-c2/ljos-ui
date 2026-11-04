// expect: reject-type 6 TS7006
// Note 11 §3.2 as JavaScript wrote it: parameters without types.
import Ljos.Ui
Item {
    property list<string> tags: []
    function toggled(list, value) {
        return list.includes(value) ? list.filter(each => each !== value) : list.concat([value])
    }
    function toggle(value: string): void { tags = toggled(tags, value) }
}
