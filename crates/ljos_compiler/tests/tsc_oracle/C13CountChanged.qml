// expect: accept
// Shape: a change handler of a declared property calls a function.
// After corpus/qtdoc/examples/demos/dice/DiceSpawner.qml:19.
import Ljos.Ui
Item {
    property int count: 3
    property int spawned: 0
    function respawn(): void { spawned = count }
    onCountChanged: respawn()
}
