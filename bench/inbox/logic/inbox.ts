// The inbox's rules: what a record is, which records a view shows and in
// what order, what the chart counts and what the edit form refuses. Every
// function is pure; the QML holds the state, so a view that rests runs no
// code at all.

export type Status = "open" | "closed"
export type Sort = "updated" | "created" | "priority" | "title"
// In the order the sort selector lists them.
export const sorts: readonly Sort[] = ["updated", "created", "priority", "title"]
// One of the sidebar's rows: "inbox", "assigned", "starred", "closed", or
// "tag:<name>" for a label.
export type Filter = string

export interface Issue {
    readonly id: number
    readonly key: string
    readonly title: string
    readonly author: string
    readonly avatar: string
    readonly assignee: string
    readonly status: Status
    readonly priority: number
    readonly tags: readonly string[]
    readonly starred: boolean
    readonly comments: number
    // Milliseconds since the epoch: sorting compares numbers, not text.
    readonly created: number
    readonly updated: number
    readonly body: string
    // The title, the author and the tags in lower case, which a search
    // looks through; made once, not on every key.
    readonly haystack: string
}

// The fixture's "now" (SPEC §1): never the wall clock, so relative dates and
// the chart are the same on every run.
// 2026-11-27T12:00:00Z.
export const now = 1795780800000
export const me = "Ada Okafor"

const DAY = 24 * 60 * 60 * 1000

function text(v: unknown): string {
    return typeof v === "string" ? v : ""
}

function count(v: unknown): number {
    return typeof v === "number" ? v : 0
}

// Days since 1970-01-01 of a date in the proleptic Gregorian calendar
// (Howard Hinnant's days_from_civil), and back: the toolkit's scripts have
// no Date, and the fixture's dates are UTC.
function daysFrom(y: number, m: number, d: number): number {
    const yy = m <= 2 ? y - 1 : y
    const era = Math.floor(yy / 400)
    const yoe = yy - era * 400
    const doy = Math.floor((153 * (m + (m > 2 ? -3 : 9)) + 2) / 5) + d - 1
    const doe = yoe * 365 + Math.floor(yoe / 4) - Math.floor(yoe / 100) + doy
    return era * 146097 + doe - 719468
}

// "2026-08-12T15:32:00Z" as milliseconds since the epoch; 0 for anything else.
function instant(v: unknown): number {
    if (typeof v !== "string" || !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/.test(v)) {
        return 0
    }
    const iso: string = v
    const n = (from: number, to: number): number => Number(iso.slice(from, to))
    return daysFrom(n(0, 4), n(5, 7), n(8, 10)) * DAY + ((n(11, 13) * 60 + n(14, 16)) * 60 + n(17, 19)) * 1000
}

function words(v: unknown): string[] {
    const out: string[] = []
    if (Array.isArray(v)) {
        for (const w of v) {
            if (typeof w === "string") {
                out.push(w)
            }
        }
    }
    return out
}

function haystack(title: string, author: string, tags: readonly string[]): string {
    return `${title} ${author} ${tags.join(" ")}`.toLowerCase()
}

function make(
    id: number, title: string, author: string, avatar: string, assignee: string, status: Status,
    priority: number, tags: readonly string[], starred: boolean, comments: number,
    created: number, updated: number, body: string,
): Issue {
    return {
        id, key: `INB-${id}`, title, author, avatar, assignee, status, priority, tags, starred,
        comments, created, updated, body, haystack: haystack(title, author, tags),
    }
}

// A row of the JSON as the model gives it, checked field by field: a
// document that is not the fixture shows what it has rather than failing.
export function decode(r: { readonly [field: string]: unknown }): Issue {
    return make(
        count(r.id), text(r.title), text(r.author), text(r.avatar), text(r.assignee),
        r.status === "closed" ? "closed" : "open", count(r.priority), words(r.tags),
        r.starred === true, count(r.comments), instant(r.created), instant(r.updated), text(r.body),
    )
}

function byUpdated(a: Issue, b: Issue): number {
    return b.updated - a.updated || b.id - a.id
}

// Every record in the order `sort` puts them. A view filters this, so the
// order is worked out when the records or the sort change, never per key.
export function sorted(all: readonly Issue[], sort: Sort): readonly Issue[] {
    const out: Issue[] = [...all]
    switch (sort) {
        case "updated":
            out.sort(byUpdated)
            break
        case "created":
            out.sort((a, b) => b.created - a.created || b.id - a.id)
            break
        case "priority":
            out.sort((a, b) => a.priority - b.priority || byUpdated(a, b))
            break
        case "title":
            out.sort((a, b) => a.title.localeCompare(b.title) || a.id - b.id)
            break
    }
    return out
}

function admits(filter: Filter, issue: Issue): boolean {
    switch (filter) {
        case "inbox":
            return issue.status === "open"
        case "assigned":
            return issue.status === "open" && issue.assignee === me
        case "starred":
            return issue.starred
        case "closed":
            return issue.status === "closed"
        default:
            return issue.status === "open" && issue.tags.includes(filter.slice(4))
    }
}

// The words of a search, each of which must be in a record (AND).
export function terms(query: string): readonly string[] {
    return query.toLowerCase().split(/\s+/).filter((w) => w !== "")
}

// A row of the list: the record's id and where it is in the records shown.
// A view copies each row of its model into a model of its own, field by
// field, whenever the list is assigned (2 µs a row for these two fields,
// 11 µs for a whole record), so the list carries what tells rows apart and
// each row on screen reads its record from `Shown.issues`.
export interface Key {
    readonly id: number
    readonly at: number
}

export interface Shown {
    readonly issues: readonly Issue[]
    readonly keys: readonly Key[]
}

// What the list shows: the records of `ordered` the filter admits and every
// word of the search is found in, made in one pass.
export function shown(ordered: readonly Issue[], filter: Filter, query: string): Shown {
    const ws = terms(query)
    const issues: Issue[] = []
    const keys: Key[] = []
    for (const issue of ordered) {
        if (admits(filter, issue) && ws.every((w) => issue.haystack.includes(w))) {
            keys.push({ id: issue.id, at: issues.length })
            issues.push(issue)
        }
    }
    return { issues, keys }
}

export function indexOf(rows: readonly Key[], id: number): number {
    for (let i = 0; i < rows.length; i += 1) {
        if (rows[i]?.id === id) {
            return i
        }
    }
    return -1
}

// What the detail pane shows with nothing selected: a record no list has.
export const none: Issue = make(0, "", "", "", "", "open", 2, [], false, 0, 0, 0, "")

export function pick(all: readonly Issue[], id: number): Issue {
    for (const r of all) {
        if (r.id === id) {
            return r
        }
    }
    return none
}

// The record a row of the list stands for: where its key says, or, while
// the keys and the records are between two versions, found by its id.
export function at(shown: Shown, key: Key, all: readonly Issue[]): Issue {
    const r = shown.issues[key.at]
    return r !== undefined && r.id === key.id ? r : pick(all, key.id)
}

// The labels the sidebar lists and the people the assignee selector offers,
// from the records themselves.
export function tagsOf(all: readonly Issue[]): readonly string[] {
    const seen: string[] = []
    for (const issue of all) {
        for (const tag of issue.tags) {
            if (!seen.includes(tag)) {
                seen.push(tag)
            }
        }
    }
    return seen.sort()
}

export function peopleOf(all: readonly Issue[]): readonly string[] {
    const seen: string[] = []
    for (const issue of all) {
        if (!seen.includes(issue.author)) {
            seen.push(issue.author)
        }
        if (issue.assignee !== "" && !seen.includes(issue.assignee)) {
            seen.push(issue.assignee)
        }
    }
    return seen.sort((a, b) => a.localeCompare(b))
}

// How long ago `when` was, in the unit it is told in.
export interface Ago {
    readonly unit: "now" | "hours" | "days"
    readonly n: number
}

export function ago(when: number): Ago {
    const minutes = Math.floor((now - when) / 60000)
    if (minutes < 60) {
        return { unit: "now", n: 0 }
    }
    if (minutes < 24 * 60) {
        return { unit: "hours", n: Math.floor(minutes / 60) }
    }
    return { unit: "days", n: Math.floor(minutes / (24 * 60)) }
}

export interface Day {
    readonly year: number
    readonly month: number
    readonly day: number
}

export function dayOf(when: number): Day {
    // civil_from_days, the inverse of daysFrom.
    const z = Math.floor(when / DAY) + 719468
    const era = Math.floor(z / 146097)
    const doe = z - era * 146097
    const yoe = Math.floor((doe - Math.floor(doe / 1460) + Math.floor(doe / 36524) - Math.floor(doe / 146096)) / 365)
    const doy = doe - (365 * yoe + Math.floor(yoe / 4) - Math.floor(yoe / 100))
    const mp = Math.floor((5 * doy + 2) / 153)
    const day = doy - Math.floor((153 * mp + 2) / 5) + 1
    const month = mp < 10 ? mp + 3 : mp - 9
    return { year: yoe + era * 400 + (month <= 2 ? 1 : 0), month, day }
}

// The chart (SPEC §4): records created in each ISO week (from Monday) of the
// twelve ending with the week of `now`, oldest first.
export interface Week {
    readonly start: number
    readonly count: number
}

export function weekly(rows: readonly Issue[]): readonly Week[] {
    const today = Math.floor(now / DAY) * DAY
    // 1970-01-01 was a Thursday: (days + 3) % 7 is the days since Monday.
    const monday = today - (((Math.floor(today / DAY) + 3) % 7) * DAY)
    const first = monday - 11 * 7 * DAY
    const counts: number[] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    for (const r of rows) {
        if (r.created >= first && r.created <= now) {
            counts[Math.floor((r.created - first) / (7 * DAY))] += 1
        }
    }
    const out: Week[] = []
    for (let i = 0; i < 12; i += 1) {
        out.push({ start: first + i * 7 * DAY, count: counts[i] ?? 0 })
    }
    return out
}

// The top of the chart's scale: the largest count rounded up to 1, 2 or 5
// times a power of ten.
export function niceMax(weeks: readonly Week[]): number {
    let top = 0
    for (const w of weeks) {
        top = Math.max(top, w.count)
    }
    if (top <= 5) {
        return 5
    }
    const p = Math.pow(10, Math.floor(Math.log10(top)))
    for (const m of [1, 2, 5, 10]) {
        if (m * p >= top) {
            return m * p
        }
    }
    return 10 * p
}

export interface Tally {
    readonly open: number
    readonly closed: number
    readonly byPriority: readonly number[]
}

export function tally(rows: readonly Issue[]): Tally {
    let open = 0
    const byPriority: number[] = [0, 0, 0, 0]
    for (const r of rows) {
        if (r.status === "open") {
            open += 1
        }
        if (r.priority >= 0 && r.priority < 4) {
            byPriority[r.priority] += 1
        }
    }
    return { open, closed: rows.length - open, byPriority }
}

// The edit form's rules (SPEC §5). Each field's answer is the message's
// name, "" when it passes; the QML says it in the UI's language.
export type TitleError = "" | "required" | "long"
export type TagsError = "" | "words" | "many"
export type BodyError = "" | "long"

export const titleLimit = 120
export const bodyLimit = 10000
export const tagLimit = 6

export function titleError(title: string): TitleError {
    if (title.trim() === "") {
        return "required"
    }
    return title.length > titleLimit ? "long" : ""
}

export function tagWords(tags: string): readonly string[] {
    return tags.split(/\s+/).filter((w) => w !== "")
}

export function tagsError(tags: string): TagsError {
    const ws = tagWords(tags)
    if (!ws.every((w) => /^[a-z0-9-]+$/.test(w))) {
        return "words"
    }
    return ws.length > tagLimit ? "many" : ""
}

export function bodyError(body: string): BodyError {
    return body.length > bodyLimit ? "long" : ""
}

// The records after an edit is saved: the record's fields replaced and its
// `updated` set to now.
export function saved(
    all: readonly Issue[], id: number, title: string, tags: string, priority: number,
    assignee: string, body: string,
): readonly Issue[] {
    const ws: string[] = []
    for (const w of tagWords(tags)) {
        if (!ws.includes(w)) {
            ws.push(w)
        }
    }
    ws.sort()
    const out: Issue[] = []
    for (const r of all) {
        out.push(r.id !== id ? r : make(
            r.id, title, r.author, r.avatar, assignee, r.status, priority, ws, r.starred, r.comments,
            r.created, now, body,
        ))
    }
    return out
}

export function withStatus(all: readonly Issue[], id: number, status: Status): readonly Issue[] {
    const out: Issue[] = []
    for (const r of all) {
        out.push(r.id !== id ? r : make(
            r.id, r.title, r.author, r.avatar, r.assignee, status, r.priority, r.tags, r.starred,
            r.comments, r.created, now, r.body,
        ))
    }
    return out
}

export function nextId(all: readonly Issue[]): number {
    let top = 0
    for (const r of all) {
        top = Math.max(top, r.id)
    }
    return top + 1
}

// A new record (SPEC §5): by me, open, Normal, created now, with the
// avatar my other records have.
export function created(all: readonly Issue[], title: string): readonly Issue[] {
    const id = nextId(all)
    const mine = all.find((r) => r.author === me)
    const avatar = mine === undefined ? "avatars/00.png" : mine.avatar
    return [...all, make(id, title, me, avatar, "", "open", 2, [], false, 0, now, now, "")]
}

// The record a removal selects next: the one after `id` in `rows`, or the
// one before it at the end.
export function after(rows: readonly Key[], id: number): number {
    const i = indexOf(rows, id)
    if (i < 0 || rows.length < 2) {
        return -1
    }
    return rows[i + 1 < rows.length ? i + 1 : i - 1]?.id ?? -1
}

export function clamp(i: number, length: number): number {
    return Math.max(0, Math.min(length - 1, i))
}
