// The properties a launcher gives an example's root (in Qt,
// setInitialProperties), by the end of the entry's path: those a root
// `required` property needs. They are the ones the Ljós gallery's
// backends give (examples/gallery_backends).
.pragma library

function of(entry) {
    if (entry.endsWith("/mediaplayer/MediaPlayer/Main.qml"))
        return { source: "", nameFilters: ["MPEG-4 (*.mp4 *.m4v *.m4a *.mov)", "Matroska (*.mkv *.webm)",
            "AVI (*.avi)", "MPEG transport stream (*.ts *.m2ts)", "Ogg (*.ogg *.oga *.opus)",
            "FLAC (*.flac)", "MP3 (*.mp3)"], selectedNameFilter: 0 }
    return {}
}
