// Stands in for StockListModel (stocklistmodel.cpp): the stocks the program
// knows, a row each, with the latest quote of each and whether it is a
// favourite.
//
// The roles are stockId, name, favorite, date, price, change,
// changePercentage and filter. Whenever something changes all rows are made
// again, which is what the C++ tells its views (a model reset).
import QtQml
import QtQml.Models

ListModel {
    // Private in C++: what each row is of (stockId, name, favorite and the
    // stock's StockModel), and the favourites' stockIds.
    readonly property var d: ({ data: [], favorites: [] })
    readonly property Component stock: Component { StockModel {} }

    // QAbstractItemModel's modelReset.
    signal wasReset()

    // What a view is given of each stock.
    function rows() {
        return d.data.map((entry) => ({
            stockId: entry.stockId,
            name: entry.name,
            favorite: entry.favorite,
            date: entry.model.quoteTime(0),
            price: entry.model.price(0),
            change: entry.model.change(0),
            changePercentage: entry.model.changePercentage(0),
            filter: entry.stockId + " " + entry.name
        }))
    }

    function reset() {
        clear()
        for (const row of rows())
            append(row)
        wasReset()
    }

    function updateDetails(data) {
        for (let i = 0; i < d.data.length; ++i) {
            // A quote for each stock, in order: C++ takes it that there is.
            if (i < data.length)
                d.data[i].model.appendQuote(data[i])
            d.data[i].favorite = d.favorites.includes(d.data[i].stockId)
        }
        reset()
    }

    function resetQuotes() {
        for (const entry of d.data)
            entry.model.resetQuote()
    }

    function addFavorite(stockId) {
        for (const entry of d.data) {
            if (entry.stockId === stockId) {
                entry.favorite = true
                d.favorites.push(stockId)
                reset()
            }
        }
    }

    function removeFavorite(stockId) {
        for (const entry of d.data) {
            if (entry.stockId === stockId) {
                entry.favorite = false
                reset()
            }
        }
        for (let i = 0; i < d.favorites.length; ++i) {
            if (d.favorites[i] === stockId)
                d.favorites.splice(i, 1)
        }
    }

    function stockModel(index) {
        return d.data[index].model
    }

    // What the constructor does in C++. The engine asks for it: it needs the
    // stocks before anything else is done.
    function populate() {
        if (d.data.length > 0)
            return
        for (const [stockId, name] of stocks) {
            const model = stock.createObject(this)
            model.d.stockId = stockId
            model.d.name = name
            d.data.push({ stockId, name, favorite: false, model })
        }
        reset()
    }

    readonly property var stocks: [
        ["QTCOM.HE", "Qt Group"],
        ["AAPL", "Apple Inc"],
        ["MSFT", "Microsoft Corp"],
        ["AMZN", "Amazon.com Inc"],
        ["NVDA", "NVIDIA Corp"],
        ["TSLA", "Tesla Inc"],
        ["GOOG", "Alphabet Inc"],
        ["GOOGL", "Alphabet Inc"],
        ["META", "Meta Platforms Inc"],
        ["AVGO", "Broadcom Inc"],
        ["PEP", "PepsiCo Inc"],
        ["COST", "Costco Wholesale Corp"],
        ["CSCO", "Cisco Systems Inc"],
        ["TMUS", "T-Mobile US Inc"],
        ["CMCSA", "Comcast Corp"],
        ["TXN", "Texas Instruments Inc"],
        ["ADBE", "Adobe Inc"],
        ["NFLX", "Netflix Inc"],
        ["QCOM", "QUALCOMM Inc"],
        ["HON", "Honeywell International Inc"],
        ["AMD", "Advanced Micro Devices Inc"],
        ["AMGN", "Amgen Inc"],
        ["SBUX", "Starbucks Corp"],
        ["INTU", "Intuit Inc"],
        ["INTC", "Intel Corp"],
        ["GILD", "Gilead Sciences Inc"],
        ["AMAT", "Applied Materials Inc"],
        ["BKNG", "Booking Holdings Inc"],
        ["ADI", "Analog Devices Inc"],
        ["ADP", "Automatic Data Processing Inc"],
        ["MDLZ", "Mondelez International Inc"],
        ["PYPL", "PayPal Holdings Inc"],
        ["REGN", "Regeneron Pharmaceuticals Inc"],
        ["ISRG", "Intuitive Surgical Inc"],
        ["VRTX", "Vertex Pharmaceuticals Inc"],
        ["LRCX", "Lam Research Corp"],
        ["CSX", "CSX Corp"],
        ["MU", "Micron Technology Inc"],
        ["MELI", "MercadoLibre Inc"],
        ["ATVI", "Activision Blizzard Inc"],
        ["CHTR", "Charter Communications Inc"],
        ["PANW", "Palo Alto Networks Inc"],
        ["SNPS", "Synopsys Inc"],
        ["ASML", "ASML Holding NV"],
        ["KLAC", "KLA Corp"],
        ["MAR", "Marriott International Inc/MD"],
        ["MRNA", "Moderna Inc"],
        ["MNST", "Monster Beverage Corp"],
        ["CDNS", "Cadence Design Systems Inc"],
        ["ORLY", "O'Reilly Automotive Inc"],
        ["ABNB", "Airbnb Inc"],
        ["KDP", "Keurig Dr Pepper Inc"],
        ["KHC", "Kraft Heinz Co/The"],
        ["FTNT", "Fortinet Inc"],
        ["NXPI", "NXP Semiconductors NV"],
        ["AEP", "American Electric Power Co Inc"],
        ["MCHP", "Microchip Technology Inc"],
        ["CTAS", "Cintas Corp"],
        ["ADSK", "Autodesk Inc"],
        ["DXCM", "Dexcom Inc"],
        ["PDD", "PDD Holdings Inc ADR"],
        ["EXC", "Exelon Corp"],
        ["AZN", "AstraZeneca PLC ADR"],
        ["PAYX", "Paychex Inc"],
        ["IDXX", "IDEXX Laboratories Inc"],
        ["BIIB", "Biogen Inc"],
        ["MRVL", "Marvell Technology Inc"],
        ["ROST", "Ross Stores Inc"],
        ["WBD", "Warner Bros Discovery Inc"],
        ["LULU", "Lululemon Athletica Inc"],
        ["PCAR", "PACCAR Inc"],
        ["ODFL", "Old Dominion Freight Line Inc"],
        ["WDAY", "Workday Inc"],
        ["GFS", "GLOBALFOUNDRIES Inc"],
        ["XEL", "Xcel Energy Inc"],
        ["CPRT", "Copart Inc"],
        ["CTSH", "Cognizant Technology Solutions Corp"],
        ["DLTR", "Dollar Tree Inc"],
        ["ILMN", "Illumina Inc"],
        ["WBA", "Walgreens Boots Alliance Inc"],
        ["BKR", "Baker Hughes Co"],
        ["EA", "Electronic Arts Inc"],
        ["FAST", "Fastenal Co"],
        ["CSGP", "CoStar Group Inc"],
        ["ENPH", "Enphase Energy Inc"],
        ["VRSK", "Verisk Analytics Inc"],
        ["ANSS", "ANSYS Inc"],
        ["CRWD", "Crowdstrike Holdings Inc"],
        ["EBAY", "eBay Inc"],
        ["FANG", "Diamondback Energy Inc"],
        ["CEG", "Constellation Energy Corp"],
        ["TEAM", "Atlassian Corp"],
        ["ALGN", "Align Technology Inc"],
        ["DDOG", "Datadog Inc"],
        ["JD", "JD.com Inc ADR"],
        ["ZS", "Zscaler Inc"],
        ["ZM", "Zoom Video Communications Inc"],
        ["RIVN", "Rivian Automotive Inc"],
        ["SIRI", "Sirius XM Holdings Inc"],
        ["LCID", "Lucid Group Inc"]
    ]
}
