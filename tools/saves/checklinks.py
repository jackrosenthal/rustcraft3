"""Check the relation lists in a save's first records (each object's two link pairs:
previous, next, itself, owner): each neighbour naming it back, no cycles, each list
under one owner; and the owners' heads (an agent's four, a property's two): each
naming the first of a list it owns, and each list's first named by its owner.

    w3zobj <save.w3z> dumpall > dump.txt
    python3 checklinks.py dump.txt
"""
import re, sys, collections

LINK = r'\(Link\(Ref\((\d+), (\d+)\), (\d+)\), Link\(Ref\((\d+), (\d+)\), (\d+)\), Ref\((\d+), (\d+)\), Ref\((\d+), (\d+)\)\)'
NONE = 4294967295
objs = {}
heads = {}
HEAD = r'links: \[((?:Link\(Ref\(\d+, \d+\), \d+\)(?:, )?)+)\]'
for ln in open(sys.argv[1]):
    head, _, rest = ln.partition(' | ')
    hp = head.split()
    if len(hp) < 3:
        continue
    m = re.search(r'links: \[' + LINK + ', ' + LINK + r'\]', rest)
    if not m:
        continue
    g = [int(x) for x in m.groups()]
    tag = int(hp[0])
    pairs = []
    for k in (0, 10):
        prev, nxt, me, owner = (g[k], g[k + 1]), (g[k + 3], g[k + 4]), (g[k + 6], g[k + 7]), (g[k + 8], g[k + 9])
        pairs.append((prev, nxt, owner))
    objs[tag] = (hp[1], int(hp[-1]), pairs)
    h = re.search(HEAD, rest)
    if h:
        heads[tag] = [((int(a), int(b)), int(k)) for a, b, k in re.findall(r'Link\(Ref\((\d+), (\d+)\), (\d+)\)', h.group(1))]

bad = collections.Counter()
examples = {}
for tag, (kind, birth, pairs) in objs.items():
    for j, (prev, nxt, owner) in enumerate(pairs):
        for side, other, back in (('next', nxt, 0), ('prev', prev, 1)):
            if other[0] == NONE:
                continue
            o = objs.get(other[0])
            if o is None or o[1] != other[1]:
                key = (kind, j, side, 'missing')
            else:
                op = o[2][j]
                mine = op[back]
                key = None
                if mine != (tag, birth):
                    key = (kind, j, side, f'not named back by {o[0]}')
                elif op[2] != owner:
                    key = (kind, j, side, f'other owner {o[0]}')
            if key:
                bad[key] += 1
                examples.setdefault(key, (tag, other))
        # A cycle along next.
        seen = {tag}
        cur = nxt
        steps = 0
        while cur[0] != NONE and steps < 10000:
            if cur[0] in seen:
                key = (kind, j, 'cycle', '')
                bad[key] += 1
                examples.setdefault(key, (tag, cur))
                break
            seen.add(cur[0])
            o = objs.get(cur[0])
            if not o:
                break
            cur = o[2][j][1]
            steps += 1
# The owners' heads.
named = set()
for tag, hs in heads.items():
    me = (tag, objs[tag][1])
    for i, (t, k) in enumerate(hs):
        if t[0] == NONE:
            continue
        o = objs.get(t[0])
        if o is None or o[1] != t[1]:
            key = (objs[tag][0], f'head {i}', 'missing', '')
        else:
            named.add((t[0], k))
            prev, nxt, owner = o[2][k]
            key = None
            if prev[0] != NONE:
                key = (objs[tag][0], f'head {i}', f'names a {o[0]} not first', '')
            elif owner != me:
                key = (objs[tag][0], f'head {i}', f'names a {o[0]} of another owner', '')
        if key:
            bad[key] += 1
            examples.setdefault(key, (tag, t))
for tag, (kind, birth, pairs) in objs.items():
    for j, (prev, nxt, owner) in enumerate(pairs):
        if prev[0] == NONE and owner[0] != NONE and (tag, j) not in named:
            key = (kind, j, 'first not named by its owner', objs.get(owner[0], ('?',))[0])
            bad[key] += 1
            examples.setdefault(key, (tag, owner))
for k, n in sorted(bad.items(), key=lambda kv: -kv[1]):
    print(n, k, examples[k])
print(len(objs), 'objects with links')
