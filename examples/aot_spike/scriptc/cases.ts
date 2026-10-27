// Back end A: the eight cases of ../cases, lowered by hand to what
// scriptc's library mode takes. This is the glue an AOT step would
// generate; what it had to change is the list of what A can't compile.
//
// * An object's property is a numbered slot the host resolves (a.rs).
//   Numbers and booleans are read and written through callbacks, so a
//   read in a binding is tracked by the engine.
// * A callback cannot return a string, so a string the case reads comes
//   in as a parameter the host read for it, and a list of strings as one
//   string joined with U+001F.
// * qsTr is the identity here: it cannot reach the catalogue.
// * Emits go out through a callback; the host delivers them after the
//   export returns, because a handler that is itself compiled here would
//   enter the library again, which scriptc rejects.

declare function readNum(slot: number): number;
declare function readBool(slot: number): boolean;
declare function writeNum(slot: number, v: number): void;
declare function writeBool(slot: number, v: boolean): void;
declare function writeStr(slot: number, v: string): void;
declare function writeStrList(slot: number, joined: string): void;
declare function emitStr(signal: number, v: string): void;
declare function listLen(slot: number): number;
declare function listNum(slot: number, i: number): number;
declare function listBool(slot: number, i: number): boolean;
declare function modelCount(): number;
declare function modelRowNum(i: number): number;
declare function modelRemove(i: number): void;
declare function modelInsertNum(i: number, toastId: number): void;
declare function modelMove(from: number, to: number, n: number): void;

// The slots, in the order of a.rs PROPS and SIGNALS.
const TOTAL = 0;
const DONE = 1;
const BAR_WIDTH = 2;
const STOP_MODE = 3;
const LAST_MODE = 4;
const CHANGES = 5;
const HEALTH = 6;
const SELECTED = 7;
const RAN = 8;
const CURSOR = 9;
const DEPTH = 10;
const ACCEPTED = 11;
const ITEMS = 12;
const SYNCS = 13;
const DISPATCHED = 0;
const SUBMITTED = 1;

const CONTROL_MODIFIER = 0x04000000;
const KEY_1 = 0x31;
const KEY_9 = 0x39;
const KEY_DOWN = 0x01000015;
const KEY_BACKSPACE = 0x01000003;
const SEP = "\u001f";

function qsTr(source: string): string {
    return source;
}

// c1
export function progress(): number {
    return readNum(TOTAL) > 0 ? Math.min(1, readNum(DONE) / readNum(TOTAL)) * readNum(BAR_WIDTH) : 0;
}

// c2
export function onClicked(): number {
    readBool(STOP_MODE) ? emitStr(DISPATCHED, "editor.interrupt") : emitStr(SUBMITTED, "foreground");
    return 0;
}

// c3
export function onSubmitted(mode: string): number {
    writeStr(LAST_MODE, mode);
    return 0;
}

// c4
function statusLabel(status: string): string {
    switch (status) {
    case "todo": return qsTr("To do");
    case "doing": return qsTr("In progress");
    case "blocked": return qsTr("Blocked");
    case "done": return qsTr("Done");
    case "dropped": return qsTr("Dropped");
    default: return status;
    }
}

export function statusText(status: string): string {
    return statusLabel(status);
}

// c5
export function runText(status: string): string {
    if (status !== "proposed")
        return statusLabel(status);
    const changes: boolean[] = [];
    const n = listLen(CHANGES);
    for (let i = 0; i < n; ++i)
        changes.push(listBool(CHANGES, i));
    const kept = changes.filter(dismissed => !dismissed).length;
    if (kept === 0 && readBool(HEALTH))
        return qsTr("Status update");
    return kept === 1 ? qsTr("1 change") : qsTr("%1 changes").replace("%1", String(kept));
}

// c6
function toggled(list: string[], value: string): string[] {
    return list.includes(value) ? list.filter(each => each !== value) : [...list, value];
}

export function onPicked(selected: string, value: string): number {
    const list: string[] = selected === "" ? [] : selected.split(SEP);
    writeStrList(SELECTED, toggled(list, value).join(SEP));
    return 0;
}

// c7
function leaveSubmenu(): boolean {
    if (readNum(DEPTH) === 0)
        return false;
    writeNum(DEPTH, readNum(DEPTH) - 1);
    return true;
}

export function onKeyPressed(key: number, modifiers: number, fieldText: string): number {
    const ctrl = (modifiers & CONTROL_MODIFIER) !== 0;
    if (ctrl && key >= KEY_1 && key <= KEY_9) {
        writeNum(RAN, key - KEY_1);
    } else if (key === KEY_DOWN) {
        writeNum(CURSOR, readNum(CURSOR) + 1);
    } else if (key === KEY_BACKSPACE && fieldText.length === 0) {
        if (!leaveSubmenu())
            return 0;
    } else {
        return 0;
    }
    writeBool(ACCEPTED, true);
    return 0;
}

// c8
export function onItemsChanged(): number {
    const items: number[] = [];
    const n = listLen(ITEMS);
    for (let i = 0; i < n; ++i)
        items.push(listNum(ITEMS, i));
    const shown = new Set(items);
    for (let i = modelCount() - 1; i >= 0; --i) {
        if (shown.has(modelRowNum(i)))
            continue;
        modelRemove(i);
    }
    for (let i = 0; i < items.length; ++i) {
        let at = -1;
        for (let j = i; j < modelCount(); ++j) {
            if (modelRowNum(j) === items[i]) {
                at = j;
                break;
            }
        }
        if (at < 0)
            modelInsertNum(i, items[i]);
        else if (at !== i)
            modelMove(at, i, 1);
    }
    writeNum(SYNCS, readNum(SYNCS) + 1);
    return 0;
}

// The boundary: n tracked reads, n writes, n emits in one call.
export function readMany(n: number): number {
    let s = 0;
    for (let i = 0; i < n; ++i)
        s += readNum(DONE);
    return s;
}

export function writeMany(n: number): number {
    for (let i = 0; i < n; ++i)
        writeNum(CURSOR, i);
    return n;
}

export function emitMany(n: number): number {
    for (let i = 0; i < n; ++i)
        emitStr(DISPATCHED, "x");
    return n;
}

// No work: what entering the library costs.
export function nothing(): number {
    return 0;
}
