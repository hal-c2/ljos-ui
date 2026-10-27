// c6: a list filter (note 11 §3.2).
function toggled(list: readonly string[], value: string): readonly string[] {
    return list.includes(value) ? list.filter(each => each !== value) : [...list, value];
}

export function root__onPicked(value: string): void {
    root.selected = toggled(root.selected, value);
}
