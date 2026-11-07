// expect: accept
// Shape: a file writes the type of a component it declares inline.
import Ljos.Ui
Item {
    component Card: Rectangle {}
    property Card card
    property list<Card> cards
}
