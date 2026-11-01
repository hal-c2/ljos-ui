// Stands in for StockModel (stockmodel.cpp): what is known of one stock, its
// quotes and its history, the latest of each first.
import QtQml

QtObject {
    // Private in C++. What a function answers changes with these and tells
    // nobody, as there: the two signals are what does the telling.
    readonly property var d: ({ stockId: "", name: "", history: [], quotes: [], live: false })

    signal historyDataReady()
    signal quoteDataReady()

    // For the engine and the list of stocks: not for QML in C++.

    function addData(data) {
        d.history.push(data)
        historyDataReady()
    }

    function updateHistory(data) {
        d.history = data
        historyDataReady()
    }

    function appendQuote(data) {
        // The quote there is already.
        if (d.quotes.length > 0 && d.quotes[0].time === data.time)
            return
        d.quotes.unshift(data)
        quoteDataReady()
    }

    function resetQuote() {
        d.quotes = []
        quoteDataReady()
    }

    function setDataIsLive(live) {
        d.live = live
    }

    // What QML may call.

    function historyCount() {
        return d.history.length
    }

    function quoteCount() {
        return d.quotes.length
    }

    function stockId() {
        return d.stockId
    }

    function name() {
        return d.name
    }

    function price(index) {
        return d.quotes.length === 0 ? 0 : d.quotes[index].price
    }

    function change(index) {
        return d.quotes.length === 0 ? 0 : d.quotes[index].change
    }

    function changePercentage(index) {
        return d.quotes.length === 0 ? 0 : d.quotes[index].changePercentage
    }

    // The prices of a day are whole numbers to QML: `int` of a `float`.
    function highPrice(index) {
        return d.history.length === 0 ? 0 : Math.trunc(d.history[index].high)
    }

    function lowPrice(index) {
        return d.history.length === 0 ? 0 : Math.trunc(d.history[index].low)
    }

    function openPrice(index) {
        return d.history.length === 0 ? 0 : Math.trunc(d.history[index].open)
    }

    function closePrice(index) {
        return d.history.length === 0 ? 0 : Math.trunc(d.history[index].close)
    }

    function volume(index) {
        return d.history.length === 0 ? 0 : d.history[index].volume
    }

    function avgVolume() {
        let total = 0
        for (const day of d.history)
            total += day.volume
        return d.history.length > 0 ? Math.trunc(total / d.history.length) : 0
    }

    // Days of the calendar from that day to today.
    function daysFromNow(index) {
        if (d.history.length === 0)
            return 0
        const day = (time) => {
            const date = new Date(time)
            return Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()) / 86400000
        }
        return day(Date.now()) - day(d.history[index].time)
    }

    // The first day of the history that is before `date`, less the time of
    // day it is now; the earliest when none is.
    function indexOf(date) {
        if (d.history.length === 0)
            return -1
        const now = new Date()
        const sinceStartOfDay = ((now.getHours() * 60 + now.getMinutes()) * 60 + now.getSeconds()) * 1000 + now.getMilliseconds()
        const today = date.getTime() - sinceStartOfDay
        let index = 0
        while (index < d.history.length - 1 && Math.trunc((d.history[index].time - today) / 1000) >= 0)
            index++
        return index
    }

    function dataIsLive() {
        return d.live
    }

    function historyDate(index, milliseconds = true) {
        if (d.history.length === 0)
            return 0
        return milliseconds ? d.history[index].time : Math.floor(d.history[index].time / 1000)
    }

    function quoteTime(index, milliseconds = true) {
        if (d.quotes.length === 0 || index >= d.quotes.length)
            return 0
        return milliseconds ? d.quotes[index].time : Math.floor(d.quotes[index].time / 1000)
    }
}
