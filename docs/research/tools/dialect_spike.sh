#!/usr/bin/env bash
# The phase-1 spike of docs/DIALECT.md: a real corpus file in Qt's QML, run
# as written, lowered as it loads (`--compat lower`) and migrated on disk
# (`ljos migrate --write`), and the three compared with one another and with
# qml6. The corpus file is copied into a scratch directory, never into the
# repository (CLEAN_ROOM.md).
#
# Run:  docs/research/tools/dialect_spike.sh LJOS QML_COMPARE [SCRATCH]
#   LJOS          the `ljos` binary
#   QML_COMPARE   the `qml-compare` binary
#   SCRATCH       where to work (default: a new directory under $TMPDIR)
# qml6 is optional: without it the Qt comparison is skipped and said so.
set -euo pipefail

ljos=$1
compare=$2
here=$(cd "$(dirname "$0")/../../.." && pwd)
work=${3:-$(mktemp -d "${TMPDIR:?set TMPDIR}/dialect-spike.XXXXXX")}
source=$here/corpus/qtdoc/examples/demos/hangman/qml/SimpleButton.qml
[ -f "$source" ] || {
	echo "SKIPPED: no corpus at $source (git submodule update --init corpus/qtdoc)"
	exit 0
}

rm -rf "$work/button" "$work/migrated" "$work/bad"
mkdir -p "$work/button"
cp "$source" "$work/button/SimpleButton.qml"
# Two buttons, one pressed, laid out by `x` alone: a `Row` is not of the
# Ljós API yet and would keep the pair as written.
cat >"$work/button/Main.qml" <<'EOF'
import QtQuick
Item {
    width: 240; height: 90
    SimpleButton { x: 0; width: 110; height: 80; text: "Play"; buttonColor: "#2060c0"; textColor: "#ffffff" }
    SimpleButton { x: 120; width: 110; height: 80; text: "Off"; buttonColor: "#c04020"; state: "PRESSED" }
}
EOF

echo "== ljos migrate (what lowering changes)"
"$ljos" migrate "$work/button"
cp -r "$work/button" "$work/migrated"
"$ljos" migrate --write "$work/migrated" >/dev/null
diff -u "$work/button/SimpleButton.qml" "$work/migrated/SimpleButton.qml" || true

echo "== renders"
for c in keep lower; do
	"$ljos" render "$work/button" --size 240x90 --system-fonts --compat "$c" --out "$work/$c.png" | tail -1
done
"$ljos" render "$work/migrated" --size 240x90 --system-fonts --out "$work/migrated.png" | tail -1
cmp "$work/keep.png" "$work/lower.png" && echo "keep and lower: identical PNG"
cmp "$work/keep.png" "$work/migrated.png" && echo "keep and migrated: identical PNG"

echo "== the checker on the migrated file"
"$ljos" check "$work/migrated"
mkdir -p "$work/bad"
sed 's/button.state = "PRESSED"/button.state = 1/' "$work/migrated/SimpleButton.qml" >"$work/bad/SimpleButton.qml"
"$ljos" check "$work/bad" || true

echo "== against qml6"
if command -v qml6 >/dev/null; then
	QT_QPA_PLATFORM=offscreen timeout -k 5 60 qml6 -I "$work/button" "$here/corpus/harness/reference.qml" \
		"file://$work/button/Main.qml" 240 90 1500 "$work/qt.png"
	for c in keep lower migrated; do
		echo "$c vs qml6: $("$compare" diff "$work/qt.png" "$work/$c.png")"
	done
else
	echo "SKIPPED: no qml6"
fi
