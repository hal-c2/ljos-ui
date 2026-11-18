// The media player's catalogue and its visualiser. The tracks are made up
// for this demo: no title or artist here is a real recording.

export interface Track {
    readonly title: string
    readonly artist: string
    readonly seconds: number
    // The hue of the track's cover art, 0 to 1.
    readonly hue: number
}

export const tracks: readonly Track[] = [
    { title: "Halogen Tide", artist: "The Night Shift Ensemble", seconds: 214, hue: 0.52 },
    { title: "Lumen Drift", artist: "Orra Vey", seconds: 187, hue: 0.83 },
    { title: "Glass Avenue", artist: "Kilowatt Choir", seconds: 241, hue: 0.12 },
    { title: "Signal Fires", artist: "Pale Circuit", seconds: 199, hue: 0.62 },
    { title: "Static Bloom", artist: "Orra Vey", seconds: 226, hue: 0.95 },
    { title: "Northbound 9", artist: "The Night Shift Ensemble", seconds: 263, hue: 0.42 },
]

const silence: Track = { title: "", artist: "", seconds: 1, hue: 0.5 }

export function trackAt(index: number): Track {
    return tracks[index] ?? silence
}

export function next(index: number, step: number): number {
    const count = tracks.length
    return (((index + step) % count) + count) % count
}

// m:ss
export function clock(seconds: number): string {
    const whole = Math.max(0, Math.floor(seconds))
    const s = whole % 60
    return `${Math.floor(whole / 60)}:${s < 10 ? "0" : ""}${s}`
}

// The height of bar `bar` of `bars` at `phase` (0 to 1 round a loop), 0
// to 1: a few sines, each a whole number of turns per loop so the loop
// joins up, at rates that differ bar to bar so the bars look alive
// without a sound to read. Bass on the left, air on the right, as an
// analyser shows it.
export function level(bar: number, bars: number, phase: number, energy: number): number {
    const x = bar / Math.max(1, bars - 1)
    const turn = phase * Math.PI * 2
    const body = 0.55 - x * 0.3
    const wave =
        Math.sin(turn * (3 + (bar % 4)) + bar * 1.7) * 0.22 +
        Math.sin(turn * (7 - (bar % 3)) + bar * 0.9) * 0.14 +
        Math.sin(turn * 2 + bar * 2.3) * 0.1
    return Math.max(0.04, Math.min(1, (body + wave) * energy))
}
