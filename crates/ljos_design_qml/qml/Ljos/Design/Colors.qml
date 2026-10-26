// Written by ljos_design (crates/ljos_design/src/module.rs); `ljos design export` writes it again.
pragma Singleton
import QtQuick

// The palette's colours by what they are for, light or dark as the
// platform's colour scheme is, and with the contrast a user asks for.
// Every text, mark and focus ring passes WCAG against the grounds it
// is meant for: 4.5:1 for text (7:1 in high contrast), 3:1 for the rest.
QtObject {
    // The OKLCH hue the palette grows from, in degrees: the nearest
    // palette of one every 30° is used.
    property real seedHue: 240
    property bool dark: Application.styleHints.colorScheme === Qt.ColorScheme.Dark
    property bool highContrast: Qt.styleHints.accessibility !== undefined && Qt.styleHints.accessibility.contrastPreference === 1

    // One entry for each seed, a list of the roles below for each variant.
    readonly property var table: [
        { // hue 0
            "light": ["#faf9f9", "#fdfdfd", "#ffffff", "#edeaeb", "#e5e2e3", "#ffe3ea", "#948f90", "#777173", "#c1587d", "#bc5378", "#ab446a", "#9a355c", "#ffffff", "#292526", "#655e60", "#867e80", "#953f5e", "#c5514c", "#ffffff", "#9b6e00", "#ffffff", "#05893e", "#ffffff", "#1879ca", "#ffffff", "#52000000", "#29000000"],
            "dark": ["#0c0c0c", "#131213", "#1c1b1c", "#242223", "#2c292a", "#301c22", "#6b6667", "#898384", "#c1587d", "#bc5378", "#ab446a", "#9a355c", "#ffffff", "#f0ebec", "#c0b8ba", "#867e80", "#fa99b7", "#c5514c", "#ffffff", "#9b6e00", "#ffffff", "#05893e", "#ffffff", "#1879ca", "#ffffff", "#99000000", "#80000000"],
            "lightHigh": ["#faf9f9", "#fdfdfd", "#ffffff", "#edeaeb", "#e5e2e3", "#ffe3ea", "#948f90", "#777173", "#c1587d", "#99335a", "#88234d", "#78113f", "#ffffff", "#292526", "#292526", "#867e80", "#8d3757", "#a12f2f", "#ffffff", "#755200", "#ffffff", "#00672c", "#ffffff", "#005b9f", "#ffffff", "#52000000", "#29000000"],
            "darkHigh": ["#0c0c0c", "#131213", "#1c1b1c", "#242223", "#2c292a", "#301c22", "#6b6667", "#898384", "#c1587d", "#ec7ea2", "#fd8eb2", "#ffa9c3", "#250d15", "#f0ebec", "#f0ebec", "#867e80", "#fa99b7", "#f67d75", "#270d0b", "#d39700", "#201300", "#4cb86a", "#051c0a", "#4ea6fb", "#051729", "#99000000", "#80000000"]
        },
        { // hue 30
            "light": ["#faf9f9", "#fdfdfc", "#ffffff", "#edeaea", "#e5e2e1", "#ffe4df", "#948f8e", "#777170", "#c65b4c", "#bf5546", "#ae4538", "#9d3629", "#ffffff", "#292625", "#665e5d", "#877e7c", "#994135", "#c5514c", "#ffffff", "#9b6e00", "#ffffff", "#05893e", "#ffffff", "#1879ca", "#ffffff", "#52000000", "#29000000"],
            "dark": ["#0c0c0c", "#131212", "#1d1b1b", "#242222", "#2c2929", "#311d19", "#6b6665", "#888280", "#c65b4c", "#bf5546", "#ae4538", "#9d3629", "#ffffff", "#f0eceb", "#c1b8b7", "#877e7c", "#fe9c8c", "#c5514c", "#ffffff", "#9b6e00", "#ffffff", "#05893e", "#ffffff", "#1879ca", "#ffffff", "#99000000", "#80000000"],
            "lightHigh": ["#faf9f9", "#fdfdfc", "#ffffff", "#edeaea", "#e5e2e1", "#ffe4df", "#948f8e", "#777170", "#c65b4c", "#9d3629", "#8d261b", "#7c140a", "#ffffff", "#292625", "#292625", "#877e7c", "#913a2e", "#a12f2f", "#ffffff", "#755200", "#ffffff", "#00672c", "#ffffff", "#005b9f", "#ffffff", "#52000000", "#29000000"],
            "darkHigh": ["#0c0c0c", "#131212", "#1d1b1b", "#242222", "#2c2929", "#311d19", "#6b6665", "#888280", "#c65b4c", "#f18170", "#ff9483", "#ffaea0", "#260e0a", "#f0eceb", "#f0eceb", "#877e7c", "#fe9c8c", "#f67d75", "#270d0b", "#d39700", "#201300", "#4cb86a", "#051c0a", "#4ea6fb", "#051729", "#99000000", "#80000000"]
        },
        { // hue 60
            "light": ["#f9f9f8", "#fdfdfc", "#ffffff", "#ecebe9", "#e5e2e0", "#fde7d5", "#93908d", "#76726f", "#bb6802", "#b16200", "#9c5600", "#884a00", "#ffffff", "#282624", "#655f5b", "#857f7a", "#8e4e00", "#c5514c", "#ffffff", "#9b6e00", "#ffffff", "#05893e", "#ffffff", "#1879ca", "#ffffff", "#52000000", "#29000000"],
            "dark": ["#0c0c0c", "#131312", "#1c1c1b", "#242221", "#2b2a28", "#2f1f12", "#6a6764", "#888480", "#bb6802", "#b16200", "#9c5600", "#884a00", "#ffffff", "#efece9", "#bfb9b4", "#857f7a", "#f3a767", "#c5514c", "#ffffff", "#9b6e00", "#ffffff", "#05893e", "#ffffff", "#1879ca", "#ffffff", "#99000000", "#80000000"],
            "lightHigh": ["#f9f9f8", "#fdfdfc", "#ffffff", "#ecebe9", "#e5e2e0", "#fde7d5", "#93908d", "#76726f", "#bb6802", "#864900", "#733e00", "#603200", "#ffffff", "#282624", "#282624", "#857f7a", "#814500", "#a12f2f", "#ffffff", "#755200", "#ffffff", "#00672c", "#ffffff", "#005b9f", "#ffffff", "#52000000", "#29000000"],
            "darkHigh": ["#0c0c0c", "#131312", "#1c1c1b", "#242221", "#2b2a28", "#2f1f12", "#6a6764", "#888480", "#bb6802", "#e38d3d", "#f59d4f", "#ffb06e", "#241102", "#efece9", "#efece9", "#857f7a", "#f3a767", "#f67d75", "#270d0b", "#d39700", "#201300", "#4cb86a", "#051c0a", "#4ea6fb", "#051729", "#99000000", "#80000000"]
        },
        { // hue 90
            "light": ["#f9f9f8", "#fdfdfc", "#ffffff", "#ecebe9", "#e4e3e0", "#f4ebd2", "#92908d", "#73716d", "#9d7c00", "#907200", "#7f6400", "#6e5600", "#ffffff", "#272624", "#62605a", "#838079", "#765d00", "#c5514c", "#ffffff", "#9b6e00", "#ffffff", "#05893e", "#ffffff", "#1879ca", "#ffffff", "#52000000", "#29000000"],
            "dark": ["#0c0c0c", "#131312", "#1c1c1b", "#232321", "#2b2a28", "#29220f", "#696764", "#85837e", "#9d7c00", "#907200", "#7f6400", "#6e5600", "#ffffff", "#eeede9", "#bdbab4", "#838079", "#d8b758", "#c5514c", "#ffffff", "#9b6e00", "#ffffff", "#05893e", "#ffffff", "#1879ca", "#ffffff", "#99000000", "#80000000"],
            "lightHigh": ["#f9f9f8", "#fdfdfc", "#ffffff", "#ecebe9", "#e4e3e0", "#f4ebd2", "#92908d", "#73716d", "#9d7c00", "#6e5600", "#5e4900", "#4e3c00", "#ffffff", "#272624", "#272624", "#838079", "#695200", "#a12f2f", "#ffffff", "#755200", "#ffffff", "#00672c", "#ffffff", "#005b9f", "#ffffff", "#52000000", "#29000000"],
            "darkHigh": ["#0c0c0c", "#131312", "#1c1c1b", "#232321", "#2b2a28", "#29220f", "#696764", "#85837e", "#9d7c00", "#c59e1b", "#d5ae34", "#e6be48", "#1d1500", "#eeede9", "#eeede9", "#838079", "#d8b758", "#f67d75", "#270d0b", "#d39700", "#201300", "#4cb86a", "#051c0a", "#4ea6fb", "#051729", "#99000000", "#80000000"]
        },
        { // hue 120
            "light": ["#f9f9f8", "#fdfdfc", "#ffffff", "#ebebe9", "#e2e3e1", "#e8efd6", "#90918d", "#71726d", "#758a05", "#6b7f00", "#5e6f00", "#516000", "#ffffff", "#262724", "#60615b", "#7f817a", "#586900", "#c5514c", "#ffffff", "#9b6e00", "#ffffff", "#05893e", "#ffffff", "#1879ca", "#ffffff", "#52000000", "#29000000"],
            "dark": ["#0c0c0c", "#131312", "#1c1c1b", "#232322", "#2a2a28", "#212512", "#676864", "#82847f", "#768b09", "#6b7f00", "#5e6f00", "#516000", "#ffffff", "#ecedea", "#b9bbb5", "#7f817a", "#b1c66a", "#c5514c", "#ffffff", "#9b6e00", "#ffffff", "#05893e", "#ffffff", "#1879ca", "#ffffff", "#99000000", "#80000000"],
            "lightHigh": ["#f9f9f8", "#fdfdfc", "#ffffff", "#ebebe9", "#e2e3e1", "#e8efd6", "#90918d", "#71726d", "#758a05", "#4f5f00", "#435000", "#364200", "#ffffff", "#262724", "#262724", "#7f817a", "#4d5c00", "#a12f2f", "#ffffff", "#755200", "#ffffff", "#00672c", "#ffffff", "#005b9f", "#ffffff", "#52000000", "#29000000"],
            "darkHigh": ["#0c0c0c", "#131312", "#1c1c1b", "#232322", "#2a2a28", "#212512", "#676864", "#82847f", "#768b09", "#95ac3b", "#a4bc4d", "#b4cc5d", "#141902", "#ecedea", "#ecedea", "#7f817a", "#b1c66a", "#f67d75", "#270d0b", "#d39700", "#201300", "#4cb86a", "#051c0a", "#4ea6fb", "#051729", "#99000000", "#80000000"]
        },
        { // hue 150
            "light": ["#f9f9f9", "#fcfdfc", "#ffffff", "#eaecea", "#e1e3e2", "#dcf2e0", "#8d908d", "#707471", "#2c924c", "#1c8742", "#007835", "#00672c", "#ffffff", "#252725", "#5d625e", "#7c827d", "#1d7339", "#c5514c", "#ffffff", "#9b6e00", "#ffffff", "#05893e", "#ffffff", "#1879ca", "#ffffff", "#52000000", "#29000000"],
            "dark": ["#0c0c0c", "#121312", "#1b1c1b", "#222322", "#292b29", "#17271a", "#656866", "#808481", "#319751", "#1c8742", "#007835", "#00672c", "#ffffff", "#eaeeeb", "#b6bcb7", "#7c827d", "#7fd091", "#c5514c", "#ffffff", "#9b6e00", "#ffffff", "#05893e", "#ffffff", "#1879ca", "#ffffff", "#99000000", "#80000000"],
            "lightHigh": ["#f9f9f9", "#fcfdfc", "#ffffff", "#eaecea", "#e1e3e2", "#dcf2e0", "#8d908d", "#707471", "#2c924c", "#00672c", "#005724", "#00481d", "#ffffff", "#252725", "#252725", "#7c827d", "#00622a", "#a12f2f", "#ffffff", "#755200", "#ffffff", "#00672c", "#ffffff", "#005b9f", "#ffffff", "#52000000", "#29000000"],
            "darkHigh": ["#0c0c0c", "#121312", "#1b1c1b", "#222322", "#292b29", "#17271a", "#656866", "#808481", "#319751", "#53b56d", "#63c57c", "#74d58b", "#061b0b", "#eaeeeb", "#eaeeeb", "#7c827d", "#7fd091", "#f67d75", "#270d0b", "#d39700", "#201300", "#4cb86a", "#051c0a", "#4ea6fb", "#051729", "#99000000", "#80000000"]
        },
        { // hue 180
            "light": ["#f8f9f9", "#fcfdfd", "#ffffff", "#e9eceb", "#e0e4e3", "#d4f3ec", "#8c908f", "#6e7473", "#009280", "#008474", "#007465", "#006457", "#ffffff", "#242726", "#5b6260", "#798280", "#007162", "#c5514c", "#ffffff", "#9b6e00", "#ffffff", "#05893e", "#ffffff", "#1879ca", "#ffffff", "#52000000", "#29000000"],
            "dark": ["#0c0c0c", "#121313", "#1b1c1c", "#212323", "#282b2a", "#0e2823", "#646967", "#7f8583", "#009583", "#008474", "#007465", "#006457", "#ffffff", "#e9eeed", "#b4bdbb", "#798280", "#4dd4bd", "#c5514c", "#ffffff", "#9b6e00", "#ffffff", "#05893e", "#ffffff", "#1879ca", "#ffffff", "#99000000", "#80000000"],
            "lightHigh": ["#f8f9f9", "#fcfdfd", "#ffffff", "#e9eceb", "#e0e4e3", "#d4f3ec", "#8c908f", "#6e7473", "#009280", "#006457", "#005449", "#00453c", "#ffffff", "#242726", "#242726", "#798280", "#006154", "#a12f2f", "#ffffff", "#755200", "#ffffff", "#00672c", "#ffffff", "#005b9f", "#ffffff", "#52000000", "#29000000"],
            "darkHigh": ["#0c0c0c", "#121313", "#1b1c1c", "#212323", "#282b2a", "#0e2823", "#646967", "#7f8583", "#009583", "#00b6a0", "#00c8b0", "#1fd9bf", "#001b17", "#e9eeed", "#e9eeed", "#798280", "#4dd4bd", "#f67d75", "#270d0b", "#d39700", "#201300", "#4cb86a", "#051c0a", "#4ea6fb", "#051729", "#99000000", "#80000000"]
        },
        { // hue 210
            "light": ["#f8f9f9", "#fcfdfd", "#ffffff", "#e9ecec", "#e0e3e4", "#d3f2f8", "#8b9091", "#6e7475", "#008d9f", "#008292", "#007280", "#00626f", "#ffffff", "#242728", "#5a6263", "#798284", "#006d7b", "#c5514c", "#ffffff", "#9b6e00", "#ffffff", "#05893e", "#ffffff", "#1879ca", "#ffffff", "#52000000", "#29000000"],
            "dark": ["#0c0c0c", "#121313", "#1b1c1c", "#212323", "#282b2b", "#0c272c", "#646869", "#808687", "#0090a2", "#008292", "#007280", "#00626f", "#ffffff", "#e9eeef", "#b4bcbe", "#798284", "#42cfe5", "#c5514c", "#ffffff", "#9b6e00", "#ffffff", "#05893e", "#ffffff", "#1879ca", "#ffffff", "#99000000", "#80000000"],
            "lightHigh": ["#f8f9f9", "#fcfdfd", "#ffffff", "#e9ecec", "#e0e3e4", "#d3f2f8", "#8b9091", "#6e7475", "#008d9f", "#00626f", "#00535e", "#00444e", "#ffffff", "#242728", "#242728", "#798284", "#005e6a", "#a12f2f", "#ffffff", "#755200", "#ffffff", "#00672c", "#ffffff", "#005b9f", "#ffffff", "#52000000", "#29000000"],
            "darkHigh": ["#0c0c0c", "#121313", "#1b1c1c", "#212323", "#282b2b", "#0c272c", "#646869", "#808687", "#0090a2", "#00b2c8", "#00c3db", "#00d5ef", "#001a1f", "#e9eeef", "#e9eeef", "#798284", "#42cfe5", "#f67d75", "#270d0b", "#d39700", "#201300", "#4cb86a", "#051c0a", "#4ea6fb", "#051729", "#99000000", "#80000000"]
        },
        { // hue 240
            "light": ["#f8f9fa", "#fcfdfd", "#ffffff", "#e9ebed", "#e1e3e5", "#d9efff", "#8e9194", "#6e7276", "#0089c9", "#007cb7", "#006da2", "#005e8c", "#ffffff", "#242729", "#5c6165", "#7a8186", "#006799", "#c5514c", "#ffffff", "#9b6e00", "#ffffff", "#05893e", "#ffffff", "#1879ca", "#ffffff", "#52000000", "#29000000"],
            "dark": ["#0c0c0c", "#121313", "#1b1c1c", "#222324", "#292a2c", "#132531", "#65686a", "#7f8487", "#0089c9", "#007cb7", "#006da2", "#005e8c", "#ffffff", "#eaedf0", "#b5bbc0", "#7a8186", "#6dc4ff", "#c5514c", "#ffffff", "#9b6e00", "#ffffff", "#05893e", "#ffffff", "#1879ca", "#ffffff", "#99000000", "#80000000"],
            "lightHigh": ["#f8f9fa", "#fcfdfd", "#ffffff", "#e9ebed", "#e1e3e5", "#d9efff", "#8e9194", "#6e7276", "#0089c9", "#005e8c", "#005078", "#004264", "#ffffff", "#242729", "#242729", "#7a8186", "#005a86", "#a12f2f", "#ffffff", "#755200", "#ffffff", "#00672c", "#ffffff", "#005b9f", "#ffffff", "#52000000", "#29000000"],
            "darkHigh": ["#0c0c0c", "#121313", "#1b1c1c", "#222324", "#292a2c", "#132531", "#65686a", "#7f8487", "#0089c9", "#39aaee", "#4cbaff", "#7ac9ff", "#021827", "#eaedf0", "#eaedf0", "#7a8186", "#6dc4ff", "#f67d75", "#270d0b", "#d39700", "#201300", "#4cb86a", "#051c0a", "#4ea6fb", "#051729", "#99000000", "#80000000"]
        },
        { // hue 270
            "light": ["#f9f9fa", "#fdfdfd", "#ffffff", "#eaebed", "#e2e3e5", "#e4ebff", "#8f9094", "#717378", "#6079d3", "#5870c9", "#4a61b9", "#3d52a8", "#ffffff", "#252629", "#5e6066", "#7d8087", "#465aa4", "#c5514c", "#ffffff", "#9b6e00", "#ffffff", "#05893e", "#ffffff", "#1879ca", "#ffffff", "#52000000", "#29000000"],
            "dark": ["#0c0c0c", "#121313", "#1b1c1d", "#222324", "#292a2c", "#1c2233", "#66676b", "#838489", "#6079d3", "#5870c9", "#4a61b9", "#3d52a8", "#ffffff", "#ebedf0", "#b8bac1", "#7d8087", "#a0b7ff", "#c5514c", "#ffffff", "#9b6e00", "#ffffff", "#05893e", "#ffffff", "#1879ca", "#ffffff", "#99000000", "#80000000"],
            "lightHigh": ["#f9f9fa", "#fdfdfd", "#ffffff", "#eaebed", "#e2e3e5", "#e4ebff", "#8f9094", "#717378", "#6079d3", "#3d52a8", "#314398", "#253588", "#ffffff", "#252629", "#252629", "#7d8087", "#3d5099", "#a12f2f", "#ffffff", "#755200", "#ffffff", "#00672c", "#ffffff", "#005b9f", "#ffffff", "#52000000", "#29000000"],
            "darkHigh": ["#0c0c0c", "#121313", "#1b1c1d", "#222324", "#292a2c", "#1c2233", "#66676b", "#838489", "#6079d3", "#819dfa", "#95afff", "#abc0ff", "#0f1529", "#ebedf0", "#ebedf0", "#7d8087", "#a0b7ff", "#f67d75", "#270d0b", "#d39700", "#201300", "#4cb86a", "#051c0a", "#4ea6fb", "#051729", "#99000000", "#80000000"]
        },
        { // hue 300
            "light": ["#f9f9fa", "#fdfdfd", "#ffffff", "#ebebed", "#e3e2e5", "#eee7ff", "#919094", "#737277", "#8e6ac7", "#8864c0", "#7955b0", "#6b46a0", "#ffffff", "#272629", "#615f65", "#817f86", "#6c4e9a", "#c5514c", "#ffffff", "#9b6e00", "#ffffff", "#05893e", "#ffffff", "#1879ca", "#ffffff", "#52000000", "#29000000"],
            "dark": ["#0c0c0c", "#131313", "#1c1c1c", "#232224", "#2a2a2c", "#251f31", "#68676a", "#858489", "#8e6ac7", "#8864c0", "#7955b0", "#6b46a0", "#ffffff", "#edecf0", "#bbb9c0", "#817f86", "#c6a9fc", "#c5514c", "#ffffff", "#9b6e00", "#ffffff", "#05893e", "#ffffff", "#1879ca", "#ffffff", "#99000000", "#80000000"],
            "lightHigh": ["#f9f9fa", "#fdfdfd", "#ffffff", "#ebebed", "#e3e2e5", "#eee7ff", "#919094", "#737277", "#8e6ac7", "#69459e", "#5c368e", "#4e287e", "#ffffff", "#272629", "#272629", "#817f86", "#634590", "#a12f2f", "#ffffff", "#755200", "#ffffff", "#00672c", "#ffffff", "#005b9f", "#ffffff", "#52000000", "#29000000"],
            "darkHigh": ["#0c0c0c", "#131313", "#1c1c1c", "#232224", "#2a2a2c", "#251f31", "#68676a", "#858489", "#8e6ac7", "#b28fef", "#c29fff", "#ceb5ff", "#191126", "#edecf0", "#edecf0", "#817f86", "#c6a9fc", "#f67d75", "#270d0b", "#d39700", "#201300", "#4cb86a", "#051c0a", "#4ea6fb", "#051729", "#99000000", "#80000000"]
        },
        { // hue 330
            "light": ["#f9f9f9", "#fdfdfd", "#ffffff", "#eceaec", "#e4e2e4", "#f9e4f6", "#938f92", "#767175", "#ae5ea8", "#a959a3", "#994b93", "#893c84", "#ffffff", "#282628", "#645e63", "#847e83", "#854480", "#c5514c", "#ffffff", "#9b6e00", "#ffffff", "#05893e", "#ffffff", "#1879ca", "#ffffff", "#52000000", "#29000000"],
            "dark": ["#0c0c0c", "#131213", "#1c1b1c", "#242223", "#2b292b", "#2c1d2b", "#696669", "#868285", "#ae5ea8", "#a959a3", "#994b93", "#893c84", "#ffffff", "#efecee", "#beb8bd", "#847e83", "#e69edf", "#c5514c", "#ffffff", "#9b6e00", "#ffffff", "#05893e", "#ffffff", "#1879ca", "#ffffff", "#99000000", "#80000000"],
            "lightHigh": ["#f9f9f9", "#fdfdfd", "#ffffff", "#eceaec", "#e4e2e4", "#f9e4f6", "#938f92", "#767175", "#ae5ea8", "#873a82", "#782b73", "#681c65", "#ffffff", "#282628", "#282628", "#847e83", "#7e3d79", "#a12f2f", "#ffffff", "#755200", "#ffffff", "#00672c", "#ffffff", "#005b9f", "#ffffff", "#52000000", "#29000000"],
            "darkHigh": ["#0c0c0c", "#131213", "#1c1b1c", "#242223", "#2b292b", "#2c1d2b", "#696669", "#868285", "#ae5ea8", "#d784d0", "#e894e0", "#f9a3f1", "#200e1f", "#efecee", "#efecee", "#847e83", "#e69edf", "#f67d75", "#270d0b", "#d39700", "#201300", "#4cb86a", "#051c0a", "#4ea6fb", "#051729", "#99000000", "#80000000"]
        }
    ]
    readonly property var roles: table[Math.round(((seedHue % 360) + 360) % 360 / 30) % 12][(dark ? "dark" : "light") + (highContrast ? "High" : "")]

    readonly property color canvas: roles[0]
    readonly property color surface: roles[1]
    readonly property color surfaceRaised: roles[2]
    readonly property color surfaceHover: roles[3]
    readonly property color surfacePressed: roles[4]
    readonly property color surfaceSelected: roles[5]
    readonly property color border: roles[6]
    readonly property color borderStrong: roles[7]
    readonly property color focusRing: roles[8]
    readonly property color accent: roles[9]
    readonly property color accentHover: roles[10]
    readonly property color accentPressed: roles[11]
    readonly property color textOnAccent: roles[12]
    readonly property color text: roles[13]
    readonly property color textMuted: roles[14]
    readonly property color textDisabled: roles[15]
    readonly property color link: roles[16]
    readonly property color danger: roles[17]
    readonly property color textOnDanger: roles[18]
    readonly property color warning: roles[19]
    readonly property color textOnWarning: roles[20]
    readonly property color success: roles[21]
    readonly property color textOnSuccess: roles[22]
    readonly property color info: roles[23]
    readonly property color textOnInfo: roles[24]
    readonly property color scrim: roles[25]
    readonly property color shadow: roles[26]
}
