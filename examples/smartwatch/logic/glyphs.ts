// The watch's icons, drawn for it: SVG paths on a 24 by 24 grid, stroked,
// not filled, so one colour and one weight make them all.

export type Glyph =
    | "heart" | "run" | "timer" | "stopwatch" | "music" | "bell" | "gear"
    | "moon" | "sun" | "spark" | "play" | "pause" | "prev" | "next"
    | "check" | "close" | "plus" | "minus" | "bolt" | "flag" | "message"
    | "calendar" | "volume" | "back"

// A glyph or none: a control with only a label draws no icon.
export type Mark = Glyph | ""

export function path(glyph: Mark): string {
    switch (glyph) {
        case "": return ""
        case "heart": return "M 12 20 C 6 16 3 13 3 9 C 3 6 5 4 7.5 4 C 9.5 4 11 5.2 12 7 C 13 5.2 14.5 4 16.5 4 C 19 4 21 6 21 9 C 21 13 18 16 12 20 Z"
        case "run": return "M 14 3.5 A 1.6 1.6 0 1 1 14 6.7 A 1.6 1.6 0 1 1 14 3.5 M 6 21 L 9.5 15 L 13 17 L 14 12 M 9 10 L 12.5 8.5 L 15 11 L 19 12 M 14 12 L 11 11 M 13 17 L 16 21"
        case "timer": return "M 12 7 A 7 7 0 1 1 12 21 A 7 7 0 1 1 12 7 M 12 14 L 15 11 M 10 2.5 L 14 2.5 M 12 2.5 L 12 7"
        case "stopwatch": return "M 12 6 A 7.5 7.5 0 1 1 12 21 A 7.5 7.5 0 1 1 12 6 M 12 13.5 L 12 9.5 M 12 3 L 12 6 M 18 6 L 19.5 7.5"
        case "music": return "M 9 18 A 3 3 0 1 1 9 17.9 M 12 18 L 12 4 L 20 2 L 20 15 A 3 3 0 1 1 20 14.9"
        case "bell": return "M 6 17 L 6 11 A 6 6 0 0 1 18 11 L 18 17 L 20 19 L 4 19 Z M 10 21.5 L 14 21.5"
        case "gear": return "M 12 8 A 4 4 0 1 1 12 16 A 4 4 0 1 1 12 8 M 12 2 L 12 5 M 12 19 L 12 22 M 2 12 L 5 12 M 19 12 L 22 12 M 5 5 L 7 7 M 17 17 L 19 19 M 5 19 L 7 17 M 17 7 L 19 5"
        case "moon": return "M 20 14.5 A 8.5 8.5 0 1 1 9.5 4 A 7 7 0 0 0 20 14.5 Z"
        case "sun": return "M 12 8 A 4 4 0 1 1 12 16 A 4 4 0 1 1 12 8 M 12 2 L 12 4 M 12 20 L 12 22 M 2 12 L 4 12 M 20 12 L 22 12 M 4.9 4.9 L 6.3 6.3 M 17.7 17.7 L 19.1 19.1 M 4.9 19.1 L 6.3 17.7 M 17.7 6.3 L 19.1 4.9"
        case "spark": return "M 12 2 L 14 10 L 22 12 L 14 14 L 12 22 L 10 14 L 2 12 L 10 10 Z"
        case "play": return "M 7 4 L 20 12 L 7 20 Z"
        case "pause": return "M 7 4 L 7 20 M 17 4 L 17 20"
        case "prev": return "M 5 4 L 5 20 M 19 4 L 8 12 L 19 20 Z"
        case "next": return "M 19 4 L 19 20 M 5 4 L 16 12 L 5 20 Z"
        case "check": return "M 4 12.5 L 9.5 18 L 20 6"
        case "close": return "M 5 5 L 19 19 M 19 5 L 5 19"
        case "plus": return "M 12 4 L 12 20 M 4 12 L 20 12"
        case "minus": return "M 4 12 L 20 12"
        case "bolt": return "M 13 2 L 5 13 L 11 13 L 10 22 L 19 10 L 13 10 Z"
        case "flag": return "M 5 22 L 5 3 M 5 4 L 18 4 L 15 8.5 L 18 13 L 5 13"
        case "message": return "M 3 5 L 21 5 L 21 17 L 9 17 L 4 21 L 4 17 L 3 17 Z"
        case "calendar": return "M 3 5 L 21 5 L 21 21 L 3 21 Z M 3 10 L 21 10 M 8 2.5 L 8 7 M 16 2.5 L 16 7"
        case "volume": return "M 3 9 L 7 9 L 12 4 L 12 20 L 7 15 L 3 15 Z M 16 8.5 C 17.5 10 17.5 14 16 15.5 M 19 5.5 C 22 9 22 15 19 18.5"
        case "back": return "M 10 5 L 3 12 L 10 19 M 3 12 L 21 12"
    }
}
