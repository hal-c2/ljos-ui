// A simulated body: the same heart rate for the same second every run,
// so a test can count on it and a screenshot is the same each time.

export interface Rings {
    move: number
    exercise: number
    stand: number
}

// Beats a minute at the n-th sample: a resting pulse that breathes, or a
// working one that climbs over the first minute of a workout.
export function bpm(n: number, working: boolean, workSeconds: number): number {
    const rest = 64 + 5 * Math.sin(n / 4.1) + 2.5 * Math.sin(n / 1.3)
    if (!working) {
        return Math.round(rest)
    }
    const climb = Math.min(1, workSeconds / 60)
    return Math.round(rest + 78 * climb + 4 * Math.sin(n / 2.2))
}

// The last `keep` samples, oldest first, with one more.
export function push(history: readonly number[], value: number, keep: number): number[] {
    const next: number[] = []
    for (let i = Math.max(0, history.length - keep + 1); i < history.length; i++) {
        next.push(history[i] ?? value)
    }
    next.push(value)
    return next
}

// An SVG polyline through the samples, `w` by `h`, between lo and hi.
export function chart(history: readonly number[], w: number, h: number, lo: number, hi: number, keep: number): string {
    if (history.length === 0) {
        return ""
    }
    const step = w / Math.max(1, keep - 1)
    const x0 = w - step * (history.length - 1)
    let d = ""
    for (let i = 0; i < history.length; i++) {
        const f = ((history[i] ?? lo) - lo) / (hi - lo)
        const y = h - Math.max(0, Math.min(1, f)) * h
        d += (i === 0 ? "M " : " L ") + (x0 + i * step).toFixed(1) + " " + y.toFixed(1)
    }
    return d
}

// The day so far: steps, minutes of exercise, hours stood, each a share
// of its goal. A workout adds to the first two.
export function rings(steps: number, workSeconds: number): Rings {
    return {
        move: Math.min(1, (steps + workSeconds * 2.6) / 10000),
        exercise: Math.min(1, (22 + workSeconds / 60) / 30),
        stand: 7 / 12,
    }
}

// Kilometres and kilocalories of a run of that many seconds.
export function km(seconds: number): number {
    return seconds * 2.9 / 1000
}

export function kcal(seconds: number): number {
    return Math.round(seconds * 0.19)
}

// The minutes before the watch was put on: `keep` resting samples, so the
// chart is full from its first frame.
export function seed(keep: number): number[] {
    const out: number[] = []
    for (let i = 0; i < keep; i++) {
        out.push(bpm(i - keep, false, 0))
    }
    return out
}
