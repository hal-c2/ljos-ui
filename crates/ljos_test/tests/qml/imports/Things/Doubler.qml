import QtQuick

// A test-only helper, found through `-import imports`.
QtObject {
    property int input: 0
    readonly property int output: input * 2
}
