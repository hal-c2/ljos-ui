#!/usr/bin/env python3
"""Ljos UI research 08: inventory QML/JS usage across two corpora.

Run:  python3 -I qml_inventory.py [--md PATH] [--json PATH]

Stdlib only and read-only over the corpora. This is a light heuristic, not a
QML parser: comments and string/regex/template contents are masked, then a
brace stack attributes `key:` lines to the innermost `Type {` instantiation.
"""
import argparse
import collections
import datetime
import json
import os
import re

DEV = '/home/olafura/dev'
DOCS = os.path.join(DEV, 'ljos-ui', 'docs', 'research')
DEFAULT_MD = os.path.join(DOCS, '08-qml-usage-inventory.md')
DEFAULT_JSON = os.path.join(DOCS, 'data', 'qml_inventory.json')

CORPORA = collections.OrderedDict([
    ('A', {
        'title': 'Corpus A: hal-c2 product QML',
        'roots': [os.path.join(DEV, 'hal-c2', 'apps', 'desktop-qt'),
                  os.path.join(DEV, 'hal-c2', 'apps', 'mobile-qt'),
                  os.path.join(DEV, 'hal-c2', 'plugins')],
    }),
    ('B', {
        'title': "Corpus B: Qt's own examples (qtdoc)",
        'roots': [os.path.join(DEV, 'qml-solid', 'corpus', 'qtdoc')],
    }),
])
EXCLUDE_DIRS = {'build', 'node_modules', 'target', '.git'}
SOURCE_EXT = ('.qml', '.js', '.mjs')
REQUESTED_APIS = ['Qt.binding', 'Qt.callLater', 'Qt.createComponent', 'Qt.rgba',
                  'Qt.formatDateTime', 'Qt.include']
API_JS_FEATURES = ['JSON.parse', 'JSON.stringify', 'Promise', 'qsTr()', 'qsTrId()',
                   'qsTranslate()', 'qsTrN()/qsTrIdN()']
REGEX_AFTER = set('(,=:[!&|?{};+-*%<>~^')
REGEX_KEYWORDS = {'return', 'typeof', 'case', 'do', 'else', 'in', 'of', 'new',
                  'delete', 'void', 'throw', 'instanceof', 'yield', 'await'}
CONT_CHARS = set('+-*/%&|?:,(=<>!^[')

TYPE_OPEN_RE = re.compile(r'(?<![\w.$])((?:[A-Za-z_]\w*\.)*)([A-Z]\w*)(\s+on\s+[A-Za-z_][\w.]*)?\s*\{')
BOUNDARY_RE = re.compile(r'[{};\n]')
KEY_RE = re.compile(r'^\s*(?:(?:default|required|readonly)\s+)*'
                    r'(property\s+(?:alias\s+)?[^:;{}]*?\s+)?'
                    r'([A-Za-z_]\w*(?:\.[A-Za-z_]\w*)*)\s*:(?!:)')
ON_HANDLER_RE = re.compile(r'on[A-Z]')
IMPORT_RE = re.compile(r'^[ \t]*import[ \t]+(.+?)[ \t]*$', re.M)
MOD_RE = re.compile(r'^([A-Za-z_][\w.]*)(?:[ \t]+(\d[\d.]*))?(?:[ \t]+as[ \t]+([A-Za-z_]\w*))?$')
PATH_RE = re.compile(r'^(["\'])(.+?)\1(?:[ \t]+as[ \t]+[A-Za-z_]\w*)?$')
QT_INCLUDE_RE = re.compile(r'\bQt\.include\(\s*(["\'])(.+?)\1')
PRAGMA_RE = re.compile(r'^[ \t]*pragma[ \t]+([A-Za-z_]\w*)(?:[ \t]*:[ \t]*([A-Za-z_]\w*))?', re.M)
JS_PRAGMA_RE = re.compile(r'^[ \t]*\.pragma[ \t]+(\w+)', re.M)
QT_CALL_RE = re.compile(r'\bQt\.([a-z]\w*)\s*\(')
QT_CONST_RE = re.compile(r'\bQt\.([A-Z]\w*)\b(?!\s*\()')

JS_FEATURES = collections.OrderedDict([
    ('function declaration', re.compile(r'\bfunction\s+[A-Za-z_$][\w$]*\s*\(')),
    ('function expression (anonymous)', re.compile(r'\bfunction\s*\(')),
    ('arrow function =>', re.compile(r'=>')),
    ('let', re.compile(r'\blet\s+[A-Za-z_$]')),
    ('const', re.compile(r'\bconst\s+[A-Za-z_$]')),
    ('var', re.compile(r'\bvar\s+[A-Za-z_$]')),
    ('JSON.parse', re.compile(r'\bJSON\.parse\b')),
    ('JSON.stringify', re.compile(r'\bJSON\.stringify\b')),
    ('Promise', re.compile(r'\bPromise\b')),
    ('async', re.compile(r'\basync\b')),
    ('await', re.compile(r'\bawait\b')),
    ('qsTr()', re.compile(r'\bqsTr\s*\(')),
    ('qsTrId()', re.compile(r'\bqsTrId\s*\(')),
    ('qsTranslate()', re.compile(r'\bqsTranslate\s*\(')),
    ('qsTrN()/qsTrIdN()', re.compile(r'\bqsTr(?:Id)?N\s*\(')),
])

MISC_FEATURES = collections.OrderedDict([
    ('required property', re.compile(r'\brequired\s+property\b')),
    ('default property', re.compile(r'\bdefault\s+property\b')),
    ('readonly property', re.compile(r'\breadonly\s+property\b')),
    ('property alias', re.compile(r'\bproperty\s+alias\b')),
    ('inline component (component X: Y {)', re.compile(r'\bcomponent\s+[A-Z]\w*\s*:\s*[A-Za-z_][\w.]*\s*\{')),
    ('enum declaration', re.compile(r'\benum\s+[A-Za-z_]\w*\s*\{')),
])


class Tally:
    """Instance counts and distinct-file sets per key."""

    def __init__(self):
        self.n = collections.Counter()
        self.f = collections.defaultdict(set)

    def add(self, key, rel, k=1):
        if k:
            self.n[key] += k
            self.f[key].add(rel)

    def files(self, key):
        return len(self.f.get(key, ()))


def scan(src):
    """Return (code, masked, regex_count, template_count).

    code:   comments blanked, strings kept (for import paths).
    masked: comments and string/regex/template contents blanked (for braces, keys, features).
    """
    n = len(src)
    code = list(src)
    masked = list(src)
    regex_n = tmpl_n = 0
    last_sig = ''
    last_word = ''
    i = 0

    def blank(buf, a, b):
        for k in range(a, b):
            if buf[k] != '\n':
                buf[k] = ' '

    while i < n:
        c = src[i]
        nxt = src[i + 1] if i + 1 < n else ''
        if c == '/' and nxt == '/':
            j = src.find('\n', i)
            j = n if j < 0 else j
            blank(code, i, j)
            blank(masked, i, j)
            i = j
        elif c == '/' and nxt == '*':
            j = src.find('*/', i + 2)
            j = n if j < 0 else j + 2
            blank(code, i, j)
            blank(masked, i, j)
            i = j
        elif c in '"\'':
            j = i + 1
            while j < n and src[j] not in (c, '\n'):
                j += 2 if src[j] == '\\' else 1
            j = min(j, n)
            blank(masked, i + 1, j)
            last_sig, last_word = c, ''
            i = min(j + 1, n)
        elif c == '`':
            j = i + 1
            while j < n and src[j] != '`':
                j += 2 if src[j] == '\\' else 1
            j = min(j, n)
            blank(masked, i + 1, j)
            tmpl_n += 1
            last_sig, last_word = c, ''
            i = min(j + 1, n)
        elif c == '/' and (last_sig == '' or last_sig in REGEX_AFTER or last_word in REGEX_KEYWORDS):
            j = i + 1
            in_class = False
            while j < n and src[j] != '\n':
                ch = src[j]
                if ch == '\\':
                    j += 2
                    continue
                if ch == '[':
                    in_class = True
                elif ch == ']':
                    in_class = False
                elif ch == '/' and not in_class:
                    break
                j += 1
            if j < n and src[j] == '/':
                blank(masked, i + 1, j)
                j += 1
                while j < n and src[j].isalpha():
                    j += 1
                regex_n += 1
                last_sig, last_word = 'a', ''
                i = min(j, n)
            else:
                last_sig, last_word = '/', ''
                i += 1
        elif c.isalnum() or c in '_$':
            j = i
            while j < n and (src[j].isalnum() or src[j] in '_$'):
                j += 1
            last_sig, last_word = 'a', src[i:j]
            i = j
        elif c.isspace():
            i += 1
        else:
            last_sig, last_word = c, ''
            i += 1
    return ''.join(code), ''.join(masked), regex_n, tmpl_n


def build_units(roots):
    paths = []
    for root in roots:
        for dirpath, dirnames, filenames in os.walk(root):
            dirnames[:] = sorted(d for d in dirnames if d not in EXCLUDE_DIRS)
            for fn in filenames:
                if fn.endswith(SOURCE_EXT):
                    paths.append(os.path.join(dirpath, fn))
    units = []
    for path in sorted(set(paths)):
        with open(path, encoding='utf-8', errors='replace') as fh:
            src = fh.read()
        code, masked, regex_n, tmpl_n = scan(src)
        units.append({
            'path': path,
            'rel': os.path.relpath(path, DEV),
            'kind': os.path.splitext(path)[1][1:],
            'lines': len(src.splitlines()),
            'code': code,
            'masked': masked,
            'regex': regex_n,
            'template': tmpl_n,
            'imports': [],
            'import_dirs': set(),
            'inline': set(),
        })
    return units


def index_units(units):
    """Collect imports, inline components, same-dir type definitions and import aliases."""
    local_dir_defs = set()
    aliases = {}
    for u in units:
        if u['kind'] != 'qml':
            continue
        d = os.path.dirname(u['path'])
        base = os.path.basename(u['path'])[:-len('.qml')]
        if base[:1].isupper():
            local_dir_defs.add((d, base))
        u['inline'] = set(re.findall(r'\bcomponent\s+([A-Z]\w*)\s*:', u['masked']))
        for m in IMPORT_RE.finditer(u['code']):
            rest = m.group(1)
            pm = PATH_RE.match(rest)
            if pm:
                target = os.path.normpath(os.path.join(d, pm.group(2)))
                if pm.group(2).endswith(('.js', '.mjs')):
                    u['imports'].append(('js', pm.group(2), target, os.path.isfile(target)))
                else:
                    isdir = os.path.isdir(target)
                    u['imports'].append(('dir', pm.group(2), target, isdir))
                    if isdir:
                        u['import_dirs'].add(target)
                continue
            mm = MOD_RE.match(rest)
            if mm:
                mod, ver, alias = mm.groups()
                if alias:
                    aliases[alias] = mod
                u['imports'].append(('module', mod, ver or 'none', True))
        for m in QT_INCLUDE_RE.finditer(u['code']):
            target = os.path.normpath(os.path.join(d, m.group(2)))
            u['imports'].append(('js', m.group(2), target, os.path.isfile(target)))
    return local_dir_defs, aliases


def key_walk(masked, brace_types):
    """Yield (owner_type, key, is_decl) for `key:` statements directly inside object bodies."""
    frames = []
    seg_start = 0
    for m in BOUNDARY_RE.finditer(masked):
        p = m.start()
        ch = m.group()
        text = masked[seg_start:p]
        if ch == '\n' and text.strip() and text.rstrip()[-1] in CONT_CHARS:
            continue  # expression continues on the next line
        if frames and frames[-1] is not None:
            km = KEY_RE.match(text)
            if km:
                yield frames[-1], km.group(2), km.group(1) is not None
        if ch == '{':
            frames.append(brace_types.get(p))
        elif ch == '}' and frames:
            frames.pop()
        seg_start = p + 1


def is_visible_local(u, name, local_dir_defs):
    d = os.path.dirname(u['path'])
    if (d, name) in local_dir_defs or name in u['inline']:
        return True
    return any((D, name) in local_dir_defs for D in u['import_dirs'])


def analyze(units, local_dir_defs, aliases):
    tal = collections.defaultdict(Tally)
    type_keys = collections.defaultdict(Tally)
    counts = collections.Counter()

    for u in units:
        rel, kind, m = u['rel'], u['kind'], u['masked']
        counts['files_' + kind] += 1
        counts['lines_' + kind] += u['lines']
        tal['js'].add('regex literal', rel, u['regex'])
        tal['js'].add('template literal', rel, u['template'])
        for name, rx in JS_FEATURES.items():
            tal['js'].add(name, rel, len(rx.findall(m)))
        for mm in QT_CALL_RE.finditer(m):
            tal['qt_call'].add('Qt.' + mm.group(1) + '()', rel)
        for mm in QT_CONST_RE.finditer(m):
            tal['qt_const'].add('Qt.' + mm.group(1), rel)
        if kind == 'qml':
            for pm in PRAGMA_RE.finditer(m):
                key = f'pragma {pm.group(1)}' + (f': {pm.group(2)}' if pm.group(2) else '')
                tal['pragma'].add(key, rel)
            for name, rx in MISC_FEATURES.items():
                tal['misc'].add(name, rel, len(rx.findall(m)))
        else:
            for pm in JS_PRAGMA_RE.finditer(m):
                tal['pragma'].add(f'.pragma {pm.group(1)} (js)', rel)
            continue

        # ---- QML-only analysis below ----
        for imp in u['imports']:
            if imp[0] == 'module':
                tal['module'].add(imp[1], rel)
                tal['module_version'].add(f'{imp[1]} {imp[2]}', rel)
            elif imp[0] == 'dir':
                tal['dir_import'].add(imp[1], rel)
                counts['dir_import_stmts'] += 1
                if not imp[3]:
                    counts['dir_import_unresolved'] += 1
            elif imp[0] == 'js':
                counts['js_import_stmts'] += 1
                if imp[3]:
                    tal['js_import'].add(os.path.relpath(imp[2], DEV), rel)
                else:
                    tal['js_unresolved'].add(imp[1], rel)

        brace = {}
        for mt in TYPE_OPEN_RE.finditer(m):
            qual = mt.group(1).rstrip('.')
            name = mt.group(2)
            pre = re.search(r'(\w+)\s*$', m[max(0, mt.start() - 16):mt.start()])
            if pre and pre.group(1) in ('enum', 'class'):
                continue
            if not qual:
                cls = 'local' if is_visible_local(u, name, local_dir_defs) else 'qt'
            else:
                cls = 'qt' if aliases.get(qual, qual).startswith('Qt') else 'third-party'
            tid = name if cls == 'qt' else f'{name} ({cls})'
            tal['types'].add(tid, rel)
            counts['instantiations'] += 1
            if mt.group(3):
                tal['value_source'].add(tid, rel)
            brace[mt.end() - 1] = tid
        if brace:
            counts['files_with_objects'] += 1

        for owner, key, is_decl in key_walk(m, brace):
            if is_decl:
                counts['declared_props'] += 1
                continue
            if key == 'id':
                continue
            if '.' in key and key.split('.')[0][:1].isupper():
                tal['attached'].add(key, rel)
            elif ON_HANDLER_RE.match(key):
                tal['handler'].add(key, rel)
            else:
                type_keys[owner].add(key, rel)

    return {'tal': tal, 'type_keys': type_keys, 'counts': counts}


def ser_tally(t):
    keys = sorted(t.n, key=lambda k: (-t.files(k), -t.n[k], k))
    return {k: {'instances': t.n[k], 'files': t.files(k)} for k in keys}


# ----------------------------------------------------------------- markdown

def md_table(headers, rows):
    out = ['| ' + ' | '.join(headers) + ' |', '|' + '|'.join('---' for _ in headers) + '|']
    for r in rows:
        out.append('| ' + ' | '.join(str(c).replace('|', '\\|') for c in r) + ' |')
    return '\n'.join(out)


def pct(files, denom):
    return f'{100 * files / denom:.0f}%' if denom else '—'


def ranked_by_files(t, skip=()):
    return sorted((k for k in t.n if k not in skip), key=lambda k: (-t.files(k), -t.n[k], k))


def ranked_by_instances(t):
    return sorted(t.n, key=lambda k: (-t.n[k], -t.files(k), k))


def keys_text(t, k, skip=('id',)):
    items = [x for x in ranked_by_files(t, skip)][:k]
    return ', '.join(f'`{x}` ({t.files(x)})' for x in items) or '—'


def side_rows(ra, rb, k, fn):
    rows = []
    for i in range(k):
        a = ra[i] if i < len(ra) else None
        b = rb[i] if i < len(rb) else None
        rows.append([i + 1] + fn(a) + fn(b))
    return rows


def frac(t, key, denom):
    return t.files(key) / denom if denom else 0.0


def score(key, sa, sb, field, denom_field):
    """Weighted prevalence: 3 x share of A files + share of B files."""
    fa = frac(sa['tal'][field], key, sa[denom_field])
    fb = frac(sb['tal'][field], key, sb[denom_field])
    return 3 * fa + fb, fa, fb


def implication_table(cands, sa, sb, field, denom_field, k, label_fn=lambda x: f'`{x}`'):
    scored = []
    for key in cands:
        s, fa, fb = score(key, sa, sb, field, denom_field)
        scored.append((s, key, fa, fb))
    scored.sort(key=lambda x: (-x[0], x[1]))
    rows = []
    for i, (s, key, fa, fb) in enumerate(scored[:k]):
        rows.append([i + 1, label_fn(key),
                     f"{sa['tal'][field].files(key)} ({fa:.0%})",
                     f"{sb['tal'][field].files(key)} ({fb:.0%})",
                     f'{s:.2f}'])
    return md_table(['Rank', 'Item', 'A files (share)', 'B files (share)', 'Score'], rows)


def build_md(S, meta):
    A, B = S['A'], S['B']
    out = []
    w = out.append
    w('# QML usage inventory (Ljós UI research 08)')
    w('')
    w(f"Generated {meta['date']} by `docs/research/tools/qml_inventory.py` (run with `python3 -I`). "
      "Raw results: `docs/research/data/qml_inventory.json`.")
    w('')
    w('## Scope and method')
    w('')
    w(f"- **A** ({A['title']}): {', '.join('`' + r + '`' for r in A['roots_rel'])}. "
      "All `*.qml` under these trees, plus `.js`/`.mjs` for import resolution and JS-feature counts.")
    w(f"- **B** ({B['title']}): `{B['roots_rel'][0]}`.")
    w('- Excluded at any depth: `build/`, `node_modules/`, `target/`, `.git/`.')
    w('- Comments and string/regex/template contents are masked before analysis. Object instantiations '
      'are `TypeName {` (including dotted names such as `Controls.Button {`) and value sources '
      '`Type on prop {`. Keys are attributed to the innermost instantiation with a brace stack; '
      '`key:` statements inside plain blocks (handler bodies, object literals, functions) are ignored.')
    w('- Types are classed as `qt` (default), `local` (a `<Name>.qml` in the same directory, '
      'an imported relative directory, or an inline `component`), or `third-party` (qualified by a '
      'non-`Qt*` module). Local types are shown with a suffix and excluded from the Qt-type tables.')
    w('- `files` means distinct files containing the item. Shares use the number of `.qml` files '
      '(QML items) or all `.qml`/`.js`/`.mjs` files (JS features and Qt API calls) as denominator.')
    w('- Heuristic, not a QML parser: expect small error bars, mostly from ternaries split across lines, '
      'unusual formatting, and unqualified types that actually come from third-party modules.')
    w('')
    w(f"Denominators: A has **{A['nqml']} .qml** / **{A['nsrc']} source** files; "
      f"B has **{B['nqml']} .qml** / **{B['nsrc']} source** files.")
    w('')

    # 1 -------------------------------------------------------------
    w('## 1. Corpus size')
    w('')
    rows = [
        ['`.qml` files', A['nqml'], B['nqml']],
        ['total lines in `.qml`', A['c']['lines_qml'], B['c']['lines_qml']],
        ['`.js` files in corpus', A['c']['files_js'], B['c']['files_js']],
        ['`.mjs` files in corpus', A['c']['files_mjs'], B['c']['files_mjs']],
        ['distinct `.js`/`.mjs` files imported by QML (incl. `Qt.include`)',
         len(A['tal']['js_import'].n), len(B['tal']['js_import'].n)],
        ['`.js`/`.mjs` import statements (resolved + unresolved)',
         A['c']['js_import_stmts'], B['c']['js_import_stmts']],
        ['unresolved `.js` imports', sum(A['tal']['js_unresolved'].n.values()),
         sum(B['tal']['js_unresolved'].n.values())],
        ['distinct relative directory imports', len(A['tal']['dir_import'].n), len(B['tal']['dir_import'].n)],
        ['relative directory import statements', A['c']['dir_import_stmts'], B['c']['dir_import_stmts']],
        ['object instantiations (all types)', A['c']['instantiations'], B['c']['instantiations']],
        ['distinct instantiated type names', len(A['tal']['types'].n), len(B['tal']['types'].n)],
        ['`.qml` files with ≥1 instantiation', A['c']['files_with_objects'], B['c']['files_with_objects']],
    ]
    w(md_table(['Metric', 'A', 'B'], rows))
    w('')

    # 2 -------------------------------------------------------------
    w('## 2. Imports')
    w('')
    w('### 2a. Module imports (`import X [version] [as Y]`)')
    w('')
    mods = sorted(set(A['tal']['module'].n) | set(B['tal']['module'].n),
                  key=lambda m: (-(A['tal']['module'].n[m] + B['tal']['module'].n[m]), m))

    def versions(c, mod):
        vs = [(v.split(' ', 1)[1], n) for v, n in c.most_common() if v.split(' ', 1)[0] == mod]
        return ', '.join(f'{v} ({n})' for v, n in vs[:3]) or '—'

    vA, vB = A['tal']['module_version'].n, B['tal']['module_version'].n
    rows = []
    for mod in mods:
        rows.append([f'`{mod}`',
                     A['tal']['module'].n[mod], A['tal']['module'].files(mod), versions(vA, mod),
                     B['tal']['module'].n[mod], B['tal']['module'].files(mod), versions(vB, mod)])
    w(md_table(['Module', 'A stmts', 'A files', 'A versions', 'B stmts', 'B files', 'B versions'], rows))
    w('')
    w('### 2b. Relative directory imports (`import "dir"`)')
    w('')
    dirs = sorted(set(A['tal']['dir_import'].n) | set(B['tal']['dir_import'].n),
                  key=lambda d: (-(A['tal']['dir_import'].n[d] + B['tal']['dir_import'].n[d]), d))[:20]
    rows = [[f'`"{d}"`', A['tal']['dir_import'].n[d], B['tal']['dir_import'].n[d]] for d in dirs]
    w(md_table(['Import string', 'A stmts', 'B stmts'], rows) if rows else 'None.')
    w('')
    w('### 2c. JS files imported by QML (top 20 per corpus, resolved paths)')
    w('')
    jsA = ranked_by_instances(A['tal']['js_import'])[:20]
    jsB = ranked_by_instances(B['tal']['js_import'])[:20]
    rows = []
    for i in range(20):
        a = jsA[i] if i < len(jsA) else None
        b = jsB[i] if i < len(jsB) else None
        rows.append([
            f'`{a}`' if a else '—', A['tal']['js_import'].n[a] if a else '—',
            f'`{b}`' if b else '—', B['tal']['js_import'].n[b] if b else '—',
        ])
    w(md_table(['A file', 'A imports', 'B file', 'B imports'], rows))
    w('')

    # 3 -------------------------------------------------------------
    w('## 3. Object type instantiations (top 150 per corpus, ranked by instance count)')
    w('')
    w('`Type on prop {` value sources are counted as instantiations of `Type`. `(local)` and `(third-party)` '
      'types are excluded from the Qt tables.')
    w('')
    ra = ranked_by_instances(A['tal']['types'])[:150]
    rb = ranked_by_instances(B['tal']['types'])[:150]

    def type_cell(t, tid):
        if tid is None:
            return ['—', '—', '—']
        return [f'`{tid}`', t['tal']['types'].n[tid], t['tal']['types'].files(tid)]

    rows = []
    for i in range(150):
        a = ra[i] if i < len(ra) else None
        b = rb[i] if i < len(rb) else None
        rows.append([i + 1] + type_cell(A, a) + type_cell(B, b))
    w(md_table(['Rank', 'A type', 'A inst.', 'A files', 'B type', 'B inst.', 'B files'], rows))
    w('')

    # 4 -------------------------------------------------------------
    w('## 4. Property binding names on the top 40 Qt types')
    w('')
    w('Types are ranked by the Implications score (3 × A share + B share, `.qml` files containing the type). '
      'Keys are `name:` or `name.sub:` bindings directly in the object body; the number in parentheses is the '
      'number of distinct files using the key on that type. `id` is excluded.')
    w('')
    cands = set()
    for t in (A, B):
        cands |= {x for x in t['tal']['types'].n if ' (' not in x}
    ranked_types = []
    for tid in cands:
        s, _, _ = score(tid, A, B, 'types', 'nqml')
        ranked_types.append((s, tid))
    ranked_types.sort(key=lambda x: (-x[0], x[1]))
    top40 = [tid for _, tid in ranked_types[:40]]
    rows = []
    for tid in top40:
        rows.append([
            f'`{tid}`',
            f"{A['tal']['types'].n[tid]} / {A['tal']['types'].files(tid)}",
            keys_text(A['type_keys'].get(tid, Tally()), 8),
            f"{B['tal']['types'].n[tid]} / {B['tal']['types'].files(tid)}",
            keys_text(B['type_keys'].get(tid, Tally()), 8),
        ])
    w(md_table(['Type', 'A inst / files', 'A top keys', 'B inst / files', 'B top keys'], rows))
    w('')
    w('### 4b. Attached properties and grouped keys (top 60 per corpus)')
    w('')
    w('Attached = `Upper.name` (`Layout.fillWidth`, `Keys.onPressed`, `Component.onCompleted`, '
      '`ListView.view`). Grouped lowercase keys (`anchors.fill`, `border.color`) are in 4a above.')
    w('')
    aa = ranked_by_files(A['tal']['attached'])[:60]
    ab = ranked_by_files(B['tal']['attached'])[:60]
    rows = []
    for i in range(60):
        a = aa[i] if i < len(aa) else None
        b = ab[i] if i < len(ab) else None
        rows.append([i + 1,
                     f"`{a}` ({A['tal']['attached'].files(a)})" if a else '—',
                     f"`{b}` ({B['tal']['attached'].files(b)})" if b else '—'])
    w(md_table(['Rank', 'A attached (files)', 'B attached (files)'], rows))
    w('')

    # 5 -------------------------------------------------------------
    w('## 5. Signal handlers (top 60 per corpus, direct `onX:` in object bodies)')
    w('')
    ha = ranked_by_files(A['tal']['handler'])[:60]
    hb = ranked_by_files(B['tal']['handler'])[:60]
    rows = []
    for i in range(60):
        a = ha[i] if i < len(ha) else None
        b = hb[i] if i < len(hb) else None
        rows.append([i + 1,
                     f"`{a}` ({A['tal']['handler'].files(a)})" if a else '—',
                     f"`{b}` ({B['tal']['handler'].files(b)})" if b else '—'])
    w(md_table(['Rank', 'A handler (files)', 'B handler (files)'], rows))
    w('')

    # 6 -------------------------------------------------------------
    w('## 6. JS features and Qt APIs')
    w('')
    w('### 6a. JS language features (all `.qml`, `.js`, `.mjs`; denominator = source files)')
    w('')
    feats = list(JS_FEATURES) + ['regex literal', 'template literal']
    rows = []
    for f_ in feats:
        rows.append([f'`{f_}`' if '(' in f_ or '=>' in f_ else f_,
                     A['tal']['js'].n[f_], A['tal']['js'].files(f_), pct(A['tal']['js'].files(f_), A['nsrc']),
                     B['tal']['js'].n[f_], B['tal']['js'].files(f_), pct(B['tal']['js'].files(f_), B['nsrc'])])
    w(md_table(['Feature', 'A count', 'A files', 'A share', 'B count', 'B files', 'B share'], rows))
    w('')
    w('### 6b. Qt.* API calls (requested names plus the top 30 by score)')
    w('')
    api_cands = set(REQUESTED_APIS) | set(A['tal']['qt_call'].n) | set(B['tal']['qt_call'].n)
    api_rank = sorted(api_cands, key=lambda x: (-score(x, A, B, 'qt_call', 'nsrc')[0], x))
    api_show = list(dict.fromkeys(REQUESTED_APIS + api_rank))[:len(REQUESTED_APIS) + 30]
    rows = []
    for api in api_show:
        key = api + '()' if api != 'Qt.include' else 'Qt.include()'
        rows.append([f'`{api}`',
                     A['tal']['qt_call'].n[key], A['tal']['qt_call'].files(key),
                     B['tal']['qt_call'].n[key], B['tal']['qt_call'].files(key)])
    w(md_table(['API', 'A calls', 'A files', 'B calls', 'B files'], rows))
    w('')
    w('`Qt.include` is also counted as a JS import in section 1; the `()` suffix is the lookup key.')
    w('')
    w('### 6c. Qt enum/constant references (`Qt.AlignLeft`, `Qt.Key_*`, ...; top 20 per corpus)')
    w('')
    ca = ranked_by_instances(A['tal']['qt_const'])[:20]
    cb = ranked_by_instances(B['tal']['qt_const'])[:20]
    rows = []
    for i in range(20):
        a = ca[i] if i < len(ca) else None
        b = cb[i] if i < len(cb) else None
        rows.append([i + 1,
                     f"`{a}` ({A['tal']['qt_const'].n[a]})" if a else '—',
                     f"`{b}` ({B['tal']['qt_const'].n[b]})" if b else '—'])
    w(md_table(['Rank', 'A constant (refs)', 'B constant (refs)'], rows))
    w('')

    # 7 -------------------------------------------------------------
    w('## 7. Pragmas and QML language features')
    w('')
    pk = sorted(set(A['tal']['pragma'].n) | set(B['tal']['pragma'].n),
                key=lambda k: (-(A['tal']['pragma'].files(k) + B['tal']['pragma'].files(k)), k))
    rows = [[f'`{k}`', A['tal']['pragma'].n[k], A['tal']['pragma'].files(k),
             B['tal']['pragma'].n[k], B['tal']['pragma'].files(k)] for k in pk]
    rows.append(['declared properties (`property T name: v`)', A['c']['declared_props'], '—',
                 B['c']['declared_props'], '—'])
    mk = list(MISC_FEATURES)
    for k in mk:
        rows.append([f'`{k}`', A['tal']['misc'].n[k], A['tal']['misc'].files(k),
                     B['tal']['misc'].n[k], B['tal']['misc'].files(k)])
    w(md_table(['Feature', 'A count', 'A files', 'B count', 'B files'], rows))
    w('')

    # Implications -----------------------------------------------------
    w('## Implications: what to implement first')
    w('')
    w('Score = **3 × A share + B share**, where *share* is the fraction of that corpus\'s files that use the '
      'item. Shares, not raw counts, keep the ×3 weight meaningful (B has more files than A). '
      'Local and third-party types are excluded.')
    w('')
    w('### Qt types')
    w('')
    types_cands = {x for t in (A, B) for x in t['tal']['types'].n if ' (' not in x}
    w(implication_table(types_cands, A, B, 'types', 'nqml', 25))
    w('')
    w('### Attached properties (and attached handlers)')
    w('')
    att = {x for t in (A, B) for x in t['tal']['attached'].n}
    w(implication_table(att, A, B, 'attached', 'nqml', 15))
    w('')
    w('### Qt APIs and JS built-ins used by Ljós-relevant code')
    w('')
    apis = {x for t in (A, B) for x in t['tal']['qt_call'].n} | set(API_JS_FEATURES)
    # JS features are indexed without the `()` suffix only for JSON/Promise
    w(api_implication_table(apis, A, B, 15))
    w('')
    w('### Signal handlers')
    w('')
    hd = {x for t in (A, B) for x in t['tal']['handler'].n}
    w(implication_table(hd, A, B, 'handler', 'nqml', 15))
    w('')
    return '\n'.join(out) + '\n'


def api_implication_table(apis, sa, sb, k):
    """Qt.* calls live in tal['qt_call'], JS built-ins in tal['js']; score both over source files."""

    def files(s, key):
        if key.startswith('Qt.'):
            return s['tal']['qt_call'].files(key)
        return s['tal']['js'].files(key)

    scored = []
    for key in apis:
        fa = files(sa, key) / sa['nsrc'] if sa['nsrc'] else 0.0
        fb = files(sb, key) / sb['nsrc'] if sb['nsrc'] else 0.0
        scored.append((3 * fa + fb, key, fa, fb))
    scored.sort(key=lambda x: (-x[0], x[1]))
    rows = []
    for i, (s, key, fa, fb) in enumerate(scored[:k]):
        rows.append([i + 1, f'`{key}`',
                     f"{files(sa, key)} ({fa:.0%})", f"{files(sb, key)} ({fb:.0%})", f'{s:.2f}'])
    return md_table(['Rank', 'Item', 'A files (share)', 'B files (share)', 'Score'], rows)


def summarize(S):
    """Make the per-corpus block JSON-serialisable."""
    out = {}
    for name, s in S.items():
        out[name] = {
            'title': s['title'],
            'roots': s['roots_rel'],
            'qml_files': s['nqml'],
            'source_files': s['nsrc'],
            'counts': dict(s['c']),
            'js_imports_resolved': ser_tally(s['tal']['js_import']),
            'js_imports_unresolved': dict(s['tal']['js_unresolved'].n),
            'modules': ser_tally(s['tal']['module']),
            'module_versions': ser_tally(s['tal']['module_version']),
            'relative_dir_imports': ser_tally(s['tal']['dir_import']),
            'types': ser_tally(s['tal']['types']),
            'value_source_types': ser_tally(s['tal']['value_source']),
            'type_property_keys': {tid: ser_tally(t) for tid, t in s['type_keys'].items()},
            'attached': ser_tally(s['tal']['attached']),
            'handlers': ser_tally(s['tal']['handler']),
            'js_features': ser_tally(s['tal']['js']),
            'qt_calls': ser_tally(s['tal']['qt_call']),
            'qt_constants': ser_tally(s['tal']['qt_const']),
            'pragmas': ser_tally(s['tal']['pragma']),
            'misc_features': ser_tally(s['tal']['misc']),
        }
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--md', default=DEFAULT_MD)
    ap.add_argument('--json', default=DEFAULT_JSON)
    args = ap.parse_args()

    S = collections.OrderedDict()
    for cname, cfg in CORPORA.items():
        units = build_units(cfg['roots'])
        local_dir_defs, aliases = index_units(units)
        res = analyze(units, local_dir_defs, aliases)
        c = res['counts']
        S[cname] = {
            'title': cfg['title'],
            'roots_rel': [os.path.relpath(r, DEV) for r in cfg['roots']],
            'tal': res['tal'],
            'type_keys': res['type_keys'],
            'c': c,
            'nqml': c['files_qml'],
            'nsrc': c['files_qml'] + c['files_js'] + c['files_mjs'],
        }
        print(f"{cname}: {c['files_qml']} qml, {c['lines_qml']} lines, "
              f"{c['files_js']} js, {c['files_mjs']} mjs, {c['instantiations']} instantiations")

    meta = {'date': datetime.date.today().isoformat()}
    os.makedirs(os.path.dirname(args.json), exist_ok=True)
    os.makedirs(os.path.dirname(args.md), exist_ok=True)
    with open(args.json, 'w', encoding='utf-8') as fh:
        json.dump({'generated': meta['date'], 'corpora': summarize(S)}, fh, indent=1, ensure_ascii=False)
    with open(args.md, 'w', encoding='utf-8') as fh:
        fh.write(build_md(S, meta))
    print(f'wrote {args.json}')
    print(f'wrote {args.md}')


if __name__ == '__main__':
    main()
