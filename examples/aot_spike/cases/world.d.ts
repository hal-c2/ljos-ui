// The objects of qml/World.qml as the typed subset sees them. A QML id is a
// constant of its object's type; a signal is a method that returns nothing.
// The generator of back end B reads the member types from here.

interface ListModel<Row> {
    readonly count: number;
    get(index: number): Row;
    remove(index: number): void;
    insert(index: number, row: Row): void;
    move(from: number, to: number, count: number): void;
}

declare const root: {
    done: number;
    total: number;
    stopMode: boolean;
    lastMode: string;
    status: string;
    runStatus: string;
    changes: readonly boolean[];
    health: boolean;
    selected: readonly string[];
    ran: number;
    cursor: number;
    depth: number;
    accepted: boolean;
    items: readonly number[];
    syncs: number;
    progress: number;
    statusText: string;
    runText: string;
    dispatched(action: string): void;
};
declare const bar: { width: number };
declare const editor: { submitted(mode: string): void };
declare const field: { text: string };
declare const cards: ListModel<{ toastId: number }>;

declare const Qt: {
    readonly ControlModifier: number;
    readonly Key_1: number;
    readonly Key_9: number;
    readonly Key_Down: number;
    readonly Key_Backspace: number;
};
declare function qsTr(source: string): string;
interface String {
    arg(value: number | string): string;
}
