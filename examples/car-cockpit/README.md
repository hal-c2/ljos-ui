# Car cockpit

A cockpit for Svala GT, a made-up car: an instrument cluster above an
infotainment touchscreen, in a dark neon look with a light one beside it.

```sh
mise run dev examples/car-cockpit/Main.qml
```

## What it shows

- **Cluster.** Speed and power dials, the gear (P R N D), range and charge,
  the turn signals and hazard lights, warnings, and a driving-assist view of
  the lane and the car ahead.
- **Car.** The car in 3D, from `assets/car/` (a Blender script; see its
  README). Tap a part on the car, or its button at the side, to open or close
  the doors, frunk, trunk or charge port. A car that is locked, out of park
  or moving keeps them shut and says why, and shifting out of park closes
  them. Arrow keys choose a part and Space opens it.
- **Climate.** Both temperatures, linked or apart, the fan, seat heating,
  defrost, heating or cooling.
- **Media.** Invented tracks, play, pause and skip, and a visualiser that runs
  only while music plays.
- **Navigation.** A map drawn in code, the route ahead and the next turn,
  following the car.
- **Settings.** Dark, light or as the system; the glow on or off; kilometres or
  miles; °C or °F; the simulated drive and the hazard lights.
- **All apps.** Every page as a tile.

## How it is made

- `Cockpit.qml` is everything the car is. The pages only read it and call its
  functions, so a test or an agent drives the car through the same doors a
  finger does (`features/examples/car-cockpit.feature`).
- `logic/*.ts` holds the typed rules: the drive's legs, the media library, the
  map and the route, climate units, the dials' geometry and the icons.
- The 2D pages are written in the Ljós dialect. `CarScene.qml` and
  `WallClock.qml` are QML as Qt writes it: the first because the 3D scene uses
  `QtQuick3D`, the second because it reads the local time from `Date`.
- The glow is `Neon.qml` on 2D (neon edges, the dials' arcs and needles, the
  active control) and the scene environment's glow on 3D (the car's light
  strips). Text is never blurred. Settings → Glow turns both off.

## Nothing runs at rest

Parked, with the music paused, the cockpit does no work. The drive steps once
a second only while the car is in gear or still rolling, the player counts
only while it plays, the visualiser runs only while it plays and is shown, and
the clock wakes once a minute, on the minute. Turn **Simulated drive** off and
a car in gear stays where it is.

Everything here is original: the car, its name, the icons, the map, the
streets and the music.
