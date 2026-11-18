// What the touchscreen names: the parts of the car that open, the pages
// of the app panel and the looks the theme can take.

import type { Glyph } from "./glyphs.ts"

export type Part = "doorLeft" | "doorRight" | "frunk" | "trunk" | "chargePort"
export type Page = "nav" | "media" | "climate" | "car" | "settings" | "apps"
export type Look = "dark" | "light" | "auto"

export const parts: readonly Part[] = ["doorLeft", "doorRight", "frunk", "trunk", "chargePort"]

// The pages the launcher offers, with the glyph each is drawn with.
export interface App {
    readonly page: Page
    readonly glyph: Glyph
}

export const apps: readonly App[] = [
    { page: "nav", glyph: "nav" },
    { page: "media", glyph: "media" },
    { page: "climate", glyph: "fan" },
    { page: "car", glyph: "car" },
    { page: "settings", glyph: "gear" },
    { page: "apps", glyph: "grid" },
]

// The 3D scene names each part's node after its file (rig.json); a tap on
// the car picks one of these.
export function partOfNode(name: string): Part | undefined {
    switch (name) {
        case "door_left": return "doorLeft"
        case "door_right": return "doorRight"
        case "frunk": return "frunk"
        case "trunk": return "trunk"
        case "charge_port": return "chargePort"
        default: return undefined
    }
}

// The node a part's file is named by: the inverse of `partOfNode`.
export function nodeOfPart(part: Part): string {
    switch (part) {
        case "doorLeft": return "door_left"
        case "doorRight": return "door_right"
        case "frunk": return "frunk"
        case "trunk": return "trunk"
        case "chargePort": return "charge_port"
    }
}
