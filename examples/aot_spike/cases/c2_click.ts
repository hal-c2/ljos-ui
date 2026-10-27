// c2: a one-line handler (note 11 §3.1). The two calls of the original are
// signals here. `<object>__on<Signal>` is a handler.
export function root__onClicked(): void {
    root.stopMode ? root.dispatched("editor.interrupt") : editor.submitted("foreground");
}
