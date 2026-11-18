import Ljos.Ui

// A screen's name at its top, small and spaced, inside the glass.
Text {
    required property Theme theme
    anchors.horizontalCenter: parent.horizontalCenter
    color: theme.muted
    font.size: 15
    font.weight: Font.DemiBold
    font.letterSpacing: 2.5
}
