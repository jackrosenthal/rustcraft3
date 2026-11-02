"""Generate crates/game/src/w3zsave/kinds.rs: each fitted kind's own fields past the
ability base and its tail, from one of the original's records of it at rest (read
with the layouts fitall.py fitted, the formats crate holding them).

    w3zobj <save.w3z> dumpall > dump.txt
    python3 gentemplates.py dump.txt <fitted.txt> crates/game/src/w3zsave/kinds.rs
"""
import os, re, sys

S = os.path.dirname(os.path.abspath(__file__))
dump, fitted, out_path = sys.argv[1], sys.argv[2], sys.argv[3]
codes = [ln.split('\t')[0] for ln in open(fitted) if ln.strip()]
live = {}
kind_of = {}
recs = {}
for ln in open(dump):
    head, _, rest = ln.partition(' | ')
    hp = head.split()
    if len(hp) < 3: continue
    t, k, b = hp[0], hp[1], hp[-1]
    rest = '| ' + rest
    live[int(t)] = int(b)
    kind_of[int(t)] = k
    if k in codes and k not in recs and ' | Ok(Agent { own: ' in rest:
        recs[k] = rest.split(' | ')[-1].strip()

class P:
    def __init__(s, txt): s.t = txt; s.i = 0
    def peek(s, w): return s.t.startswith(w, s.i)
    def eat(s, w):
        assert s.t.startswith(w, s.i), (w, s.t[s.i:s.i+40]); s.i += len(w)
    def ws(s):
        while s.i < len(s.t) and s.t[s.i] in ' ,': s.i += 1
    def num(s):
        m = re.compile(r'-?[0-9.e+\-]+|NaN|inf|-inf').match(s.t, s.i); s.i = m.end(); return m.group(0)
    def field(s):
        s.ws()
        for name in ('Word', 'Real', 'Byte', 'Short'):
            if s.peek(name + '('):
                s.eat(name + '('); v = s.num(); s.eat(')'); return (name, v)
        if s.peek('Ref(Ref('):
            s.eat('Ref(Ref('); a = s.num(); s.eat(', '); b = s.num(); s.eat('))'); return ('Ref', int(a), int(b))
        if s.peek('Timer(Ref('):
            s.eat('Timer(Ref('); a = s.num(); s.eat(', '); b = s.num(); s.eat('), '); k = s.num(); s.eat(', Timing(')
            if s.peek('None'):
                s.eat('None))'); return ('Timer', int(k), None)
            s.eat('Some(('); vals = []
            for j in range(5):
                s.ws(); vals.append(s.num())
            s.eat('))))'); return ('Timer', int(k), vals)
        if s.peek('List(['):
            s.eat('List(['); groups = []
            while not s.peek('])'):
                s.ws(); s.eat('['); g = []
                while not s.peek(']'):
                    g.append(s.field()); s.ws()
                s.eat(']'); groups.append(g); s.ws()
            s.eat('])'); return ('List', groups)
        if s.peek('Text("'):
            s.eat('Text("'); j = s.t.index('")', s.i); v = s.t[s.i:j]; s.i = j + 2; return ('Text', v)
        raise ValueError(s.t[s.i:s.i + 60])
    def list_(s):
        s.eat('['); out = []
        while not s.peek(']'):
            out.append(s.field()); s.ws()
        s.eat(']'); return out

def parse(rec):
    m = re.search(r'own: (\[.*?\]), handlers: ', rec)
    own = P(m.group(1)).list_()
    j = rec.index('tail: ') + len('tail: ')
    tail = P(rec[j:]).list_()
    return own, tail

def clean_pairs(fields):
    """Words holding a tag and its birth (stale in another save): none."""
    out = []
    i = 0
    while i < len(fields):
        f = fields[i]
        if f[0] == 'Word' and i + 1 < len(fields) and fields[i + 1][0] == 'Word':
            a, b = int(f[1]), int(fields[i + 1][1])
            if 0 < a < 0x80000000 and a in live and live[a] == b and a > 1000:
                out += [('Word', str(0xffffffff)), ('Word', str(0xffffffff))]; i += 2; continue
        if f[0] == 'List':
            f = ('List', [clean_pairs(g) for g in f[1]])
        out.append(f); i += 1
    return out

def real(v):
    if v == 'NaN': return 'f32::NAN'
    if v in ('inf',): return 'f32::INFINITY'
    if v in ('-inf',): return 'f32::NEG_INFINITY'
    if v == '3.4028235e38': return 'f32::MAX'
    if v == '-3.4028235e38': return '-f32::MAX'
    x = v if ('.' in v or 'e' in v) else v + '.0'
    return x + '_f32' if 'e' not in x else f'{x}_f32'

def rust(f, listener_ok):
    k = f[0]
    if k == 'Ref' and not listener_ok and f[1] != 0xffffffff and f[1] < 0x80000000 and live.get(f[1]) == f[2]:
        kind = kind_of.get(f[1], '')
        if len(kind) == 4:
            return f'companion(*b"{kind}")'
    if k == 'List' and f[1] and all(len(g) == 1 and g[0][0] == 'Ref' for g in f[1]) and any(g[0][1] != 0xffffffff for g in f[1]):
        # (Art and other live references in a list: none, as a fresh save's.)
        return 'Field::List(Vec::new())'
    if k == 'Word': return f'w({int(f[1])})'
    if k == 'Real': return f'Field::Real({real(f[1])})'
    if k == 'Byte': return f'Field::Byte({f[1]})'
    if k == 'Short': return f'Field::Short({f[1]})'
    if k == 'Ref': return 'LISTENER' if listener_ok else 'Field::Ref(NONE)'
    if k == 'Timer':
        if f[2] is not None and int(float(f[2][4])) == 1:
            return f'periodic({real(f[2][1])}, {int(float(f[2][3]))}, {f[1]})'
        return 'timer()'
    if k == 'List': return 'Field::List(vec![' + ', '.join('vec![' + ', '.join(rust(x, False) for x in g) + ']' for g in f[1]) + '])'
    if k == 'Text': return f'Field::Text({f[1]!r}.replace("\\\'", "\'").into())'.replace("'", '"') if False else 'Field::Text(String::from(' + '"' + f[1].replace('\\', '\\\\').replace('"', '\\"') + '"))'
    raise ValueError(f)

lines = ['//! Generated by tools/saves/gentemplates.py from the original\'s records (read with the layouts',
         '//! fitted to them): each kind\'s own fields past the ability\'s thirteen and its tail,',
         '//! as a unit made at rest has them.  A spell\'s mana listener is filled in, a',
         '//! repeating timer restarted from the save\'s time; other references and timers none.',
         '', 'use crate::codes;', 'use w3_formats::w3z::Field;', '',
         '/// A template\'s fields: `listener` the spell\'s listener, `timer` an empty timer,',
         '/// `periodic(period, event, kind)` a repeating one, `companion(kind)` the unit\'s object',
         '/// of that kind made with the ability.',
         'pub(super) fn kind_template(code: &[u8; 4], listener: Field, timer: &dyn Fn() -> Field, periodic: &dyn Fn(f32, u32, u32) -> Field, companion: &dyn Fn([u8; 4]) -> Field) -> Option<(Vec<Field>, Vec<Field>)> {',
         '    let w = Field::Word;',
         '    let _ = &listener;',
         '    let NONE = w3_formats::w3z::Ref(u32::MAX, u32::MAX);',
         '    let LISTENER = listener;',
         '    Some(match code {']
for code in sorted(recs):
    own, tail = parse(recs[code])
    if code.startswith('B'):
        own_extra = own[13:]
    else:
        own_extra = own[13:]
    own_extra = clean_pairs(own_extra); tail = clean_pairs(tail)
    spell_listener = len(tail) > 5 and all(t[0] == 'Word' for t in tail[:5]) and tail[5][0] == 'Ref' and int(tail[0][1]) == 852290
    o = ', '.join(rust(x, False) for x in own_extra)
    tl = ', '.join(rust(x, i == 5 and spell_listener) for i, x in enumerate(tail))
    lines.append(f'        b"{code}" => (vec![{o}], vec![{tl}]),')
lines += ['        _ => return None,', '    })', '}', '']
src = '\n'.join(lines)
src = src.replace('LISTENER', 'listener.clone()').replace('let listener.clone() = listener;', '').replace('let NONE', 'let none')
src = src.replace('Field::Ref(NONE)', 'Field::Ref(none)')
# The codes by their names in codes.rs.
sys.path.insert(0, S)
import codenames
src = codenames.rewrite(src, codenames.names())
open(out_path, 'w').write(src)
print(len(recs), 'kinds')
