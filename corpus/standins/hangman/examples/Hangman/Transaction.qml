// Stands in for Transaction, what a purchase of a Product hands its
// handlers. Its names are those the Qt Purchasing documentation gives its
// Transaction type. The desktop's store sells nothing, so none is ever made;
// the type is here for what the example's handlers name of it.
import QtQml

QtObject {
    enum Status { PurchaseApproved, PurchaseFailed, PurchaseRestored }
    enum FailureReason { NoFailure, CanceledByUser, ErrorOccurred }

    readonly property int status: Transaction.PurchaseFailed
    readonly property int failureReason: Transaction.NoFailure
    readonly property string errorString: ""
    readonly property string orderId: ""
    readonly property date timestamp
    readonly property QtObject product: null

    function finalize() {}
}
