#!/usr/bin/env python3
"""Counts the Qt QML surface the Ljós dialect (docs/DIALECT.md) cleans up.

Run:  python3 -I docs/research/tools/dialect_inventory.py ROOT [ROOT...]

Stdlib only, read-only. A heuristic over text, not a parser: comments and
string contents are masked with the masking of `qml_inventory.py` (note 08),
then each pattern is counted in `.qml` files. Build and test copies are
skipped. Prints one `pattern<TAB>uses<TAB>files` line per pattern, so two
corpora can be compared by running it twice.
"""
import collections
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from qml_inventory import scan  # noqa: E402

SKIP_DIRS = {'build', '.git', 'node_modules', 'target', 'tests', 'auto'}

# On the masked text: strings are blanked, so `"..."` is `"   "`.
PATTERNS = collections.OrderedDict([
    ('property (all declarations)', r'\bproperty\s+[\w<>.]+\s+\w+'),
    ('property var', r'\bproperty\s+var\s+\w'),
    ('property variant', r'\bproperty\s+variant\s+\w'),
    ('property list<...>', r'\bproperty\s+list\s*<'),
    ('required property', r'\brequired\s+property\b'),
    ('Qt.* (any member)', r'(?<![\w.])Qt\.[a-z]\w*'),
    ('Qt.* enum keys (Qt.AlignLeft ...)', r'(?<![\w.])Qt\.[A-Z]\w*'),
    ('Connections {', r'\bConnections\s*\{'),
    ('Component.onCompleted', r'\bComponent\.onCompleted\b'),
    ('Component.onDestruction', r'\bComponent\.onDestruction\b'),
    ('anchors.* bindings', r'\banchors\.\w+\s*:'),
    ('anchors { group', r'\banchors\s*\{'),
    ('Layout.* attached', r'\bLayout\.\w+\s*:'),
    ('parent.* reads', r'(?<![\w.])parent\.\w+'),
    ('state names compared or set ("..." strings)', r'\bstate\s*(?:===?|!==?|:|=)\s*"'),
    ('PropertyChanges {', r'\bPropertyChanges\s*\{'),
    ('properties: "..." (animation strings)', r'\bproperties?\s*:\s*"'),
    ('ListModel {', r'\bListModel\s*\{'),
    ('ListElement {', r'\bListElement\s*\{'),
    ('model: [ (JS array)', r'\bmodel\s*:\s*\['),
    ('model: <number>', r'\bmodel\s*:\s*\d'),
    ('modelData', r'\bmodelData\b'),
    ('model.<role>', r'(?<![\w.])model\.\w+'),
    ('color: "..." (colour as string)', r'\bcolor\s*:\s*"'),
    ('== / != (loose equality)', r'(?<![=!<>])[=!]=(?!=)'),
    ('=== / !==', r'[=!]=='),
    ('null', r'\bnull\b'),
    ('undefined', r'\bundefined\b'),
    ('typeof', r'\btypeof\b'),
    ('var statements', r'(?<![\w.])var\s+\w'),
    ('function declarations', r'\bfunction\s+\w+\s*\('),
    ('signal handlers on<X>:', r'(?<![\w.])on[A-Z]\w*\s*:'),
    ('.connect( (dynamic signal connection)', r'\.connect\s*\('),
    ('createObject(', r'\.createObject\s*\('),
    ('Qt.createComponent / createQmlObject', r'\bQt\.create(?:Component|QmlObject)\b'),
    ('Qt.binding(', r'\bQt\.binding\s*\('),
    ('Binding {', r'\bBinding\s*\{'),
    ('Loader {', r'\bLoader\s*\{'),
    ('MouseArea {', r'\bMouseArea\s*\{'),
    ('TapHandler {', r'\bTapHandler\s*\{'),
    ('Rectangle {', r'\bRectangle\s*\{'),
    ('Text {', r'(?<![\w.])Text\s*\{'),
    ('Label {', r'\bLabel\s*\{'),
    ('Item {', r'(?<![\w.])Item\s*\{'),
    ('QtObject {', r'\bQtObject\s*\{'),
    ('console.*', r'\bconsole\.\w+'),
    ('JSON.parse / stringify', r'\bJSON\.(?:parse|stringify)\b'),
    ('objectName', r'\bobjectName\b'),
    ('Keys.* attached', r'\bKeys\.\w+'),
    ('Accessible.* attached', r'\bAccessible\.\w+'),
    ('arguments', r'\barguments\b'),
    ('this', r'\bthis\b'),
])

QT_MEMBER = re.compile(r'(?<![\w.])Qt\.([a-z]\w*)')


def qml_files(root):
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        for f in filenames:
            if f.endswith('.qml'):
                yield os.path.join(dirpath, f)


def main(roots):
    uses = collections.Counter()
    files = collections.Counter()
    qt_members = collections.Counter()
    compiled = [(k, re.compile(p)) for k, p in PATTERNS.items()]
    n_files = n_lines = 0
    for root in roots:
        for path in qml_files(root):
            with open(path, encoding='utf-8', errors='replace') as fh:
                src = fh.read()
            n_files += 1
            n_lines += src.count('\n')
            _, masked, _, _ = scan(src)
            for key, rx in compiled:
                c = len(rx.findall(masked))
                if c:
                    uses[key] += c
                    files[key] += 1
            qt_members.update(QT_MEMBER.findall(masked))
    print(f'files\t{n_files}\t{n_lines} lines')
    for key in PATTERNS:
        print(f'{key}\t{uses[key]}\t{files[key]}')
    print('Qt.<member> by use:\t' + ', '.join(f'{m} {c}' for m, c in qt_members.most_common()))


if __name__ == '__main__':
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    main(sys.argv[1:])
