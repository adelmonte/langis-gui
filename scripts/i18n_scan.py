#!/usr/bin/env python3
"""Measures how many of langis-gui's UI string literals are Desktop messages."""
import json, re, sys, glob, os
DESKTOP = sys.argv[1] if len(sys.argv) > 1 else '../Signal-Desktop'
en = json.load(open(os.path.join(DESKTOP, '_locales/en/messages.json')))
def norm(s):
    return s.replace('’', "'").replace('“', '"').replace('”', '"').replace('…', '...').strip()
plain = {}
for k, v in en.items():
    if not k.startswith('icu:'): continue
    mf = v.get('messageformat', '')
    if '{' in mf or '<' in mf: continue
    plain.setdefault(norm(mf.replace("''", "'")), k)
lit = re.compile(r'"((?:[^"\\]|\\.)*)"')
found, total = set(), set()
for f in glob.glob('src/*.rs'):
    for line in open(f):
        if line.strip().startswith('//'): continue
        for m in lit.finditer(line):
            s = m.group(1)
            if not re.search('[A-Za-z]', s) or len(s) < 2: continue
            s2 = s.encode().decode('unicode_escape', errors='ignore') if '\\u' in s else s
            total.add(s)
            if norm(s2) in plain: found.add(s)
print(len(total), 'literals;', len(found), 'are Desktop messages')
