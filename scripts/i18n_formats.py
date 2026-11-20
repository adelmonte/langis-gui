#!/usr/bin/env python3
"""Lists the app's format! templates that are Desktop ICU messages."""
import glob, json, re, sys
en = json.load(open((sys.argv[1] if len(sys.argv) > 1 else '../Signal-Desktop') + '/_locales/en/messages.json'))
def norm(s):
    return s.replace('\u2019', "'").replace('\u201c', '"').replace('\u201d', '"').replace('\u2026', '...').strip()

def forms(mf):
    """English renderings of an ICU message with each argument as {}: one per plural branch."""
    out = set()
    def expand(text):
        # Replace innermost simple args first.
        m = re.search(r'\{(\w+), (plural|select), ((?:[^{}]|\{[^{}]*\})*)\}', text)
        if m:
            for case in re.finditer(r'(=?\w+) \{([^{}]*)\}', m.group(3)):
                body = case.group(2).replace('#', '{}')
                expand(text[:m.start()] + body + text[m.end():])
            return
        out.add(norm(re.sub(r'\{\w+(, \w+)?\}', '{}', re.sub(r'</?\w+>', '', text)).replace("''", "'")))
    expand(mf)
    return out

table = {}
for k, v in en.items():
    mf = v.get('messageformat') if isinstance(v, dict) else None
    if k.startswith('icu:') and mf and '{' in mf:
        for f in forms(mf):
            table.setdefault(f, k)
hits = 0
for f in sorted(glob.glob('src/*.rs')):
    for n, line in enumerate(open(f), 1):
        for m in re.finditer(r'format!\(\s*"((?:[^"\\]|\\.)*)"', line):
            tpl = re.sub(r'\{[^{}:]*(:[^{}]*)?\}', '{}', m.group(1))
            tpl = re.sub(r'\\u\{([0-9a-fA-F]+)\}', lambda x: chr(int(x.group(1), 16)), tpl)
            key = table.get(norm(tpl))
            if key:
                hits += 1
                print(f'{f}:{n}: {key}  <- {m.group(1)}')
print(hits, 'matches', file=sys.stderr)
