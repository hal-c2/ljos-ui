---
title: Multimedia and Sensors
description: Play audio and video with QtMultimedia and read device sensors with QtSensors while keeping idle CPU usage at 0 %.
section: rust
order: 8
---

# Multimedia and Sensors

After reading this page, you will be able to play local and streamed audio and video with `MediaPlayer`, `AudioOutput` and `VideoOutput`, play low-latency sound effects from memory with `SoundEffect`, inspect audio and video devices with `MediaDevices`, and read hardware sensors with `QtSensors`.

## Playing audio and video with `MediaPlayer`

The `QtMultimedia` module provides `MediaPlayer`, `AudioOutput`, `VideoOutput`, `Video`, `VideoSink`, `MediaDevices`, `AudioDevice`, `SoundEffect` and `MediaMetaData`.

A `MediaPlayer` opens a local file or URL, reports what tracks and metadata it found, and sends audio and video frames to the `AudioOutput` and `VideoOutput` connected to it:

```qml
import QtQuick
import QtMultimedia

Item {
    width: 320
    height: 240

    property string title: ""

    MediaPlayer {
        id: player
        objectName: "player"
        source: "clip.mp4"
        audioOutput: AudioOutput {}
        videoOutput: output
        onMetaDataChanged: title = metaData.stringValue(MediaMetaData.Title)
    }

    VideoOutput {
        id: output
        objectName: "output"
        anchors.fill: parent
    }
}
```

How video display and audio devices behave:

- **`VideoOutput` and `Video`.** `VideoOutput` displays frames from a `MediaPlayer`, fitted according to its `fillMode`. `sourceRect` gives the frame's native dimensions and `contentRect` gives the rectangle where the fitted video is drawn inside the item. `Video` is a convenience item that combines a `VideoOutput` with its own internal `MediaPlayer`.
- **`VideoSink`.** Every `VideoOutput` owns a `videoSink` (`VideoSink`), and you can also assign a standalone `VideoSink` directly to `MediaPlayer.videoOutput`. `videoSink.videoSize` reports the dimensions of the video (`-1x-1` when no video is loaded, updated while a source with video loads, and reset to `-1x-1` when playback stops or reaches the end of the stream). `videoSink.subtitleText` holds the current subtitle text, and `onVideoFrameChanged` fires for every frame and once more when the picture is cleared.
- **`MediaDevices` and headless machines.** `MediaDevices` exposes `audioOutputs`, `audioInputs`, `videoInputs` and `defaultAudioOutput` (which `AudioOutput.device` defaults to). On a machine with no sound card—such as a CI runner or headless test—the device lists are empty, audio output is discarded, and every `MediaPlayer` still plays at the pace of its own clock.

## Network streams and media byte budgets

`MediaPlayer.source` can point to an HTTP or HTTPS URL as well as a local file.

When the source is a stream of pictures without a container (such as an MJPEG stream `.mjpeg` or a raw H.264 stream `.h264`), there is no container header to report a total length: the player enters `MediaPlayer.LoadedMedia` with the dimensions of the first picture, `hasVideo: true` and `duration: 0`, and plays each picture until the stream ends.

To keep untrusted or malformed streams from exhausting memory, Ljós enforces two 16 MiB byte budgets on network media:

- **Whole media containers (16 MiB).** A container file (such as `.mp4`) fetched from a server is limited to 16 MiB (`16,777,216` bytes). If a streamed container exceeds 16 MiB, the player stops and reports a network error (`error: 3`) with `errorString` set to `"media container exceeds byte budget"`.
- **Incomplete pictures in live streams (16 MiB).** While a live picture stream (such as MJPEG or H.264) can run indefinitely across many pictures, a single incomplete picture or access unit that accumulates more than 16 MiB of pending data without ending fails with `errorString` set to `"media unit exceeds byte or NAL budget"`. Any completed pictures received before the oversized unit are delivered first.

## Short sounds with `SoundEffect`

For short UI sounds such as clicks, alerts and chimes, use `SoundEffect`. Unlike `MediaPlayer`, `SoundEffect` decodes the entire audio clip into memory when its `source` property is set and plays it directly from memory:

```qml
import QtQuick
import QtMultimedia

Item {
    width: 120
    height: 48

    // <!-- check: verify SoundEffect properties beyond source and status against sound-effect.feature -->
    SoundEffect {
        id: chime
        source: "beep.wav"
    }

    MouseArea {
        anchors.fill: parent
        onClicked: chime.play()
    }
}
```

## Zero work when media is at rest

Multimedia components obey the same 0 % idle CPU guarantee as the rest of Ljós:

- A `MediaPlayer` that is loaded and paused or stopped schedules no timers, decodes no frames and requests no repaints.
- A `SoundEffect` that is not currently playing schedules no timers or audio work.
- A video whose `VideoOutput` is not visible asks for no frames and no decoding work.

## Reading hardware sensors with `QtSensors`

The `QtSensors` module provides `Sensor`, `Accelerometer`, `Gyroscope`, `Magnetometer`, `Compass`, `PressureSensor`, `AmbientLightSensor`, `OrientationSensor` and their reading types:

```qml
import QtQuick
import QtSensors

Item {
    Accelerometer {
        id: acc
        objectName: "acc"
        property bool started: true
        property bool hasReading: reading !== null
        Component.onCompleted: started = start()
    }
}
```

How `QtSensors` interacts with the platform and the UI thread:

- **Lifecycle and `SensorSink`.** `QtSensors` reads hardware through the platform's `SensorService`. An inactive sensor holds no resources from the service and receives no readings. Calling `start()` on a sensor connected to a backend activates it, registers a `SensorSink` with the background device reader, and exposes an initial `reading` (starting at all zeros, which triggers the first `readingChanged`).
- **Coalescing and `dataRate`.** When a background thread feeds readings into a `SensorSink` faster than the display frame rate, the first reading posts a single job to the platform inbox and subsequent readings replace the pending value in place until the frame runs. If a reading arrives earlier than the sensor's configured `dataRate` allows, the UI side holds it on a timer until its interval elapses and delivers only the latest reading.
- **Behaviour without a backend.** The default stand-in service is `NoSensors` (and no host ships a native hardware sensor backend yet; on Linux, the planned backend in `ljos_shell` is `iio-sensor-proxy` over the system D-Bus for `OrientationSensor`, `AmbientLightSensor` and `Compass`). When no backend is present for a sensor type, `connectedToBackend` is `false`, `reading` is `null`, and calling `start()` returns `false` while `active` remains `false`. In Rust tests, you can install `FakeSensors` on the platform to feed deterministic readings.

## See also

- [How a Rust and QML App Fits Together](./overview.md)
- [Threads, Tasks and UiHandle](./threads-and-uihandle.md)
- [Testing a Rust and QML App](./testing-rust.md)
- [Desktop, Mobile and the Platform Host](./platforms.md)
