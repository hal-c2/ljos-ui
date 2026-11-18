import Ljos.Ui
import Ljos.Core
import "logic/drive.ts" as Drive
import "logic/car.ts" as Car
import "logic/media.ts" as Media
import "logic/climate.ts" as Climate

// Everything the car is: what the cluster shows, what the touchscreen
// changes. The views only read it and call its functions, so a test (or
// an agent) drives the car through the same doors a finger does.
Object {
    id: car

    // ----- Driving ------------------------------------------------------

    property Drive.Gear gear: "P"
    // When off, the car stays where it is: no leg advances, no timer runs.
    property bool simulate: true
    // km/h, as the drive sets it each second; the dials spring after it.
    property real speed: 0
    property int leg: 0
    property int legLeft: Drive.legAt(0).seconds
    readonly property Drive.Leg current: Drive.legAt(car.leg)
    // kW: positive draws, negative recovers.
    property real power: 0
    // State of charge, 0 to 1, and what it is worth in kilometres.
    property real charge: 0.78
    readonly property real range: car.charge * 520
    property real odometer: 18234.6
    // How far along the map's route the car is, 0 to 1.
    property real travelled: 0.08
    // The indicator the driver has on: the leg's, while driving forward.
    readonly property Drive.Signal signal: car.gear === "D" ? car.current.signal : "none"
    property bool hazard: false
    readonly property bool moving: car.speed > 0.5
    // The drive's clock runs while the car moves or is in gear to move.
    readonly property bool driving: car.simulate
        && (car.gear === "D" || car.gear === "R" || car.moving)

    // One second of the drive: settle toward the leg's speed, use or win
    // back charge, move along the map, and step to the next leg when this
    // one is done.
    function tick() {
        const target = Drive.targetSpeed(car.gear, car.current)
        // A car closes most of the gap in a second, not all of it.
        const next = Math.abs(target - car.speed) < 1 ? target : car.speed + (target - car.speed) * 0.55
        // A car that has come to rest draws nothing, whatever the last
        // second's braking won back: the clock may stop on this tick.
        car.power = car.gear === "P" || next === 0 ? 0 : Drive.power(car.speed, next)
        const km = Drive.distance(next, 1)
        car.odometer = car.odometer + km
        car.charge = Math.max(0.05, car.charge - (km * Drive.rangeCost(next)) / 520)
        car.travelled = (car.travelled + km / 9) % 1
        car.speed = next
        if (car.gear === "D") {
            if (car.legLeft <= 1) {
                car.leg = (car.leg + 1) % Drive.legs.length
                car.legLeft = Drive.legAt(car.leg).seconds
            } else {
                car.legLeft = car.legLeft - 1
            }
        }
    }

    // Leaving park closes what is open, so the car never drives off with
    // a door, the frunk or the trunk up.
    function shift(to: Drive.Gear) {
        if (to !== "P") {
            car.closeAll()
        }
        car.gear = to
        if (to === "P") {
            car.power = 0
        }
    }

    // ----- The body -------------------------------------------------------

    property bool doorLeft: false
    property bool doorRight: false
    property bool frunk: false
    property bool trunk: false
    property bool chargePort: false
    property bool locked: false
    readonly property int openCount: (car.doorLeft ? 1 : 0) + (car.doorRight ? 1 : 0)
        + (car.frunk ? 1 : 0) + (car.trunk ? 1 : 0) + (car.chargePort ? 1 : 0)

    function isOpen(part: Car.Part): bool {
        switch (part) {
            case "doorLeft": return car.doorLeft
            case "doorRight": return car.doorRight
            case "frunk": return car.frunk
            case "trunk": return car.trunk
            case "chargePort": return car.chargePort
        }
    }

    // Opens or closes a part. A locked car, one out of park or one that
    // is moving keeps its doors shut: the panel says why instead.
    function toggle(part: Car.Part): bool {
        if (!car.isOpen(part) && (car.locked || car.moving || car.gear !== "P")) {
            return false
        }
        switch (part) {
            case "doorLeft":
                car.doorLeft = !car.doorLeft
                break
            case "doorRight":
                car.doorRight = !car.doorRight
                break
            case "frunk":
                car.frunk = !car.frunk
                break
            case "trunk":
                car.trunk = !car.trunk
                break
            case "chargePort":
                car.chargePort = !car.chargePort
                break
        }
        return true
    }

    function closeAll() {
        car.doorLeft = false
        car.doorRight = false
        car.frunk = false
        car.trunk = false
        car.chargePort = false
    }

    // ----- Climate --------------------------------------------------------

    property real driverTemp: 21
    property real passengerTemp: 21
    property bool linked: true
    property int fan: 3
    property bool climateOn: true
    property bool cooling: true
    property bool defrost: false
    property int seatLeft: 0
    property int seatRight: 0

    function warmer(driver: bool, step: real) {
        if (driver || car.linked) {
            car.driverTemp = Climate.stepped(car.driverTemp, step)
        }
        if (!driver || car.linked) {
            car.passengerTemp = Climate.stepped(car.passengerTemp, step)
        }
    }

    // ----- Media ----------------------------------------------------------

    property bool playing: false
    property int track: 0
    // Seconds into the track.
    property int position: 0
    property int volume: 6
    readonly property Media.Track nowPlaying: Media.trackAt(car.track)

    function skip(step: int) {
        car.track = Media.next(car.track, step)
        car.position = 0
    }

    // One second of playback.
    function play() {
        if (car.position + 1 >= car.nowPlaying.seconds) {
            car.skip(1)
        } else {
            car.position = car.position + 1
        }
    }

    // ----- Settings ---------------------------------------------------------

    property bool miles: false
    property bool fahrenheit: false
    property Car.Page page: "nav"

    // What a page is called on its tile and to a screen reader.
    function pageTitle(page: Car.Page): string {
        switch (page) {
            case "nav": return t`Navigation`
            case "media": return t`Media`
            case "climate": return t`Climate`
            case "car": return t`Car`
            case "settings": return t`Settings`
            case "apps": return t`All apps`
        }
    }
}
