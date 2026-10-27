import { statusLabel } from "./c4_status";

// c5: string formatting with a plural (note 11 §3.4). The run is three
// properties; a change is its `dismissed` flag.
function runLabel(status: string, changes: readonly boolean[], health: boolean): string {
    if (status !== "proposed")
        return statusLabel(status);
    const kept = changes.filter(dismissed => !dismissed).length;
    if (kept === 0 && health)
        return qsTr("Status update");
    return kept === 1 ? qsTr("1 change") : qsTr("%1 changes").arg(kept);
}

export function root__runText(): string {
    return runLabel(root.runStatus, root.changes, root.health);
}
