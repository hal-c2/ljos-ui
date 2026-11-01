// Stands in for FavoritesModel (favoritesmodel.cpp): the stocks chosen as
// favourites, in the order they were chosen, and a colour for each place.
import QtQml

QtObject {
    // Private in C++: the StockModel of each favourite.
    readonly property var d: ({ favorites: [] })
    readonly property var colors: [Qt.rgba(1, 0, 0, 1), Qt.rgba(0, 1, 0, 1), Qt.rgba(0, 0, 1, 1), Qt.rgba(1, 1, 0, 1), Qt.rgba(1, 0, 1, 1)]

    signal onFavoritesChanged(bool full)

    // For the engine: not for QML in C++.

    function addFavorite(stock) {
        d.favorites.push(stock)
        onFavoritesChanged(d.favorites.length >= 5)
    }

    function removeFavorite(stockId) {
        for (let i = 0; i < d.favorites.length; ++i) {
            if (d.favorites[i].stockId() === stockId) {
                d.favorites.splice(i, 1)
                onFavoritesChanged(false)
            }
        }
    }

    function favorites() {
        return d.favorites
    }

    // What QML may call.

    function atIndex(index) {
        if (index < 0 || index > d.favorites.length) {
            console.debug("invalid index: ", index)
            return null
        } else if (d.favorites.length === 0) {
            console.debug("favorites empty")
            return null
        }
        return d.favorites[index] ?? null
    }

    function count() {
        return d.favorites.length
    }

    function color(index) {
        return colors[index]
    }
}
