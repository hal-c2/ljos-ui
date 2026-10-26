// expect: focus-order at 12
import QtQuick
import QtQuick.Controls

Rectangle {
    width: 400; height: 300
    color: palette.window
    Column {
        x: 20; y: 20
        spacing: 8
        TextField { id: name; placeholderText: "Name"; KeyNavigation.tab: city }
        TextField { id: street; placeholderText: "Street"; KeyNavigation.tab: done }
        // Tab goes from Name to City and back up to Street.
        TextField { id: city; placeholderText: "City"; KeyNavigation.tab: street }
        Button { id: done; text: "Done"; KeyNavigation.tab: name }
    }
}
