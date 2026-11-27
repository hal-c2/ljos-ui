import Ljos.Ui
import Ljos.Core
import Ljos.Controls

// Language, look and motion. Each choice takes effect as it is made, so the
// dialog has no Apply: Close (or Escape) only hides it.
Dialog {
    id: dialog
    required property Store inbox

    x: 400
    y: 220
    width: 480
    height: 360
    modal: true
    padding: 0
    topPadding: 0
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

    Item {
        width: 480
        height: 360

        Text {
            x: 24
            y: 20
            text: t`Settings`
            color: Theme.text
            font.size: 16
            font.weight: Font.DemiBold
        }

        Text {
            x: 24
            y: 64
            text: t`Language`
            color: Theme.textMuted
            font.size: 12
            font.weight: Font.DemiBold
        }

        ButtonGroup { id: languages }

        RadioButton {
            objectName: "english"
            x: 20
            y: 84
            width: 100
            height: 24
            text: "English"
            ButtonGroup.group: languages
            checked: App.language !== "ja"
            onClicked: App.language = "en"
        }

        RadioButton {
            objectName: "japanese"
            x: 120
            y: 84
            width: 100
            height: 24
            text: "日本語"
            ButtonGroup.group: languages
            checked: App.language === "ja"
            onClicked: App.language = "ja"
        }

        Text {
            x: 24
            y: 136
            text: t`Theme`
            color: Theme.textMuted
            font.size: 12
            font.weight: Font.DemiBold
        }

        ButtonGroup { id: looks }

        RadioButton {
            x: 20
            y: 156
            width: 100
            height: 24
            text: t`Light`
            ButtonGroup.group: looks
            checked: dialog.inbox.look === "light"
            onClicked: dialog.inbox.look = "light"
        }

        RadioButton {
            x: 120
            y: 156
            width: 100
            height: 24
            text: t`Dark`
            ButtonGroup.group: looks
            checked: dialog.inbox.look === "dark"
            onClicked: dialog.inbox.look = "dark"
        }

        RadioButton {
            x: 220
            y: 156
            width: 120
            height: 24
            text: t`System`
            ButtonGroup.group: looks
            checked: dialog.inbox.look === "system"
            onClicked: dialog.inbox.look = "system"
        }

        Text {
            x: 24
            y: 208
            text: t`Motion`
            color: Theme.textMuted
            font.size: 12
            font.weight: Font.DemiBold
        }

        CheckBox {
            objectName: "reduce-motion"
            x: 20
            y: 228
            width: 200
            height: 24
            text: t`Reduce motion`
            checked: dialog.inbox.reduced
            onToggled: dialog.inbox.reduced = checked
        }

        Button {
            objectName: "close-settings"
            x: 370
            y: 312
            width: 100
            height: 32
            text: t`Close`
            onClicked: dialog.close()
        }
    }
}
