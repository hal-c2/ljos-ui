# Kestrel, a smartwatch

The software of a made-up watch, written in the Ljós dialect. It is laid out for
two screens: a round one of 454 × 454 pixels and a rounded square of 410 × 502.
Every screen lays itself out for the shape it is given. There is also a 3D model
of the watch, with the live software as the texture of its screen.

```sh
ljos run examples/smartwatch/Main.qml       # the watch, round
ljos run examples/smartwatch/Showcase.qml   # the watch in 3D, on a turntable
```

## Wearing it

| On the watch      | Here                                      |
|-------------------|-------------------------------------------|
| Swipe and tap     | Drag and click                            |
| The crown, turned | The mouse wheel                           |
| The crown, pressed| Return                                    |
| The side button   | Escape or Home: wakes the watch, goes home|

From the face:

- **Swipe up:** notifications.
- **Swipe down:** quick settings.
- **Swipe left:** the launcher.
- **Swipe right:** back.
- **Hold the face:** switches to the other face.

### Screens

- **Two faces.**
  - *Pulse* is analogue. It shows the date at three, the move ring at nine and the heart rate at six.
  - *Grid* is digital, inside the three activity rings, with the next event.
- **Notifications:** a list and the message itself, which can be dismissed or all cleared.
- **Heart:** the heart rate with a chart of the last forty samples, simulated and the same on every run, and the day's rings.
- **Workout:** an outdoor run that counts time, heart rate, distance and calories, and can be paused, resumed and ended.
- **Timer:** set with the crown. Its ring runs down as it counts.
- **Stopwatch:** tenths of a second and laps.
- **Music:** four invented tracks. The crown is the volume.
- **Quick settings:** do not disturb, always on, and glow (the bloom).
- **Launcher:** six apps round a ring and one in the middle. The crown moves through them.

## At rest

Left alone for fifteen seconds, the watch sleeps:

- **With always on:** it shows the always-on face. This face is dim outlines with no second hand, and it drifts a few pixels every minute against burn-in.
- **With always on off:** the screen goes dark.

Asleep, any touch wakes it: a tap, a hold, or the start of a swipe.

Asleep, the only thing that runs is the clock, which wakes once at the start of each minute. Awake, the same rule holds:

- A second hand ticks only while it is on screen.
- The heart is sampled only on the screens that show it.
- A countdown has one wake: its end.

A running workout, stopwatch or track counts from the time it started, never from ticks, so it keeps its time while the screen sleeps.

## The look

- **Dark only:** the screen is OLED, so black is off.
- **Neon is kept for what matters:** only the lit ring, the hands and the highlighted control glow. *Glow* in quick settings turns the bloom off.
- **Reduced motion:** a screen does not slide in: it is in place at once and fades in, as Ljós does with every movement. The heart beats with its glow instead of its size, and the 3D watch turns five times slower, even when it is asked for while the watch is already turning. Nothing that tells you something changed is lost.
- **Contrast:** `ljos audit Main.qml --touch` finds no contrast errors at either size.

## The files

| File | What it is |
|---|---|
| `Main.qml` | The display: timers, keys, crown, the bezel mask |
| `Watch.qml` | Everything the watch is; the screens only read it and call its functions |
| `WallClock.qml` | The time of day, and the once-a-minute wake |
| `Pages.qml` | The screens and the swipes between them |
| `Face*.qml`, `AmbientFace.qml` | The faces |
| `*Page.qml`, `Note*.qml`, `Launcher.qml`, `QuickSettings.qml` | The other screens |
| `logic/*.ts` | Typed TypeScript: navigation, the simulated body, dial geometry, music, notifications |
| `Showcase.qml`, `WatchScene.qml`, `Strip.qml` | The 3D watch |
| `assets/watch/` | The model, and the Blender script that builds it (`assets/watch/README.md`) |

`features/examples/smartwatch.feature` holds the example to this README.
