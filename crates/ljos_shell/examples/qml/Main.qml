import QtQuick
import QtQuick.Controls.Basic

Item {
    id: root

    property int clicks: 0
    property string size: "Medium"

    // A 24 by 24 plus in a ring, black on nothing: the style gives it the
    // colour of the text beside it.
    readonly property url plus: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABgAAAAYCAYAAADgdz34AAAAxUlEQVR42sVVwREDIQi0hCvBElKCpVgCJdiRJVwJlkIJCQ/M7DhGTXJ4zOwHF5YBRedutkNAgixgwVPB6iPl/JQ4NUk/gZW7LPQQFEhwCqLAA8er7wRe0dhpcoaAsFBQgIJ4JHIAsXzZ26XYNCEEaEeYiKTeIQ+CVwSQw22RBAN1fwg4GDyhM6szXiAQlZPRWdvjm4QIAgHqnOMVrm16Ww3s+VYxzGcuYN4i8yGbX1Pzh2a+KrYsO/N1veXD2fJlbvn0L7MXGiy0HcjQl+cAAAAASUVORK5CYII="

    // The window colour of the palette, light or dark with the system.
    Pane { anchors.fill: parent }

    MenuBar {
        id: bar
        objectName: "bar"
        width: parent.width

        Menu {
            title: "File"
            MenuItem { text: "New"; icon.source: root.plus }
            MenuItem { text: "Open a dialog"; onTriggered: dialog.open() }
        }
        // More lines than the window is tall: the menu ends at the window
        // and its lines scroll. A separator halves them so the eye finds
        // its place.
        Menu {
            title: "Go"
            Repeater {
                model: 8
                MenuItem { text: "Line " + (index + 1) }
            }
            MenuSeparator {}
            Repeater {
                model: 8
                MenuItem { text: "Line " + (index + 9) }
            }
        }
        Menu {
            title: "Help"
            MenuItem { text: "About"; onTriggered: notice.open() }
        }
    }

    Column {
        x: 24
        y: 52
        spacing: 8

        Label {
            text: "Ljós on egui, from QML"
            font.pixelSize: 24
        }

        Row {
            spacing: 8
            Rectangle { width: 48; height: 48; color: "#2e7d32" }
            Rectangle { width: 48; height: 48; radius: 8; color: "#c62828" }
            Rectangle { width: 48; height: 48; radius: 16; color: "#1565c0" }
            Rectangle { width: 48; height: 48; radius: 24; color: "#ef6c00" }
        }

        // A 16 by 16 picture carried in its URL, decoded off the UI thread
        // and tiled.
        Image {
            width: 216
            height: 48
            fillMode: Image.Tile
            smooth: false
            // Decoration: it says nothing a listener would miss.
            Accessible.ignored: true
            source: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABAAAAAQCAIAAACQkWg2AAAAIUlEQVR42mN4n8OAFf3/8RArYhjVQBMNuCRwGTSqgSYaAMXJGZ+uT/rTAAAAAElFTkSuQmCC"
        }

        // More rows than fit: the wheel and the bar move them.
        ScrollView {
            objectName: "rows"
            width: 180
            height: 60

            Column {
                Repeater {
                    model: 12
                    Label { text: "Row " + (index + 1) }
                }
            }
        }
    }

    Rectangle {
        anchors.centerIn: parent
        width: 220
        height: 90
        radius: 10
        color: "#fff8e1"
        border.width: 2
        border.color: "#ef6c00"

        Text {
            anchors.centerIn: parent
            text: "anchors.centerIn: parent"
        }
    }

    // Controls as the Basic style draws them.
    Column {
        anchors.left: parent.left
        anchors.bottom: parent.bottom
        anchors.margins: 24
        spacing: 8

        Row {
            spacing: 8
            TextField { objectName: "name"; width: 160; placeholderText: "Type here"; Accessible.name: "Name" }
            SpinBox { value: 3; to: 10; Accessible.name: "Count" }
        }

        Row {
            spacing: 8
            Button { text: "Button"; icon.source: root.plus }
            Button { text: "Checked"; checkable: true; checked: true }
            ToolButton { text: "Tool" }
            ToolButton {
                text: "Add"
                icon.source: root.plus
                display: AbstractButton.IconOnly
                ToolTip.text: text
                ToolTip.visible: hovered
            }
        }

        Row {
            spacing: 8
            CheckBox { text: "Check box"; checked: true }
            RadioButton { text: "One"; checked: true }
            RadioButton { text: "Two" }
            Switch { text: "Switch"; checked: true }
        }
    }

    // What opens over the window: the rows of a combo box, a menu, a dialog
    // and a popup that fades in and out.
    Column {
        anchors.right: parent.right
        anchors.top: bar.bottom
        anchors.margins: 24
        anchors.topMargin: 12
        spacing: 8

        ComboBox {
            objectName: "size"
            model: ["Small", "Medium", "Large"]
            currentIndex: 1
            onActivated: root.size = currentText
        }

        Button {
            objectName: "menuButton"
            text: "Menu"
            onClicked: menu.open()

            Menu {
                id: menu
                objectName: "menu"
                y: parent.height

                MenuItem { text: "Open a dialog"; onTriggered: dialog.open() }
                MenuItem { text: "Remember"; checkable: true; checked: true }
                MenuSeparator {}
                Menu {
                    title: "More"
                    MenuItem { text: "Nothing here" }
                }
            }
        }

        Button {
            objectName: "dialogButton"
            text: "Dialog"
            onClicked: dialog.open()
        }

        // Typing completes to a row; Return takes what is typed.
        ComboBox {
            objectName: "fruit"
            editable: true
            model: ["Apple", "Banana", "Cherry"]
            Accessible.name: "Fruit"
        }

        Button {
            objectName: "noticeButton"
            text: "Notice"
            onClicked: notice.open()
        }
    }

    Popup {
        id: notice
        objectName: "notice"
        x: 24
        y: 266

        enter: Transition {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 150 }
        }
        exit: Transition {
            NumberAnimation { property: "opacity"; from: 1; to: 0; duration: 150 }
        }

        Label { text: "Comes and goes in 150 ms" }
    }

    Dialog {
        id: dialog
        objectName: "dialog"
        anchors.centerIn: parent
        title: "A dialog"
        modal: true
        standardButtons: Dialog.Ok | Dialog.Cancel

        Label { text: "Size: " + root.size }
    }

    // A button whose parts the application draws itself.
    Button {
        id: button
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 24
        text: root.clicks > 0 ? "Clicked " + root.clicks + "×" : "Click me"
        opacity: hovered ? 0.85 : 1.0
        onClicked: root.clicks++

        background: Rectangle {
            implicitWidth: 160
            implicitHeight: 56
            radius: 6
            color: root.clicks % 2 ? "#c62828" : "#1565c0"
            // The fill is the application's own, so it draws keyboard
            // focus itself.
            border.width: button.visualFocus ? 3 : 0
            border.color: "#ffb300"
        }

        contentItem: Text {
            text: button.text
            color: "white"
            font.pixelSize: 16
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
    }
}
