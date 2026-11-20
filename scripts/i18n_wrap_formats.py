#!/usr/bin/env python3
"""Turns format! templates that are Desktop ICU messages into tf(...).

`format!("{n} members")` becomes
`crate::i18n::tf("icu:Key", &format!("{n} members"), &[("count", (n).into())])`:
the English stays exactly as it was, other languages get Desktop's message.
Only when every one of the message's arguments maps to a placeholder.
"""
import glob, json, re, sys
DESKTOP = sys.argv[1] if len(sys.argv) > 1 else '../Signal-Desktop'
en = json.load(open(DESKTOP + '/_locales/en/messages.json'))
SKIP_KEYS = {'icu:message--getNotificationText--text-with-emoji', 'icu:About__AppEnvironment', 'icu:AboutContactModal__TitleAndTitleWithoutNickname'}

def norm(s):
    return s.replace('\u2019', "'").replace('\u201c', '"').replace('\u201d', '"').replace('\u2026', '...').strip()

def forms(mf):
    """(English with each argument as {}, the argument names in order)."""
    out = []
    def expand(text, names_hint):
        m = re.search(r'\{(\w+), (plural|select), ((?:[^{}]|\{[^{}]*\})*)\}', text)
        if m:
            var = m.group(1)
            for case in re.finditer(r'(=?\w+) \{([^{}]*)\}', m.group(3)):
                body = case.group(2).replace('#', '{' + var + '}')
                expand(text[:m.start()] + body + text[m.end():], names_hint)
            return
        clean = re.sub(r'</?\w+>', '', text).replace("''", "'")
        names = re.findall(r'\{(\w+)(?:, \w+)?\}', clean)
        out.append((norm(re.sub(r'\{\w+(?:, \w+)?\}', '{}', clean)), names))
    expand(mf, None)
    return out

table = {}
for k, v in en.items():
    mf = v.get('messageformat') if isinstance(v, dict) else None
    if k.startswith('icu:') and mf and '{' in mf and k not in SKIP_KEYS:
        required = set(re.findall(r'\{(\w+)[,}]', mf))
        for f, names in forms(mf):
            table.setdefault(f, (k, names, required))

def split_args(text):
    """Top-level comma-separated arguments of `text` (after the template)."""
    args, depth, cur, i, in_str = [], 0, '', 0, False
    while i < len(text):
        c = text[i]
        if in_str:
            cur += c
            if c == '\\':
                cur += text[i + 1]; i += 1
            elif c == '"':
                in_str = False
        elif c == '"':
            in_str = True; cur += c
        elif c in '([{':
            depth += 1; cur += c
        elif c in ')]}':
            if depth == 0:
                args.append(cur.strip()); return args, i
            depth -= 1; cur += c
        elif c == ',' and depth == 0:
            args.append(cur.strip()); cur = ''
        else:
            cur += c
        i += 1
    return None, None

count = 0
for f in sorted(glob.glob('src/*.rs')):
    if 'screenshot' in f or f.endswith('i18n.rs'):
        continue
    src = open(f).read()
    out, last = [], 0
    for m in re.finditer(r'format!\(\s*"((?:[^"\\]|\\.)*)"', src):
        tpl = m.group(1)
        holders = re.findall(r'\{([^{}]*)\}', tpl)
        if any(':' in h for h in holders) or len(re.findall(r'[A-Za-z]', re.sub(r'\{[^{}]*\}', '', tpl))) < 3:
            continue
        shape = re.sub(r'\\u\{([0-9a-fA-F]+)\}', lambda x: chr(int(x.group(1), 16)), re.sub(r'\{[^{}]*\}', '{}', tpl))
        hit = table.get(norm(shape))
        if not hit:
            continue
        key, names, required = hit
        rest = src[m.end():]
        if rest.lstrip().startswith(','):
            args, end = split_args(rest.lstrip()[1:])
            if args is None:
                continue
            end = m.end() + (len(rest) - len(rest.lstrip())) + 1 + end
        elif rest.lstrip().startswith(')'):
            args, end = [], m.end() + (len(rest) - len(rest.lstrip()))
        else:
            continue
        args = [a for a in args if a]
        # Each placeholder's expression, in order.
        exprs, pos = [], 0
        ok = True
        for h in holders:
            if h == '':
                if pos >= len(args):
                    ok = False; break
                exprs.append(args[pos]); pos += 1
            elif re.fullmatch(r'\w+', h):
                exprs.append(h)
            else:
                ok = False; break
        if not ok or len(exprs) != len(names) or set(names) != required:
            continue
        pairs = ', '.join(f'("{n}", crate::i18n::Arg::from(&{e}))' for n, e in zip(names, exprs))
        original = src[m.start():end + 1]
        out.append(src[last:m.start()])
        out.append(f'crate::i18n::tf("{key}", &{original}, &[{pairs}])')
        last = end + 1
        count += 1
    if out:
        out.append(src[last:])
        open(f, 'w').write(''.join(out))
print(count, 'templates translated')
