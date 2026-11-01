// Stands in for TimeFormatter (timeformatter.cpp), the formatter of a 3D
// graph's value axis whose values are times: so much since 1970, in the unit
// epochFormat says, and written as selectionFormat says.
//
// C++ is asked through stringForValue, a virtual function of
// QValue3DAxisFormatter, which a QML type cannot take over: with Qt the
// labels of an axis with this formatter stay the numbers. The function is
// here for a graph that calls its formatter's.
import QtQml
import QtGraphs

Value3DAxisFormatter {
    enum EpochFormat { Ms, S, Day }

    property string selectionFormat
    property int epochFormat: TimeFormatter.EpochFormat.Ms
    // In milliseconds, whichever the format.
    property real epochOffset: 0

    signal originDateChanged(date date)

    function setSelectionFormat(format) {
        selectionFormat = format
    }

    function setEpochFormat(format) {
        epochFormat = format
    }

    function setEpochOffset(origin) {
        epochOffset = origin
    }

    function stringForValue(value, format) {
        switch (epochFormat) {
        case TimeFormatter.EpochFormat.Ms:
            return Qt.formatDateTime(new Date(Math.trunc(value + epochOffset)), selectionFormat)
        case TimeFormatter.EpochFormat.S:
            return Qt.formatDateTime(new Date(Math.trunc(value + Math.trunc(epochOffset / 1000)) * 1000), selectionFormat)
        case TimeFormatter.EpochFormat.Day:
            return Qt.formatDateTime(new Date(Math.trunc((value + Math.trunc(epochOffset / 86400000)) * 86400) * 1000), selectionFormat)
        default:
            return ""
        }
    }
}
