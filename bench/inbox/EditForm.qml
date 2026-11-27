import Ljos.Ui
import Ljos.Core
import Ljos.Controls
import "logic/inbox.ts" as Inbox

// The edit form. It starts from the selected record each time it opens,
// checks every field as it is typed, says what is wrong under the field and
// keeps Save off while anything is.
Item {
    id: form
    required property Store inbox
    required property bool open

    readonly property Inbox.TitleError titleError: Inbox.titleError(title.text)
    readonly property Inbox.TagsError tagsError: Inbox.tagsError(tags.text)
    readonly property Inbox.BodyError bodyError: Inbox.bodyError(body.text)
    readonly property bool valid: form.titleError === "" && form.tagsError === "" && form.bodyError === ""

    // The choices are kept as what they name, and the boxes' indices bound
    // to them: a model made again (the people arriving a turn after the
    // records, the language changing the names) shows the same choice.
    property int priorityChoice: 2
    property string assigneeChoice: ""

    function reset(): void {
        const issue = form.inbox.selected
        title.text = issue.title
        tags.text = issue.tags.join(" ")
        body.text = issue.body
        form.priorityChoice = issue.priority
        form.assigneeChoice = issue.assignee
    }

    onOpenChanged: {
        if (form.open) {
            form.reset()
        }
    }

    readonly property int shownId: form.inbox.selectedId
    onShownIdChanged: {
        if (form.open) {
            form.reset()
        }
    }

    Text {
        x: 20
        y: 16
        text: t`Title`
        color: Theme.textMuted
        font.size: 12
        font.weight: Font.DemiBold
    }

    Text {
        elide: Text.ElideRight
        x: 20
        y: 16
        width: 540
        align: Text.AlignRight
        visible: title.text.length > 100
        text: `${title.text.length}/${Inbox.titleLimit}`
        color: title.text.length > Inbox.titleLimit ? Theme.errorForeground : Theme.textMuted
        font.size: 12
    }

    TextField {
        id: title
        objectName: "title"
        x: 20
        y: 40
        width: 540
        height: 40
        Accessible.name: t`Title`
        Accessible.description: error.text
    }

    Text {
        elide: Text.ElideRight
        id: error
        x: 20
        y: 84
        width: 540
        height: 20
        text: form.titleError === "required" ? t`Title is required`
            : form.titleError === "long" ? t`At most 120 characters` : ""
        Accessible.role: Accessible.StaticText
        Accessible.name: text
        color: Theme.errorForeground
        font.size: 12
    }

    Text {
        x: 20
        y: 108
        text: t`Tags`
        color: Theme.textMuted
        font.size: 12
        font.weight: Font.DemiBold
    }

    TextField {
        id: tags
        objectName: "tags"
        x: 20
        y: 130
        width: 540
        height: 40
        Accessible.name: t`Tags`
        Accessible.description: tagsMessage.text
    }

    Text {
        elide: Text.ElideRight
        id: tagsMessage
        x: 20
        y: 174
        width: 540
        height: 20
        text: form.tagsError === "words" ? t`Tags are lowercase words`
            : form.tagsError === "many" ? t`At most 6 tags` : ""
        Accessible.role: Accessible.StaticText
        Accessible.name: text
        color: Theme.errorForeground
        font.size: 12
    }

    // The fields' frame, which a scrolled text area does not draw itself.
    Rectangle {
        x: 20
        y: 200
        width: 540
        height: 320
        radius: Theme.radius
        fill: Theme.canvas
        border.width: body.activeFocus ? 2 : 1
        border.color: body.activeFocus ? Theme.focus : Theme.input
    }

    ScrollView {
        x: 20
        y: 200
        width: 540
        height: 320

        TextArea {
            id: body
            objectName: "body"
            wrapMode: TextEdit.Wrap
            placeholderText: t`Description`
            Accessible.name: t`Description`
            Accessible.description: bodyMessage.text
        }
    }

    Text {
        elide: Text.ElideRight
        id: bodyMessage
        x: 20
        y: 522
        width: 540
        height: 20
        text: form.bodyError === "long" ? t`At most 10 000 characters` : ""
        Accessible.role: Accessible.StaticText
        Accessible.name: text
        color: Theme.errorForeground
        font.size: 12
    }

    ComboBox {
        id: priority
        x: 20
        y: 546
        width: 130
        height: 28
        Accessible.name: t`Priority`
        model: form.inbox.priorityNames()
        currentIndex: form.priorityChoice
        onActivated: (index: number) => {
            form.priorityChoice = index
        }
    }

    ComboBox {
        id: assignee
        x: 160
        y: 546
        width: 170
        height: 28
        Accessible.name: t`Assignee`
        model: form.inbox.assignees()
        // Nobody until the people are gathered, then the one chosen.
        currentIndex: form.assigneeChoice === "" ? 0 : form.inbox.people.indexOf(form.assigneeChoice) + 1
        onActivated: (index: number) => {
            form.assigneeChoice = index === 0 ? "" : form.inbox.people[index - 1] ?? ""
        }
    }

    Button {
        objectName: "cancel"
        x: 340
        y: 544
        width: 100
        height: 32
        text: t`Cancel`
        onClicked: {
            form.reset()
            form.inbox.tab = 0
        }
    }

    Button {
        objectName: "save"
        x: 446
        y: 544
        width: 100
        height: 32
        primary: true
        enabled: form.valid
        text: t`Save`
        onClicked: form.inbox.save(title.text, tags.text, form.priorityChoice, form.assigneeChoice, body.text)
    }
}
