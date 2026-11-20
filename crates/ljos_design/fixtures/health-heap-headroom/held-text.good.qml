import QtQuick

Item {
    id: root
    // 1 MiB of text: the heap keeps its room.
    property var cache: ({ rows: [] })
    Component.onCompleted: {
        for (var i = 0; i < 10; i++)
            root.cache.rows.push("x".repeat(100 * 1024) + i)
    }
}
