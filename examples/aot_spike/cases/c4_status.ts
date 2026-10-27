// c4: a branch on a mode (note 11 §3.3), keeping the original's default.
export function statusLabel(status: string): string {
    switch (status) {
    case "todo": return qsTr("To do");
    case "doing": return qsTr("In progress");
    case "blocked": return qsTr("Blocked");
    case "done": return qsTr("Done");
    case "dropped": return qsTr("Dropped");
    default: return status;
    }
}

export function root__statusText(): string {
    return statusLabel(root.status);
}
