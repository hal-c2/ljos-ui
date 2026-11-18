// The cabin's climate: set points kept in range, and how they read.

export const lowest = 16
export const highest = 28

export function stepped(celsius: number, step: number): number {
    return Math.max(lowest, Math.min(highest, Math.round((celsius + step) * 2) / 2))
}

export function shown(celsius: number, fahrenheit: boolean): string {
    if (fahrenheit) {
        return `${Math.round(celsius * 1.8 + 32)}°`
    }
    return `${celsius.toFixed(1)}°`
}

// 0 (cold) to 1 (hot), for the colour of a set point.
export function warmth(celsius: number): number {
    return (celsius - lowest) / (highest - lowest)
}

export const fanSteps = 6
