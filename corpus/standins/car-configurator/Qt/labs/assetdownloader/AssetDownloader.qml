// Stands in for AssetDownloader, which Qt links into the example and which
// downloads an archive and unpacks it. `mise run gallery:assets car-configurator`
// has done that and linked the result as `downloaded` beside these stand-ins.
import QtQml

QtObject {
    id: downloader

    property url downloadBase
    property url preferredLocalDownloadDir
    property url offlineAssetsFilePath
    property string jsonFileName
    property string zipFileName
    readonly property url localDownloadDir: Qt.resolvedUrl("../../../downloaded")

    signal started()
    signal finished(bool success)
    signal progressChanged(int progressValue, int progressMaximum, string progressText)

    function start() {
        started()
        Qt.callLater(() => downloader.finished(true))
    }
}
