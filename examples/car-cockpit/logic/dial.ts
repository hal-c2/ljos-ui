// The geometry of a round gauge. Angles are in degrees, clockwise from
// three o'clock, as a PathAngleArc takes them.

// The tick marks of a gauge as one SVG path: `count` marks spread over
// the sweep, every `major`th of them longer.
export function ticks(
    cx: number, cy: number, radius: number, start: number, sweep: number,
    count: number, major: number,
): string {
    const parts: string[] = []
    for (let i = 0; i <= count; i += 1) {
        const a = ((start + (sweep * i) / count) * Math.PI) / 180
        const long = i % major === 0
        const inner = radius - (long ? 16 : 8)
        const x1 = cx + Math.cos(a) * inner
        const y1 = cy + Math.sin(a) * inner
        const x2 = cx + Math.cos(a) * radius
        const y2 = cy + Math.sin(a) * radius
        parts.push(`M ${x1.toFixed(1)} ${y1.toFixed(1)} L ${x2.toFixed(1)} ${y2.toFixed(1)}`)
    }
    return parts.join(" ")
}

// Where along the sweep `value` of `maximum` falls, 0 to 1.
export function fraction(value: number, maximum: number): number {
    return maximum > 0 ? Math.max(0, Math.min(1, value / maximum)) : 0
}

// The place of a label at `fraction` of the sweep, `radius` from the centre.
export interface Place {
    readonly x: number
    readonly y: number
}

export function place(cx: number, cy: number, radius: number, start: number, sweep: number, f: number): Place {
    const a = ((start + sweep * f) * Math.PI) / 180
    return { x: cx + Math.cos(a) * radius, y: cy + Math.sin(a) * radius }
}
