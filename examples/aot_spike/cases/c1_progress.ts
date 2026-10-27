// c1: a binding that reads several properties (note 11, the shape of most
// bindings). `<object>__<property>` is the binding of that property.
export function root__progress(): number {
    return root.total > 0 ? Math.min(1, root.done / root.total) * bar.width : 0;
}
