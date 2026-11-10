// Stands in for Product, one thing the store sells. Its names are those the
// Qt Purchasing documentation gives its Product type. Once it has both a
// store and an identifier it asks to be registered, and a store that is not
// there answers that it does not know it, so it is never Registered and
// cannot be bought: the store page shows only its Restore button.
import QtQml

QtObject {
    id: product

    enum ProductType { Consumable, Unlockable }
    enum Status { Uninitialized, PendingRegistration, Registered, Unknown }

    property string identifier
    property int type: Product.Consumable
    property QtObject store: null
    readonly property string price: ""
    readonly property string title: ""
    readonly property string description: ""
    property int status: Product.Uninitialized

    signal purchaseSucceeded(QtObject transaction)
    signal purchaseFailed(QtObject transaction)
    signal purchaseRestored(QtObject transaction)

    function register() {
        if (status !== Product.Uninitialized || store === null || identifier.length === 0)
            return
        status = Product.PendingRegistration
        Qt.callLater(() => {
            if (product.status === Product.PendingRegistration)
                product.status = store.available ? Product.Registered : Product.Unknown
        })
    }

    function purchase() {
        if (status !== Product.Registered)
            console.warn("Product: " + identifier + " is not registered with a store and cannot be bought")
    }

    function resetStatus() {
        status = Product.Uninitialized
        register()
    }

    onStoreChanged: register()
    onIdentifierChanged: register()
    Component.onCompleted: register()
}
