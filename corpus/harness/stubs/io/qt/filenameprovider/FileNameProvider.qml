// A stand-in for the mediaplayer demo's C++ module, so that qml6 runs the
// demo without building it. Written from what the demo's QML asks of it:
// the name of the file a path or URL ends in.
pragma Singleton
import QtQml

QtObject {
    function getFileName(path) {
        const s = String(path)
        return s.substring(s.lastIndexOf("/") + 1)
    }
}
