#!/usr/bin/env python3
"""Wraps shown string literals that are Desktop messages in `t(...)`.

Leaves alone literals compared or matched on, ids, format templates and
anything already wrapped. Run from the repo: python3 scripts/i18n_wrap.py ../Signal-Desktop
"""
import glob, json, os, re, sys

DESKTOP = sys.argv[1] if len(sys.argv) > 1 else '../Signal-Desktop'
en = json.load(open(os.path.join(DESKTOP, '_locales/en/messages.json')))
def norm(s):
    return s.replace('\u2019', "'").replace('\u201c', '"').replace('\u201d', '"').replace('\u2026', '...').strip()
def rust_unescape(s):
    s = re.sub(r'\\u\{([0-9a-fA-F]+)\}', lambda m: chr(int(m.group(1), 16)), s)
    return s.replace('\\"', '"').replace('\\n', '\n').replace('\\\\', '\\')
plain = set()
for k, v in en.items():
    mf = v.get('messageformat') if isinstance(v, dict) else None
    if k.startswith('icu:') and mf and '{' not in mf and '<' not in mf:
        plain.add(norm(mf.replace("''", "'")))

LIT = re.compile(r'"((?:[^"\\]|\\.)*)"')
BEFORE_SKIP = re.compile(r'(==|!=|Id::unique\(|id_salt\(|\.with\(|paint_symbol\([^"]*|format!\(|matches!\([^"]*|contains\(|starts_with\(|ends_with\(|tr\(|tf\(|include_str!\(|concat!\(|panic!\(|expect\(|assert[a-z_]*!\([^"]*|unique\(\()\s*$')
AFTER_SKIP = re.compile(r'^\s*(=>|==|!=|\|)')
total = 0
for f in sorted(glob.glob('src/*.rs')):
    if f.endswith(('i18n.rs', 'i18n_locales.rs', 'symbols.rs')) or 'screenshot' in f:
        continue
    out_lines = []
    in_test = False
    for line in open(f).read().split('\n'):
        s = line.strip()
        if s.startswith('#[cfg(test)]'):
            in_test = True
        if in_test or s.startswith('//') or s.startswith('#[') or re.match(r'(pub(\([a-z]+\))? )?(const|static) ', s) or 'debug!' in s or 'warn!' in s or 'info!' in s:
            out_lines.append(line)
            continue
        pieces, last = [], 0
        for m in LIT.finditer(line):
            lit = m.group(1)
            before, after = line[:m.start()], line[m.end():]
            # Inside a string already (an odd number of quotes before) is
            # handled by finditer; skip non-messages and guarded contexts.
            # Symbol names, ids and setting values are lower-case words.
            if re.fullmatch(r'[a-z][a-z0-9-]*', lit) or norm(rust_unescape(lit)) not in plain or BEFORE_SKIP.search(before) or AFTER_SKIP.search(after) or before.endswith("r#"):
                continue
            pieces.append(line[last:m.start()])
            pieces.append(f'tr({m.group(0)})')
            last = m.end()
            total += 1
        pieces.append(line[last:])
        out_lines.append(''.join(pieces))
    new = '\n'.join(out_lines)
    if new != open(f).read():
        if 'use crate::i18n::tr;' not in new:
            # After the last top-level `use` line.
            idx = [i for i, l in enumerate(out_lines) if l.startswith('use ')]
            if idx:
                out_lines.insert(idx[-1] + 1, 'use crate::i18n::tr;')
                new = '\n'.join(out_lines)
        open(f, 'w').write(new)
print(total, 'wrapped')
