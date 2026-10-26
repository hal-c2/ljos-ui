// expect: text-elided-hidden at 11, layout-under-minimum at 11
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 200
    Menu {
        width: 200
        Component.onCompleted: open()
        MenuItem { text: "Unavailable because the account has reached its limit"; enabled: false }
    }
}
