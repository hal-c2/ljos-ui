// c8: keeping a list model in step (note 11 §3.9), with the elided second
// loop written out: find each item's card, move it into place or insert it.
export function root__onItemsChanged(): void {
    const items = root.items;
    const shown = new Set(items);
    for (let i = cards.count - 1; i >= 0; --i) {
        if (shown.has(cards.get(i).toastId))
            continue;
        cards.remove(i);
    }
    for (let i = 0; i < items.length; ++i) {
        let at = -1;
        for (let j = i; j < cards.count; ++j) {
            if (cards.get(j).toastId === items[i]) {
                at = j;
                break;
            }
        }
        if (at < 0)
            cards.insert(i, { toastId: items[i] });
        else if (at !== i)
            cards.move(at, i, 1);
    }
    root.syncs += 1;
}
