// The simulated drive: a loop of legs the cockpit steps through, one tick a
// second, while the car is in Drive. Everything here is pure: the QML holds
// the state and its timers, so a parked car runs no code at all.

export type Gear = "P" | "R" | "N" | "D"
export type Signal = "none" | "left" | "right"

// One stretch of the loop: the speed the driver settles at, how long the
// stretch lasts, the indicator they show and what the assist sees ahead.
export interface Leg {
    readonly speed: number
    readonly seconds: number
    readonly signal: Signal
    // Metres to the car ahead in this lane, or 0 when the lane is clear.
    readonly lead: number
    // Where the car sits in its lane, -1 (left line) to 1 (right line).
    readonly drift: number
    // The speed limit of the road, km/h.
    readonly limit: number
}

const town: Leg = { speed: 42, seconds: 9, signal: "none", lead: 0, drift: 0, limit: 50 }

export const legs: readonly Leg[] = [
    town,
    { speed: 48, seconds: 6, signal: "none", lead: 38, drift: 0, limit: 50 },
    { speed: 24, seconds: 5, signal: "right", lead: 0, drift: 0.35, limit: 50 },
    { speed: 66, seconds: 8, signal: "none", lead: 0, drift: 0, limit: 80 },
    { speed: 92, seconds: 10, signal: "none", lead: 54, drift: 0, limit: 100 },
    { speed: 104, seconds: 7, signal: "left", lead: 32, drift: -0.4, limit: 100 },
    { speed: 112, seconds: 9, signal: "none", lead: 0, drift: 0, limit: 110 },
    { speed: 78, seconds: 7, signal: "none", lead: 46, drift: 0, limit: 80 },
    { speed: 34, seconds: 6, signal: "left", lead: 0, drift: -0.3, limit: 50 },
    { speed: 0, seconds: 4, signal: "none", lead: 12, drift: 0, limit: 50 },
]

export function legAt(index: number): Leg {
    const count = legs.length
    return legs[((index % count) + count) % count] ?? town
}

// Reverse is a crawl, Neutral and Park coast down to a stop.
export function targetSpeed(gear: Gear, leg: Leg): number {
    switch (gear) {
        case "D": return leg.speed
        case "R": return 6
        case "N": return 0
        case "P": return 0
    }
}

// Kilowatts drawn (positive) or recovered (negative) as the car goes from
// `from` to `to` km/h over a second, plus what holding a speed costs.
export function power(from: number, to: number): number {
    const accel = (to - from) * 2.4
    const hold = to * to * 0.0045 + to * 0.06
    const kw = accel > 0 ? accel + hold : accel * 0.7 + hold
    return Math.round(Math.max(-60, Math.min(180, kw)))
}

// Kilometres covered in `seconds` at `kmh`.
export function distance(kmh: number, seconds: number): number {
    return (kmh * seconds) / 3600
}

// What a kilometre costs of the range at a speed: more above 90 km/h.
export function rangeCost(kmh: number): number {
    return kmh > 90 ? 1 + (kmh - 90) * 0.012 : 1
}

export function toUnit(kmh: number, miles: boolean): number {
    return Math.round(miles ? kmh * 0.621371 : kmh)
}

// The gears in the order the selector shows them.
export const gears: readonly Gear[] = ["P", "R", "N", "D"]

export function gearAfter(gear: Gear, step: number): Gear {
    const at = gears.indexOf(gear) + step
    return gears[Math.max(0, Math.min(gears.length - 1, at))] ?? gear
}
