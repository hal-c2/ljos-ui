// The cockpit's icons, drawn for it: SVG paths on a 24 by 24 grid,
// stroked, not filled, so one colour and one weight make them all.

export type Glyph =
    | "left" | "right" | "lamp" | "door" | "battery" | "belt" | "park"
    | "play" | "pause" | "prev" | "next" | "nav" | "media" | "fan" | "car"
    | "gear" | "snow" | "flame" | "defrost" | "lock" | "unlock"
    | "bolt" | "plus" | "minus" | "volume" | "seat" | "link" | "power"
    | "straight" | "assist"

// A glyph or none: a control with only a label draws no icon.
export type Mark = Glyph | ""

export function path(glyph: Mark): string {
    switch (glyph) {
        case "": return ""
        case "left": return "M 10 5 L 3 12 L 10 19 M 3 12 L 21 12"
        case "right": return "M 14 5 L 21 12 L 14 19 M 21 12 L 3 12"
        case "straight": return "M 5 10 L 12 3 L 19 10 M 12 3 L 12 21"
        case "lamp": return "M 12 6 C 7 6 5 9 5 12 C 5 15 7 18 12 18 Z M 15 8 L 21 7 M 15 12 L 21 12 M 15 16 L 21 17"
        case "door": return "M 6 21 L 6 6 L 12 3 L 18 3 L 18 21 Z M 9 13 L 11 13 M 6 21 L 18 21"
        case "battery": return "M 3 8 L 19 8 L 19 16 L 3 16 Z M 19 10.5 L 21 10.5 L 21 13.5 L 19 13.5 M 6 11 L 6 13 M 9 11 L 9 13"
        case "belt": return "M 12 3 A 2 2 0 1 1 12 7 A 2 2 0 1 1 12 3 M 8 21 L 9 10 L 15 10 L 16 21 M 7 9 L 17 19"
        case "park": return "M 12 2 A 10 10 0 1 1 12 22 A 10 10 0 1 1 12 2 M 9 17 L 9 7 L 13 7 A 3 3 0 0 1 13 13 L 9 13"
        case "play": return "M 7 4 L 20 12 L 7 20 Z"
        case "pause": return "M 7 4 L 7 20 M 17 4 L 17 20"
        case "prev": return "M 5 4 L 5 20 M 19 4 L 8 12 L 19 20 Z"
        case "next": return "M 19 4 L 19 20 M 5 4 L 16 12 L 5 20 Z"
        case "nav": return "M 12 2 L 20 21 L 12 16 L 4 21 Z"
        case "media": return "M 9 18 A 3 3 0 1 1 9 17.9 M 12 18 L 12 4 L 20 2 L 20 15 A 3 3 0 1 1 20 14.9"
        case "fan": return "M 12 12 C 12 6 15 3 18 5 C 20 7 16 11 12 12 C 18 12 21 15 19 18 C 17 20 13 16 12 12 C 12 18 9 21 6 19 C 4 17 8 13 12 12 C 6 12 3 9 5 6 C 7 4 11 8 12 12"
        case "car": return "M 3 15 L 5 9 C 6 7 7 6 9 6 L 15 6 C 17 6 18 7 19 9 L 21 15 L 21 18 L 3 18 Z M 3 13 L 21 13 M 6 18 L 6 20 M 18 18 L 18 20"
        case "gear": return "M 12 8 A 4 4 0 1 1 12 16 A 4 4 0 1 1 12 8 M 12 2 L 12 5 M 12 19 L 12 22 M 2 12 L 5 12 M 19 12 L 22 12 M 5 5 L 7 7 M 17 17 L 19 19 M 5 19 L 7 17 M 17 7 L 19 5"
        case "snow": return "M 12 2 L 12 22 M 3.5 7 L 20.5 17 M 3.5 17 L 20.5 7 M 9 3.5 L 12 6 L 15 3.5 M 9 20.5 L 12 18 L 15 20.5"
        case "flame": return "M 12 22 C 7 22 5 18 6 14 C 7 10 11 9 11 3 C 15 6 18 10 18 15 C 18 19 16 22 12 22 Z M 12 22 C 10 22 9 20 10 17 C 11 15 12 15 12 13 C 14 15 15 17 14 19"
        case "defrost": return "M 3 16 C 5 6 19 6 21 16 Z M 8 20 C 7 19 9 18 8 17 M 12 20 C 11 19 13 18 12 17 M 16 20 C 15 19 17 18 16 17"
        case "lock": return "M 5 11 L 19 11 L 19 21 L 5 21 Z M 8 11 L 8 7 A 4 4 0 0 1 16 7 L 16 11"
        case "unlock": return "M 5 11 L 19 11 L 19 21 L 5 21 Z M 8 11 L 8 7 A 4 4 0 0 1 15.5 5"
        case "bolt": return "M 13 2 L 5 13 L 11 13 L 10 22 L 19 10 L 13 10 Z"
        case "plus": return "M 12 5 L 12 19 M 5 12 L 19 12"
        case "minus": return "M 5 12 L 19 12"
        case "volume": return "M 4 9 L 8 9 L 13 5 L 13 19 L 8 15 L 4 15 Z M 16 9 C 17.5 10.5 17.5 13.5 16 15 M 18.5 6.5 C 21.5 9.5 21.5 14.5 18.5 17.5"
        case "seat": return "M 8 3 L 10 13 L 18 13 L 18 16 L 9 16 L 6 4 Z M 9 16 L 8 21 M 16 16 L 17 21"
        case "link": return "M 10 14 L 14 10 M 8 11 L 5 14 A 3 3 0 0 0 10 19 L 13 16 M 16 13 L 19 10 A 3 3 0 0 0 14 5 L 11 8"
        case "power": return "M 12 3 L 12 12 M 7 6 A 8 8 0 1 0 17 6"
        case "assist": return "M 8 21 L 10 3 M 16 21 L 14 3 M 12 19 L 12 16 M 12 12 L 12 9 M 12 6 L 12 4"
    }
}
