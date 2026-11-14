// What Qt's own renderer says a 3D frame cost, for `mise run bench:3d`:
// `qml6 renderstats-probe.qml -- <scene.qml>` opens the scene and once a
// second logs a line `renderstats {…}` of every View3D's renderStats. Qt's
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
        const component = Qt.createComponent("file://" + args[args.length - 1]);
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
