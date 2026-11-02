"""Translate game.dll's decompiled save (vf14) and own-field (vf18) functions into
the formats crate's layout strings.  T marks a timer whose kind (plain "t" or
extended "t ss f") is settled by fitting against the original's records (fitall.py).

    W3_DECOMP=<game4.c> python3 xlate.py FUN_<own> ...   # print own and save layouts"""
import os, re, sys

# Ghidra's C export of game.dll, each function headed "// ==== <name> @ <address>".
DECOMP = os.environ.get('W3_DECOMP', 'game4.c')

PRIM = {
    'FUN_6f06b3a0': 'w', 'FUN_6f06b280': 'w', 'FUN_6f06b340': 'w', 'FUN_6f06b2e0': 'w',
    'FUN_6f06f9b0': 'f', 'FUN_6f06b220': 's', 'FUN_6f06bc20': 'z',
    'FUN_6f21b520': 'r', 'FUN_6f3e2ce0': 'r', 'FUN_6f21b4d0': '[ww]',
    'FUN_6f224e20': 'wwwww f w fff w f [ww] f f f [w] f s',
}
# Own-field bases (vf18) and save bases (vf14): their layouts.  A save base's is
# the tail after the handler lists.
OWN_BASE = {
    'FUN_6f42e830': 'wwww wwwwwww t w',
    'FUN_6f42f4b0': 'wwww wwwwwww t w ffff wwww f',
    'FUN_6f42f3e0': 'wwww wwwwwww t w ffff wwww f wwww ff wwww ff',
    'FUN_6f532960': 'wwww wwwwwww t w w f www fff',
    'FUN_6f42f5c0': 'wwww wwwwwww t w w f www fff f www',
}
SAVE_BASE = {
    'FUN_6f0562b0': '',
    'FUN_6f42d5f0': 'wwwww',
    'FUN_6f42e2b0': 'wwwww r t t',
    'FUN_6f42d2a0': 'wwwww r t t t',
    'FUN_6f03eda0': '',
}

_bodies = None
def bodies():
    global _bodies
    if _bodies is None:
        src = open(DECOMP, errors='replace').read()
        _bodies = {}
        for m in re.finditer(r'// ==== (\S+) @ ([0-9a-f]+)\n(.*?)(?=\n// ==== |\Z)', src, re.S):
            _bodies[m.group(2)] = (m.group(1), m.group(3))
    return _bodies

def body_of(name):
    addr = name.split('_')[-1]
    return bodies().get(addr, (None, None))[1]

def strip_gates(b):
    # The CSaveGame.h version holders' allocation: no writes in them.
    b = re.sub(r'if \(DAT_[0-9a-f]+ == \([^)]*\)0x0\) \{\s*DAT_[0-9a-f]+ = \([^)]*\)\s*Ordinal_401\(.*?\n\s*\}\s*\n\s*\}', '', b, flags=re.S)
    return b

REF_PAT = re.compile(
    r'(\w+) = \*\(\w+ \*\)\([^;]*\);\s*'
    r'(?:\w+\s*=\s*[^;]*;\s*)?'
    r'if \(\1 == 0\) \{\s*FUN_6f06b3a0\(\w+,0xffffffff\);\s*(?:\w+ = 0xffffffff;|FUN_6f06b3a0\(\w+,0xffffffff\);\s*return(?: \w+)?;)\s*\}\s*'
    r'(?:else \{\s*)?FUN_6f06b3a0\(\w+,\*\(\w+ \*\)\(\1 \+ (?:8|0x10|0xc)\)\);\s*(?:\w+ = \*\(\w+ \*\)\(\1 \+ (?:0xc|0x10)\);\s*\}\s*FUN_6f06b3a0\(\w+,\w+\);|FUN_6f06b3a0\(\w+,\*\(\w+ \*\)\(\1 \+ (?:0xc|0x10)\)\);(?:\s*return(?: \w+)?;)?)', re.S)

# The same written the other way: present, its two words and return; else none.
REF_PAT2 = re.compile(
    r'(\w+) = \*\(\w+ \*\)\([^;]*\);\s*'
    r'if \(\1 != 0\) \{\s*FUN_6f06b3a0\(\w+,\*\(\w+ \*\)\(\1 \+ (?:8|0xc|0x10)\)\);\s*FUN_6f06b3a0\(\w+,\*\(\w+ \*\)\(\1 \+ (?:0xc|0x10|0x14)\)\);\s*return(?: \w+)?;\s*\}\s*'
    r'FUN_6f06b3a0\(\w+,0xffffffff\);\s*FUN_6f06b3a0\(\w+,0xffffffff\);', re.S)

def risky(b):
    """Whether a body still branches around writes after the forms understood: then
    its translation is not trusted."""
    for m in re.finditer(r'\bif \(', b):
        # A gate's version check, or a loop's guard, writes once whatever the branch.
        seg = b[m.start():m.start() + 400]
        if re.match(r'if \(\(\*puVar\d+ <= uVar\d+\)', seg) or re.match(r'if \(\w+ != 0\) \{\s*(?:\w+ = [^;]*;\s*)*do \{', seg) or re.match(r'if \(\(\*\w+ <= \w+\)', seg):
            continue
        body = seg[:seg.find('}') + 1] if '}' in seg else seg
        if re.search(r'FUN_6f06b|FUN_6f06f9b0|0x20\)\)', body):
            return True
    return False

def translate(name, kind, depth=0):
    """The layout a function writes: kind 'own' (fields before the lists) or 'save'
    (the tail after them).  None where it holds what is not understood."""
    if depth > 4:
        return None
    if kind == 'own' and name in OWN_BASE:
        return OWN_BASE[name]
    if kind == 'save' and name in SAVE_BASE:
        return SAVE_BASE[name]
    b = body_of(name)
    if b is None:
        return None
    b = b[b.index('{') + 1:] if '{' in b else b
    b = strip_gates(b)
    b = REF_PAT.sub(' __REF__; ', b)
    b = REF_PAT2.sub(' __REF__; ', b)
    if risky(b):
        return None
    out = []
    # Tokens in order: calls, timer vcalls, the handler loop, loops.
    tok = re.compile(
        r'(?P<ref>__REF__)'
        r'|(?P<hloop>\(\*\*\(code \*\*\)\(\*\(int \*\)\w+ \+ 0x50\)\))'
        r'|(?P<timer>\(\*\*\(code \*\*\)\((?:\*\(int \*\)\((?P<toff>[^;]*?)\)|\w+\[-?0x[0-9a-f]+\]|\*\w+) \+ 0x20\)\))'
        r'|(?P<call>(?<![\w*])(?P<fn>FUN_[0-9a-f]{8}|[A-Za-z_:]+::vf\d+_[0-9a-f]{8})\()'
        r'|(?P<do>do \{)|(?P<while>\} while \((?P<cond>[^;]*)\);)'
        r'|(?P<vcall>\(\*\*\(code \*\*\)[^;]*\)\()')
    stack = [[]]
    counts = []
    started = kind == 'own'
    for m in tok.finditer(b):
        cur = stack[-1]
        if m.group('ref'):
            cur.append('r')
        elif m.group('hloop'):
            if kind == 'save':
                # (Inside its do: the loop's level kept to be dropped at its while.)
                stack = [[], ['HLOOP']]; started = True
        elif m.group('timer'):
            off = re.search(r'\+ (0x[0-9a-f]+)$', m.group('toff') or '')
            cur.append(f'T@{off.group(1)}' if off else 'T')
        elif m.group('call'):
            fn = m.group('fn')
            if fn in PRIM:
                cur.append(PRIM[fn])
            elif kind == 'own' and fn in OWN_BASE:
                cur.append(OWN_BASE[fn])
            elif kind == 'save' and fn in SAVE_BASE:
                stack = [[SAVE_BASE[fn]]] if SAVE_BASE[fn] else [[]]; started = True
            elif fn in ('FUN_6f30f850', 'FUN_6f06e8d0', 'Ordinal_401', 'FUN_6f055cc0'):
                pass
            else:
                sub = translate(fn, kind, depth + 1)
                if sub is None:
                    return None
                if kind == 'save' and fn not in SAVE_BASE and sub is not None and depth == 0 and not started:
                    # A base save of the class's own: its tail begins the tail.
                    stack = [[sub]] if sub else [[]]; started = True
                else:
                    cur.append(sub)
        elif m.group('do'):
            stack.append([])
        elif m.group('while'):
            if len(stack) < 2:
                return None
            level = stack.pop()
            if level[:1] == ['HLOOP'] or '0xd0400' in m.group('cond'):
                continue
            inner = ' '.join(x for x in level if x)
            cond = m.group('cond')
            prev = stack[-1]
            # A loop over a count written just before: a counted list.
            if prev and prev[-1] == 'w' and not re.search(r'0xd0400|< 0x', cond):
                prev[-1] = '[' + inner + ']'
            else:
                mm = re.search(r'local_\w+ = (0x[0-9a-f]+|\d+);', b)
                if mm and inner:
                    prev.append(f'{int(mm.group(1), 0)}(' + inner + ')')
                else:
                    return None
        elif m.group('vcall'):
            return None
    return ' '.join(x for x in stack[0] if x)

def constructor_timers(cls):
    """The timers a class's constructors (its own and the bases' it calls) put at each
    offset: 't' a CAgentTimer, 't ss f' a CAgentTimerExtended."""
    out = {}
    src_by = bodies()
    ctors = [name for name, (fn, b) in src_by.items() if re.search(r'= ' + re.escape(cls) + r'::vftable;', b) and ('*param_1 = ' + cls in b or '*(undefined ***)this = ' + cls in b)]
    seen = set()
    def scan(addr, depth):
        if addr in seen or depth > 4 or addr not in src_by:
            return
        seen.add(addr)
        b = src_by[addr][1]
        for m in re.finditer(r'param_1\[(0x[0-9a-f]+|\d+)\] = (CAgentTimer(?:Extended)?)::vftable', b):
            out.setdefault(int(m.group(1), 0) * 4, 't ss f' if m.group(2).endswith('Extended') else 't')
        for m in re.finditer(r'\(\w+ \*\)\(\(int\)\w+ \+ (0x[0-9a-f]+)\) = (CAgentTimer(?:Extended)?)::vftable', b):
            out.setdefault(int(m.group(1), 0), 't ss f' if m.group(2).endswith('Extended') else 't')
        for m in re.finditer(r'(FUN_[0-9a-f]{8})\(param_1', b):
            scan(m.group(1)[4:], depth + 1)
    for c in ctors:
        scan(c, 0)
    return out

def resolve(layout, cls):
    """The T marks with offsets settled by the class's constructors; the rest left."""
    timers = constructor_timers(cls)
    def sub(m):
        off = int(m.group(1), 16)
        return timers.get(off, 'T')
    return re.sub(r'T@(0x[0-9a-f]+)', sub, layout)

def expand(layout):
    """Every reading of the T marks."""
    parts = layout.split('T')
    n = len(parts) - 1
    outs = []
    for mask in range(1 << n):
        s = parts[0]
        for i in range(n):
            s += ('t ss f' if mask >> i & 1 else 't') + parts[i + 1]
        outs.append(re.sub(r'\s+', ' ', s).strip())
    return outs

if __name__ == '__main__':
    for nm in sys.argv[1:]:
        print(nm, translate(nm, 'own'), '|', translate(nm, 'save'))
