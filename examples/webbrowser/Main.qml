import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtWebEngine

// A browser in one window: back, forward, reload or stop, a URL field,
// the page's title as the page's own script reads it, and what went
// wrong when a page cannot be shown.
ApplicationWindow {
    id: window
    width: 900
    height: 640
    visible: true
    title: pageTitle.length > 0 ? pageTitle + " — Browser" : "Browser"

    property string pageTitle: ""
    property string failure: ""

    header: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 6
            anchors.rightMargin: 6
            spacing: 4

            ToolButton {
                objectName: "back"
                text: "‹"
                font.pixelSize: 20
                enabled: view.canGoBack
                onClicked: view.goBack()
                ToolTip.visible: hovered
                ToolTip.text: "Back"
            }
            ToolButton {
                objectName: "forward"
                text: "›"
                font.pixelSize: 20
                enabled: view.canGoForward
                onClicked: view.goForward()
                ToolTip.visible: hovered
                ToolTip.text: "Forward"
            }
            ToolButton {
                objectName: "reload"
                text: view.loading ? "✕" : "⟳"
                font.pixelSize: 16
                onClicked: view.loading ? view.stop() : view.reload()
                ToolTip.visible: hovered
                ToolTip.text: view.loading ? "Stop" : "Reload"
            }
            TextField {
                id: address
                objectName: "address"
                Layout.fillWidth: true
                selectByMouse: true
                placeholderText: "Type a URL and press Enter"
                text: view.url
                onAccepted: view.url = text.indexOf(":") > 0 ? text : "https://" + text
            }
            ToolButton {
                objectName: "home"
                text: "⌂"
                font.pixelSize: 16
                onClicked: view.url = Qt.resolvedUrl("pages/index.html")
                ToolTip.visible: hovered
                ToolTip.text: "Home"
            }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        ProgressBar {
            objectName: "progress"
            Layout.fillWidth: true
            from: 0
            to: 100
            value: view.loadProgress
            opacity: view.loading ? 1 : 0
        }

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            WebEngineView {
                id: view
                objectName: "view"
                anchors.fill: parent
                url: Qt.resolvedUrl("pages/index.html")

                onLoadingChanged: info => {
                    if (info.status === WebEngineView.LoadStartedStatus) {
                        window.failure = ""
                    } else if (info.status === WebEngineView.LoadFailedStatus) {
                        window.pageTitle = ""
                        window.failure = info.errorString
                    } else if (info.status === WebEngineView.LoadSucceededStatus) {
                        runJavaScript("document.title", t => window.pageTitle = t || "")
                    }
                }
            }

            Rectangle {
                objectName: "error"
                anchors.fill: parent
                visible: window.failure.length > 0
                color: palette.window

                ColumnLayout {
                    anchors.centerIn: parent
                    width: Math.min(parent.width - 48, 420)
                    spacing: 12

                    Label {
                        Layout.fillWidth: true
                        text: "This page cannot be shown"
                        font.pixelSize: 22
                        font.bold: true
                        wrapMode: Text.Wrap
                    }
                    Label {
                        objectName: "errorText"
                        Layout.fillWidth: true
                        text: view.url + "\n" + window.failure
                        wrapMode: Text.WrapAnywhere
                        opacity: 0.75
                    }
                    Button {
                        text: "Try again"
                        onClicked: view.reload()
                    }
                }
            }
        }

        Label {
            objectName: "status"
            Layout.fillWidth: true
            leftPadding: 8
            topPadding: 4
            bottomPadding: 4
            text: window.failure.length > 0 ? "Failed" : view.loading ? "Loading…" : "Title: " + window.pageTitle
            elide: Text.ElideRight
        }
    }
}
