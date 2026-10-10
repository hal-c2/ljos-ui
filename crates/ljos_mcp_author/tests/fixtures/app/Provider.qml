import QtQuick

Item {
    // `gadgetScale` is used by Gizmo without being declared there: whoever makes a
    // Gizmo in a component that has `gadgetScale` provides it.
    property int gadgetScale: 4
    Gizmo {}
}
