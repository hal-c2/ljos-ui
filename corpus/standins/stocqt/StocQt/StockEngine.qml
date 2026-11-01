// Stands in for StockEngine (stockengine.cpp), the singleton the views get
// everything from: the list of stocks with their latest quotes, the stock
// being looked at, and the favourites.
//
// The quotes and the histories are the example's own data/*.json, as in C++
// (which can ask no service either: its live data is switched off). They are
// resources of the program there, read when asked for; here they are read
// from the example's directory, and as there the answer is had before the
// function that asked returns.
pragma Singleton
import QtQml

QtObject {
    id: engine

    readonly property StockListModel stockListModel: StockListModel {}
    readonly property FavoritesModel favoritesModel: FavoritesModel {}
    // The engine's own to set. In C++ the property is announced by
    // onStockModelChanged, also when the same stock has new data, and what
    // is bound to it is evaluated again; a QML type cannot name the signal
    // of a property, so here that is only when the stock is another.
    property StockModel stockModel: null
    readonly property FilterModel filterModel: FilterModel {}

    signal onStockListModelChanged()
    signal onStockModelChanged()
    signal stockDataReady()
    signal onFavoritesModelChanged()
    signal onFavoritesChanged(bool full)
    signal onDataTypeChanged(bool live)
    signal onApiKeyTested(bool keyValid)
    signal onTimeFrameChanged()
    signal onLiveDataReady(real liveData)
    signal filterChanged()

    Component.onCompleted: {
        filterModel.sourceModel = stockListModel
        stockListModel.wasReset.connect(filterModel.reset)
        stockListModel.populate()
        stockModel = stockListModel.stockModel(0)
    }

    function currentStockId() {
        return stockModel.stockId()
    }

    function currentName() {
        return stockModel.name()
    }

    function isFavorite(stockId) {
        for (let i = 0; i < favoritesModel.count(); ++i) {
            if (favoritesModel.atIndex(i).stockId() === stockId)
                return true
        }
        return false
    }

    function useLiveData() {
        return false
    }

    function testApiKey() {
        onApiKeyTested(false)
    }

    function updateStockListModel() {
        const symbols = []
        for (let i = 0; i < stockListModel.count; ++i)
            symbols.push(stockListModel.get(i).stockId)
        stockQuote(symbols.join(","), (data) => stockListModel.updateDetails(data))
    }

    function updateStockView(stockId) {
        let index = 0
        for (let i = 0; i < stockListModel.count; i++) {
            if (stockListModel.get(i).stockId === stockId)
                index = i
        }
        const stock = stockListModel.stockModel(index)
        stockModel = stock
        // Once more for every time the stock is looked at, as in C++, which
        // disconnects nothing.
        stock.historyDataReady.connect(onStockModelChanged)
        stock.quoteDataReady.connect(onStockModelChanged)

        if (stock.historyCount() === 0 || stock.dataIsLive() !== useLiveData())
            updateStockModelHistory(stock.stockId())
        else
            onStockModelChanged()
    }

    function addFavorite(stockId) {
        for (let i = 0; i < stockListModel.count; i++) {
            if (stockListModel.get(i).stockId === stockId) {
                const stock = stockListModel.stockModel(i)
                if (favoritesModel.count() < 5) {
                    stockListModel.addFavorite(stockId)
                    favoritesModel.addFavorite(stock)
                    if (stock.historyCount() === 0)
                        stockHistory(stockId, (dataList) => stock.updateHistory(dataList))
                    onFavoritesChanged(favoritesModel.count() === 5)
                } else {
                    console.debug("Favorites are full")
                }
            }
        }
    }

    function removeFavorite(stockId) {
        stockListModel.removeFavorite(stockId)
        favoritesModel.removeFavorite(stockId)
        onFavoritesChanged(false)
    }

    function updateFavorites() {
        for (const stock of favoritesModel.favorites()) {
            if (stock.historyCount() === 0)
                stockHistory(stock.stockId(), (dataList) => stock.updateHistory(dataList))
        }
    }

    function updateStockModelHistory(stockId) {
        stockHistory(stockId, (dataList) => stockModel.updateHistory(dataList))
    }

    function updateStockModelQuote() {
        stockQuote(currentStockId(), (data) => {
            if (data.length > 0)
                stockModel.appendQuote(data[0])
        })
    }

    // The rest is ApiHandler (apihandler.cpp), private in C++.

    // A file of data/, whole; nothing when it cannot be read.
    function read(name) {
        const request = new XMLHttpRequest()
        request.open("GET", Qt.resolvedUrl("../../../qtdoc/examples/demos/stocqt/data/" + name), false)
        try {
            request.send()
        } catch (error) {
            return ""
        }
        if (request.status !== 200 && request.status !== 0 || !request.responseText) {
            console.debug("failed to read")
            return ""
        }
        return request.responseText
    }

    function stockQuote(symbols, onComplete) {
        onComplete(parseQuote(symbols.split(","), read("quotes.json")))
    }

    function stockHistory(symbol, onComplete) {
        onComplete(parseHistory(read(symbol + ".json")))
    }

    // What QJsonValue makes of a value that should be a number, as the
    // `float` or `int` it is kept in.
    function toFloat(value) {
        return typeof value === "number" ? Math.fround(value) : 0
    }

    function toInt(value) {
        return Number.isInteger(value) && Math.abs(value) <= 2147483647 ? value : 0
    }

    // The quote of each symbol that has one, in the symbols' order.
    function parseQuote(symbolList, data) {
        const dataList = []
        let array
        try {
            array = JSON.parse(data)
        } catch (error) {
            console.debug("Error when parsing json: ", error.message)
            return dataList
        }
        if (!Array.isArray(array))
            return dataList
        for (const symbol of symbolList) {
            for (const entry of array) {
                if (entry !== null && typeof entry === "object" && entry.symbol === symbol) {
                    dataList.push({
                        price: toFloat(entry.price),
                        change: toFloat(entry.change),
                        changePercentage: toFloat(entry.changesPercentage),
                        time: toInt(entry.timestamp) * 1000
                    })
                }
            }
        }
        return dataList
    }

    // Every day of the history, the latest first as the file has them. A day
    // starts at midnight, here.
    function parseHistory(data) {
        const dataList = []
        let array
        try {
            array = JSON.parse(data).historical
        } catch (error) {
            return dataList
        }
        if (!Array.isArray(array))
            return dataList
        for (const entry of array) {
            const day = entry !== null && typeof entry === "object" ? entry : ({})
            const date = /^(\d{4})-(\d{2})-(\d{2})$/.exec(typeof day.date === "string" ? day.date : "")
            dataList.push({
                high: toFloat(day.high),
                low: toFloat(day.low),
                open: toFloat(day.open),
                close: toFloat(day.close),
                volume: toInt(day.volume),
                time: date ? new Date(Number(date[1]), Number(date[2]) - 1, Number(date[3])).getTime() : 0
            })
        }
        return dataList
    }
}
