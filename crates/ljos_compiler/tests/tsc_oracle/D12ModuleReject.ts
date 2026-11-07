// A module returns a member it found with `in` but never checked.
export function title(v: unknown): string {
    if (typeof v === "object" && v !== null && "title" in v) { return v.title }
    return ""
}
