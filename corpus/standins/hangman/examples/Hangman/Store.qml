// Stands in for Store, through which the example's products are asked of
// the platform's in-app store. The documentation names Android's and iOS's;
// a desktop has none, so this store knows no products and has no purchases
// to restore.
import QtQml

QtObject {
    // Whether the platform has a store the products can be asked of.
    readonly property bool available: false

    function restorePurchases() {}
}
