// The watch's screens and the rules that move between them.
import type { Mark } from "./glyphs.ts"

export type Page =
    | "face" | "notifications" | "notification" | "health" | "workout"
    | "timer" | "stopwatch" | "music" | "settings" | "launcher"

export type Workout = "idle" | "running" | "paused" | "done"
export type Countdown = "set" | "running" | "paused" | "ringing"
export type Swipe = "up" | "down" | "left" | "right" | "none"

// The launcher's apps, round the ring from twelve o'clock, the last in
// the middle. The crown walks them in this order.
export interface App {
    page: Page
    glyph: Mark
}

export const apps: App[] = [
    { page: "health", glyph: "heart" },
    { page: "workout", glyph: "run" },
    { page: "timer", glyph: "timer" },
    { page: "stopwatch", glyph: "stopwatch" },
    { page: "music", glyph: "music" },
    { page: "settings", glyph: "gear" },
    { page: "notifications", glyph: "bell" },
]

// How far a finger must travel, in pixels, before a drag is a swipe; the
// longer way it went decides which.
export function swipe(dx: number, dy: number): Swipe {
    const ax = Math.abs(dx)
    const ay = Math.abs(dy)
    if (Math.max(ax, ay) < 48) {
        return "none"
    }
    if (ax > ay) {
        return dx < 0 ? "left" : "right"
    }
    return dy < 0 ? "up" : "down"
}

// Where a swipe goes from a page. A swipe to the right is back.
export function swipeTo(from: Page, way: Swipe): Page {
    if (way === "none") {
        return from
    }
    if (from === "face") {
        return way === "up" ? "notifications" : way === "down" ? "settings" : way === "left" ? "launcher" : "face"
    }
    if (way === "right") {
        return back(from)
    }
    if (from === "notifications" && way === "down") {
        return "face"
    }
    if (from === "settings" && way === "up") {
        return "face"
    }
    return from
}

export function back(from: Page): Page {
    return from === "notification" ? "notifications" : "face"
}

export function clamp(v: number, lo: number, hi: number): number {
    return Math.max(lo, Math.min(hi, v))
}

function two(n: number): string {
    return n < 10 ? "0" + n : "" + n
}

// "1:02:03" or "02:03" for a span of milliseconds.
export function span(ms: number): string {
    const s = Math.floor(Math.max(0, ms) / 1000)
    const h = Math.floor(s / 3600)
    const m = Math.floor((s % 3600) / 60)
    return (h > 0 ? h + ":" + two(m) : two(m)) + ":" + two(s % 60)
}

// "02:03.4": a stopwatch's span, to a tenth.
export function tenths(ms: number): string {
    const t = Math.floor(Math.max(0, ms) / 100)
    return span(t * 100) + "." + (t % 10)
}

// "9:41" as the hours and minutes of a day.
export function hm(hours: number, minutes: number): string {
    return hours + ":" + two(minutes)
}

export function pad(n: number): string {
    return two(n)
}

// The last `n` laps, the newest first.
export function lastLaps(laps: readonly number[], n: number): number[] {
    const out: number[] = []
    for (let i = laps.length - 1; i >= 0 && out.length < n; i--) {
        out.push(laps[i] ?? 0)
    }
    return out
}
