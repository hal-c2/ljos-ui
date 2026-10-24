// The Fusion style of Ljós (CLEAN_ROOM.md), a drawing of our own fitted to
// what qml6 shows for the default palette.
import QtQuick
import QtQuick.Shapes
import QtQuick.Templates as T

T.Dial {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    // The dial's square side, which its parts are measured by.
    readonly property real side: Math.min(width, height)

    // The dial's colour: the palette's button colour, but no darker than a
    // value of 180 and no more saturated than 140 of 255, as qml6 draws it
    // (any darker grey gives the same dial there).
    readonly property color face: {
        const c = control.palette.button
        const hue = c.hsvSaturation > 0 ? c.hsvHue : 0
        return Qt.hsva(hue, Math.min(c.hsvSaturation, 140 / 255),
                       Math.max(c.hsvValue, 180 / 255), 1)
    }
    // The rim's colour: a third of the face over a dark grey.
    readonly property color rim: Qt.tint(face, "#a6636363")

    // A raised disc about four fifths of the side across, in the
    // middle, lit from above its top left with a step two thirds of the way
    // out, and casting a soft shadow down and to the right. Not enabled, it
    // is flat and casts none.
    background: Component {
        Item {
            id: disc

            readonly property real radius: width / 2
            readonly property real edge: Math.max(1, Math.round(control.side / 150))
            readonly property real shift: control.side / 55

            implicitWidth: 100
            implicitHeight: 100
            width: Math.round(control.side * 0.823 - 2)
            height: width
            x: (control.width - width) / 2
            y: (control.height - height) / 2

            Repeater {
                model: control.enabled ? 4 : 0
                Rectangle {
                    readonly property real grow: (index + 1) * control.side / 200
                    x: disc.shift * (index + 1) / 4 - grow
                    y: x
                    width: disc.width + 2 * grow
                    height: width
                    radius: width / 2
                    color: "#08000000"
                }
            }

            Shape {
                anchors.fill: parent

                // The light, as qml6 places it: a third of the radius left
                // of the disc's top, reaching a little past twice the radius.
                RadialGradient {
                    id: light
                    centerX: disc.radius * 2 / 3
                    centerY: 0
                    centerRadius: disc.radius * 2.11
                    focalX: centerX
                    focalY: centerY
                    GradientStop { position: 0; color: Qt.lighter(control.face, 1.1) }
                    GradientStop { position: 0.61; color: control.face }
                    GradientStop { position: 0.62; color: Qt.darker(control.face, 1.02) }
                    GradientStop { position: 1; color: Qt.darker(control.face, 1.1) }
                }

                ShapePath {
                    strokeWidth: disc.edge
                    strokeColor: control.visualFocus ? Qt.darker(control.palette.highlight, 1.25)
                                                     : control.rim
                    fillColor: control.palette.base
                    fillGradient: control.enabled ? light : null

                    PathAngleArc {
                        centerX: disc.radius
                        centerY: disc.radius
                        radiusX: disc.radius - disc.edge / 2
                        radiusY: radiusX
                        startAngle: 0
                        sweepAngle: 360
                    }
                }

                // A faint light line inside the rim.
                ShapePath {
                    strokeWidth: control.enabled ? 1 : 0
                    strokeColor: "#26ffffff"
                    fillColor: "transparent"

                    PathAngleArc {
                        centerX: disc.radius
                        centerY: disc.radius
                        radiusX: disc.radius - disc.edge - 0.5
                        radiusY: radiusX
                        startAngle: 0
                        sweepAngle: 360
                    }
                }
            }
        }
    }

    // A small knob near the disc's rim, at the dial's angle. qml6 shades
    // it so faintly that a flat colour saves the four objects a gradient
    // takes.
    handle: Component {
        Rectangle {
            readonly property real reach: control.side * 0.28
            readonly property real turn: control.angle * Math.PI / 180

            width: control.side / 7
            height: width
            x: control.width / 2 + Math.sin(turn) * reach - width / 2
            y: control.height / 2 - Math.cos(turn) * reach - height / 2
            radius: width / 2
            color: Qt.darker(control.face, 1.07)
            border.color: control.visualFocus ? Qt.darker(control.palette.highlight, 1.25)
                                              : control.rim
        }
    }
}
