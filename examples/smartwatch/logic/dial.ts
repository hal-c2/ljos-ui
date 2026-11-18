// The geometry of the faces: where hands point, where ticks go.

export interface Hands {
    hour: number
    minute: number
    second: number
}

// Degrees clockwise from twelve.
export function hands(hours: number, minutes: number, seconds: number): Hands {
    return {
        hour: ((hours % 12) + minutes / 60) * 30,
        minute: (minutes + seconds / 60) * 6,
        second: seconds * 6,
    }
}

// Ticks round a circle: `count` of them, every `major`-th longer.
export function ticks(cx: number, cy: number, r: number, count: number, major: number, long: number, short: number): string {
    let d = ""
    for (let i = 0; i < count; i++) {
        const a = (i / count) * Math.PI * 2
        const len = i % major === 0 ? long : short
        const sx = cx + Math.sin(a) * r
        const sy = cy - Math.cos(a) * r
        const ex = cx + Math.sin(a) * (r - len)
        const ey = cy - Math.cos(a) * (r - len)
        d += "M " + sx.toFixed(1) + " " + sy.toFixed(1) + " L " + ex.toFixed(1) + " " + ey.toFixed(1) + " "
    }
    return d
}

export interface Spot {
    x: number
    y: number
}

// The place of the i-th of n round a ring of radius r, from twelve.
export function around(i: number, n: number, r: number): Spot {
    const a = (i / n) * Math.PI * 2
    return { x: Math.sin(a) * r, y: -Math.cos(a) * r }
}

// The mask that makes a screen round or a rounded square: everything of
// a w by h rectangle outside the shape, filled odd-even.
export function bezel(w: number, h: number, round: boolean, corner: number): string {
    const outer = "M 0 0 L " + w + " 0 L " + w + " " + h + " L 0 " + h + " Z "
    if (round) {
        const r = Math.min(w, h) / 2
        const cx = w / 2
        const cy = h / 2
        return outer + "M " + cx + " " + (cy - r) + " A " + r + " " + r + " 0 1 1 " + cx + " " + (cy + r)
            + " A " + r + " " + r + " 0 1 1 " + cx + " " + (cy - r) + " Z"
    }
    const c = corner
    return outer + "M " + c + " 0 L " + (w - c) + " 0 A " + c + " " + c + " 0 0 1 " + w + " " + c
        + " L " + w + " " + (h - c) + " A " + c + " " + c + " 0 0 1 " + (w - c) + " " + h
        + " L " + c + " " + h + " A " + c + " " + c + " 0 0 1 0 " + (h - c)
        + " L 0 " + c + " A " + c + " " + c + " 0 0 1 " + c + " 0 Z"
}
