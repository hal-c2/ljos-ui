// The watch's music: invented tracks by invented artists.

export interface Track {
    title: string
    artist: string
    seconds: number
}

export const tracks: Track[] = [
    { title: "Glass Rain", artist: "Kōri Static", seconds: 214 },
    { title: "Night Shift Halo", artist: "Vanta Loop", seconds: 187 },
    { title: "Signal Under Ice", artist: "Nordlys Grid", seconds: 243 },
    { title: "Chrome Lullaby", artist: "Hex & Ember", seconds: 201 },
]

const first: Track = { title: "Glass Rain", artist: "Kōri Static", seconds: 214 }

export function at(i: number): Track {
    const n = tracks.length
    return tracks[((i % n) + n) % n] ?? first
}
