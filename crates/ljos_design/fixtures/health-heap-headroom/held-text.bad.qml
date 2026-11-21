// expect: health-heap-headroom at 4
import QtQuick

Item {
    id: root
    // 74 MiB of text held for the life of the window, of a heap of 96.
    property var cache: ({ rows: [] })
    Component.onCompleted: {
        for (var i = 0; i < 740; i++)
            root.cache.rows.push("x".repeat(100 * 1024) + i)
    }
}
