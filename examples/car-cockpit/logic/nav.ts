// The map: a made-up city drawn in code, on a 1000 by 640 plane, and the
// route the drive follows across it. Nothing here is a real place.

export interface Spot {
    readonly x: number
    readonly y: number
}

export interface Turn {
    // "left", "right" or "straight": the arrow the panel draws.
    readonly kind: "left" | "right" | "straight"
    readonly street: string
    // How far along the route the turn is, 0 to 1.
    readonly at: number
}

// The river: one wide curve across the plane.
export const river =
    "M -20 420 C 180 380 260 520 420 470 S 700 330 860 380 S 1000 470 1040 440"

// The parks, as closed shapes.
export const parks: readonly string[] = [
    "M 90 90 L 250 70 L 280 190 L 120 220 Z",
    "M 640 470 L 820 450 L 860 560 L 690 600 Z",
    "M 430 120 L 520 110 L 540 200 L 445 210 Z",
]

// The streets, each an open path: avenues run long, lanes fill between.
export const avenues: readonly string[] = [
    "M -20 300 L 1040 260",
    "M 340 -20 L 300 660",
    "M 720 -20 L 760 660",
    "M -20 560 L 1040 600",
    "M -20 40 L 1040 120",
]

export const lanes: readonly string[] = [
    "M 60 -20 L 40 660", "M 190 -20 L 170 660", "M 470 -20 L 450 660",
    "M 590 -20 L 600 660", "M 860 -20 L 900 660", "M 960 -20 L 1000 660",
    "M -20 170 L 1040 190", "M -20 430 L 1040 410", "M -20 360 L 1040 330",
]

// The route, as the points it turns at.
export const route: readonly Spot[] = [
    { x: 80, y: 600 },
    { x: 310, y: 585 },
    { x: 330, y: 290 },
    { x: 735, y: 270 },
    { x: 720, y: 60 },
    { x: 930, y: 90 },
]

const start: Spot = { x: 80, y: 600 }

function span(a: Spot, b: Spot): number {
    return Math.hypot(b.x - a.x, b.y - a.y)
}

export function length(): number {
    let total = 0
    for (let i = 1; i < route.length; i += 1) {
        total += span(route[i - 1] ?? start, route[i] ?? start)
    }
    return total
}

// The route as an SVG path, for a PathSvg.
export function routePath(): string {
    return route.map((p, i) => `${i === 0 ? "M" : "L"} ${p.x} ${p.y}`).join(" ")
}

// Where the car is when it has gone `fraction` of the route, and which
// way it faces there, in degrees clockwise from the +x axis.
export interface Fix {
    readonly x: number
    readonly y: number
    readonly heading: number
}

export function fixAt(fraction: number): Fix {
    let left = Math.max(0, Math.min(1, fraction)) * length()
    for (let i = 1; i < route.length; i += 1) {
        const a = route[i - 1] ?? start
        const b = route[i] ?? start
        const s = span(a, b)
        if (left <= s || i === route.length - 1) {
            const f = s > 0 ? Math.min(1, left / s) : 0
            return {
                x: a.x + (b.x - a.x) * f,
                y: a.y + (b.y - a.y) * f,
                heading: (Math.atan2(b.y - a.y, b.x - a.x) * 180) / Math.PI,
            }
        }
        left -= s
    }
    return { x: start.x, y: start.y, heading: 0 }
}

// The streets the route turns into, corner by corner, and where it ends.
const streets: readonly string[] = ["Ember Avenue", "Copper Line", "Harbour Rise", "Lantern Quay"]
const destination = "Lantern Quay 9"

// Each corner of the route as a turn: where it is along the route, and
// which way the route bends there (y grows downward, so a positive cross
// product bends right).
function corners(): Turn[] {
    const total = length()
    const out: Turn[] = []
    let gone = 0
    for (let i = 1; i < route.length - 1; i += 1) {
        const a = route[i - 1] ?? start
        const b = route[i] ?? start
        const c = route[i + 1] ?? start
        gone += span(a, b)
        const cross = (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x)
        const kind: Turn["kind"] = cross > 0 ? "right" : "left"
        out.push({
            kind,
            street: streets[i - 1] ?? destination,
            at: gone / total,
        })
    }
    return out
}

export const turns: readonly Turn[] = corners()

const arrival: Turn = { kind: "straight", street: destination, at: 1 }

// The next turn after `fraction` of the route.
export function nextTurn(fraction: number): Turn {
    return turns.find((t) => t.at > fraction) ?? arrival
}

// Metres to the next turn.
export function metresTo(turn: Turn, fraction: number): number {
    return Math.max(0, Math.round(((turn.at - fraction) * length() * 4.2) / 10) * 10)
}
