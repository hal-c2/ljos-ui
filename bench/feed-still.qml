// The feed (bench/feed/Feed.qml) with nothing animating: what it costs to
// sit idle, to scroll, to type into its search field and to be resized.
import QtQuick
import "feed"

Feed {
    width: 1024
    height: 720
    spinning: false
}
