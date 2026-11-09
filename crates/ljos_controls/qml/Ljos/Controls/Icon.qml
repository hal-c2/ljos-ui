// Ljós's own style (docs/DIALECT.md §4.16): an icon drawn as a stroke, so
// it scales and takes any colour. `name` is a Lucide icon's name.
//
// The paths below are Lucide's (https://lucide.dev), under this licence:
//
//   ISC License
//
//   Copyright (c) for portions of Lucide are held by Cole Bemis 2013-2022
//   as part of Feather (MIT). All other copyright (c) for Lucide are held
//   by Lucide Contributors 2022.
//
//   Permission to use, copy, modify, and/or distribute this software for
//   any purpose with or without fee is hereby granted, provided that the
//   above copyright notice and this permission notice appear in all copies.
//
//   THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL
//   WARRANTIES WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED
//   WARRANTIES OF MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR
//   BE LIABLE FOR ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES
//   OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
//   WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
//   ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS
//   SOFTWARE.
import QtQuick
import QtQuick.Shapes

Item {
    id: icon

    property string name: ""
    property real size: Theme.iconSize
    property color color: Theme.text
    property real strokeWidth: Theme.stroke(size)
    property bool filled: false

    // Each on Lucide's 24-unit grid, as one SVG path. Only the marks the
    // controls draw; an app with more icons brings its own.
    readonly property var paths: ({
        "check": "M20 6 9 17l-5-5",
        "chevron-down": "M6 9l6 6 6-6",
        "chevron-left": "M15 18l-6-6 6-6",
        "chevron-right": "M9 18l6-6-6-6",
        "chevron-up": "M18 15l-6-6-6 6",
        "minus": "M5 12h14",
        "plus": "M5 12h14M12 5v14"
    })

    implicitWidth: size
    implicitHeight: size
    // A mark beside a label says nothing the label does not.
    Accessible.ignored: true

    Shape {
        width: 24
        height: 24
        transform: Scale {
            xScale: icon.size / 24
            yScale: icon.size / 24
        }

        ShapePath {
            strokeColor: icon.color
            strokeWidth: icon.strokeWidth
            fillColor: icon.filled ? icon.color : "transparent"
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin

            PathSvg {
                path: icon.paths[icon.name] ?? ""
            }
        }
    }
}
