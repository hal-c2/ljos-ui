// Ljos.Controls.Basic's tab (docs/DIALECT.md §4.16): Basic's TabButton with
// Ljos.Controls' `iconName`. The Basic style has no icons of its own, so the
// name is kept, not drawn, and a file moves between the modules by its
// import alone.
import QtQuick
import QtQuick.Controls.Basic as Basic

Basic.TabButton {
    property string iconName: ""
}
