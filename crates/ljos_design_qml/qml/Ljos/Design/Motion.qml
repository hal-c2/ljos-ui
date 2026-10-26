// Written by ljos_design (crates/ljos_design/src/module.rs); `ljos design export` writes it again.
pragma Singleton
import QtQuick

// How long a change takes and the curve it follows, by what it does,
// how much of the window it moves and how often a user meets it.
QtObject {
    // The user asked for less motion: move nothing, cross-fade instead.
    readonly property bool reduced: Qt.styleHints.accessibility !== undefined && Qt.styleHints.accessibility.motionPreference === 1

    // Milliseconds: enterPanel is met now and then, enterPanelFrequent
    // many times a day, enterPanelRare once in a while. A loop's is one cycle.
    readonly property int enterSmallRare: 200
    readonly property int enterSmall: 150
    readonly property int enterSmallFrequent: 100
    readonly property int enterPanelRare: 350
    readonly property int enterPanel: 250
    readonly property int enterPanelFrequent: 100
    readonly property int enterScreenRare: 420
    readonly property int enterScreen: 300
    readonly property int enterScreenFrequent: 100
    readonly property int exitSmallRare: 140
    readonly property int exitSmall: 100
    readonly property int exitSmallFrequent: 70
    readonly property int exitPanelRare: 280
    readonly property int exitPanel: 200
    readonly property int exitPanelFrequent: 70
    readonly property int exitScreenRare: 340
    readonly property int exitScreen: 240
    readonly property int exitScreenFrequent: 70
    readonly property int moveSmallRare: 200
    readonly property int moveSmall: 150
    readonly property int moveSmallFrequent: 100
    readonly property int movePanelRare: 350
    readonly property int movePanel: 250
    readonly property int movePanelFrequent: 100
    readonly property int moveScreenRare: 420
    readonly property int moveScreen: 300
    readonly property int moveScreenFrequent: 100
    readonly property int expandSmallRare: 200
    readonly property int expandSmall: 150
    readonly property int expandSmallFrequent: 100
    readonly property int expandPanelRare: 350
    readonly property int expandPanel: 250
    readonly property int expandPanelFrequent: 100
    readonly property int expandScreenRare: 420
    readonly property int expandScreen: 300
    readonly property int expandScreenFrequent: 100
    readonly property int collapseSmallRare: 140
    readonly property int collapseSmall: 100
    readonly property int collapseSmallFrequent: 70
    readonly property int collapsePanelRare: 280
    readonly property int collapsePanel: 200
    readonly property int collapsePanelFrequent: 70
    readonly property int collapseScreenRare: 340
    readonly property int collapseScreen: 240
    readonly property int collapseScreenFrequent: 70
    readonly property int sheetInSmallRare: 200
    readonly property int sheetInSmall: 200
    readonly property int sheetInSmallFrequent: 100
    readonly property int sheetInPanelRare: 420
    readonly property int sheetInPanel: 300
    readonly property int sheetInPanelFrequent: 100
    readonly property int sheetInScreenRare: 490
    readonly property int sheetInScreen: 350
    readonly property int sheetInScreenFrequent: 100
    readonly property int sheetOutSmallRare: 160
    readonly property int sheetOutSmall: 150
    readonly property int sheetOutSmallFrequent: 70
    readonly property int sheetOutPanelRare: 340
    readonly property int sheetOutPanel: 240
    readonly property int sheetOutPanelFrequent: 70
    readonly property int sheetOutScreenRare: 390
    readonly property int sheetOutScreen: 280
    readonly property int sheetOutScreenFrequent: 70
    readonly property int gestureSmallRare: 200
    readonly property int gestureSmall: 150
    readonly property int gestureSmallFrequent: 100
    readonly property int gesturePanelRare: 350
    readonly property int gesturePanel: 250
    readonly property int gesturePanelFrequent: 100
    readonly property int gestureScreenRare: 420
    readonly property int gestureScreen: 300
    readonly property int gestureScreenFrequent: 100
    readonly property int loopSmallRare: 1000
    readonly property int loopSmall: 1000
    readonly property int loopSmallFrequent: 1000
    readonly property int loopPanelRare: 1200
    readonly property int loopPanel: 1200
    readonly property int loopPanelFrequent: 1200
    readonly property int loopScreenRare: 1500
    readonly property int loopScreen: 1500
    readonly property int loopScreenFrequent: 1500
    readonly property int press: 100

    // easing.bezierCurve lists, for easing.type: Easing.BezierSpline.
    readonly property var easeOut: [0.23, 1, 0.32, 1, 1, 1]
    readonly property var easeOutSoft: [0.165, 0.84, 0.44, 1, 1, 1]
    readonly property var easeInOut: [0.77, 0, 0.175, 1, 1, 1]
    readonly property var decelerate: [0.05, 0.7, 0.1, 1, 1, 1]
    readonly property var drawer: [0.32, 0.72, 0, 1, 1, 1]
    readonly property var linear: [0, 0, 1, 1, 1, 1]

    // What an entrance grows from and an exit shrinks to.
    readonly property real enterScale: 0.95
    readonly property real pressScale: 0.97

    // SpringAnimation numbers for what follows a finger, in pixels.
    readonly property real gestureSmallSpring: 7.018
    readonly property real gestureSmallDamping: 0.57
    readonly property real gestureSmallMass: 1
    readonly property real gestureSmallEpsilon: 0.25
    readonly property real gesturePanelSpring: 3.948
    readonly property real gesturePanelDamping: 0.427
    readonly property real gesturePanelMass: 1
    readonly property real gesturePanelEpsilon: 0.25
    readonly property real gestureScreenSpring: 2.527
    readonly property real gestureScreenDamping: 0.342
    readonly property real gestureScreenMass: 1
    readonly property real gestureScreenEpsilon: 0.25
}
