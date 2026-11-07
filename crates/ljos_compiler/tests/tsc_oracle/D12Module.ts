// A module decodes what it is given: an object's member, and each element of an array (an array is readonly, Q6).
export function title(v: unknown): string {
    if (typeof v === "object" && v !== null && "title" in v && typeof v.title === "string") { return v.title }
    return ""
}
export function titles(v: unknown): readonly string[] {
    if (!Array.isArray(v)) { return [] }
    return v.map(title)
}
