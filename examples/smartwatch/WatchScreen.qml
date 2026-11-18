import Ljos.Ui

// What every screen of the watch is given: the colours, the watch, the
// time, and the shape of the glass it is drawn on.
Item {
    required property Theme theme
    required property Watch watch
    required property WallClock clock
    property bool round: true
    // How far in from the edge text keeps.
    property real inset: 46
}
