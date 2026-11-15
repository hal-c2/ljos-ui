// What Qt's own renderer says a 3D frame cost, for `mise run bench:3d`:
// `qml6 renderstats-probe.qml -- <scene.qml> [call…]` opens the scene, calls
// the functions of its root named after it, as an example's own program does
// ("launcher" in corpus/examples.json; FX Material Showroom loads its 3D
// screen only then), and once a second logs a line `renderstats {…}` of
// every View3D's renderStats. Qt's
// numbers cannot be read from outside the process, and the scene is not
// changed for them: this file only loads it. Detailed counts (draw calls,
// passes) are Qt's to collect only while extendedDataCollectionEnabled is
// on, which is set here, so Qt's frames carry that small cost too.
import QtQuick
import QtQuick3D

QtObject {
    id: probe

    property var scene: null
    property var views: []
    property int second: 0

    // Qt's renderStats properties, those of them a version has.
    readonly property var names: [
        "fps", "frameTime", "renderTime", "renderPrepareTime", "syncTime",
        "maxFrameTime", "lastCompletedGpuTime", "drawCallCount", "drawVertexCount",
        "renderPassCount", "imageDataSize", "meshDataSize", "textureCount",
        "meshCount", "pipelineCount", "materialGenerationTime", "effectGenerationTime"
    ]

    function find(item) {
        if (item.renderStats !== undefined && item.environment !== undefined)
            views.push(item);
        for (let i = 0; i < item.children.length; ++i)
            find(item.children[i]);
    }

    Component.onCompleted: {
        const args = Qt.application.arguments;
        const dashes = args.indexOf("--");
        const component = Qt.createComponent("file://" + args[dashes + 1]);
        if (component.status !== Component.Ready) {
            console.log("renderstats-error " + component.errorString());
            Qt.exit(2);
            return;
        }
        scene = component.createObject(null);
        if (scene === null) {
            console.log("renderstats-error " + component.errorString());
            Qt.exit(2);
            return;
        }
        for (const call of args.slice(dashes + 2)) {
            if (typeof scene[call] !== "function") {
                console.log("renderstats-error the scene has no function " + call);
                Qt.exit(2);
                return;
            }
            scene[call]();
        }
        // A view the calls made (a Loader's) is there now; one made later is not counted.
        find(scene.contentItem !== undefined ? scene.contentItem : scene);
        for (const v of views) {
            if (v.renderStats.extendedDataCollectionEnabled !== undefined)
                v.renderStats.extendedDataCollectionEnabled = true;
        }
        console.log("renderstats-views " + views.length);
    }

    property Timer tick: Timer {
        interval: 1000
        repeat: true
        running: true
        onTriggered: {
            probe.second += 1;
            for (let i = 0; i < probe.views.length; ++i) {
                const stats = probe.views[i].renderStats;
                const line = { second: probe.second, view: i };
                for (const name of probe.names) {
                    if (stats[name] !== undefined)
                        line[name] = stats[name];
                }
                console.log("renderstats " + JSON.stringify(line));
            }
        }
    }
}
