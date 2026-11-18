// What the watch was told while it was on a wrist. The words of each are
// the watch's to say, translated (`Watch.qml`, `noteTitle`, `noteBody`).
import type { Mark } from "./glyphs.ts"

export type Kind = "message" | "calendar" | "coach" | "battery"

export interface Note {
    id: number
    kind: Kind
    from: string
    glyph: Mark
    minutesAgo: number
}

export const all: readonly Note[] = [
    { id: 1, kind: "message", from: "Ásta", glyph: "message", minutesAgo: 2 },
    { id: 2, kind: "calendar", from: "Calendar", glyph: "calendar", minutesAgo: 9 },
    { id: 3, kind: "coach", from: "Coach", glyph: "run", minutesAgo: 34 },
    { id: 4, kind: "battery", from: "Watch", glyph: "bolt", minutesAgo: 61 },
]

export function unread(gone: readonly number[]): Note[] {
    const left: Note[] = []
    for (const n of all) {
        if (gone.indexOf(n.id) < 0) {
            left.push(n)
        }
    }
    return left
}

const fallback: Note = { id: 0, kind: "message", from: "", glyph: "", minutesAgo: 0 }

export function byId(id: number): Note {
    const found = all.filter((n: Note) => n.id === id)
    return found[0] ?? fallback
}
