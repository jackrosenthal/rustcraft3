"""Each class's code (its vf7 returning a four-character code), and its save (vf14),
own-field (vf18), and read (vf15) functions, from Ghidra's vtable listing and C export.

    python3 classmap.py <game4.c> <game_vtables.tsv> > classmap.txt
    (lines: code class save own read)"""
import re, sys, collections

src = open(sys.argv[1], errors='replace').read()
bodies = {m.group(2): m.group(3) for m in re.finditer(r'// ==== (\S+) @ ([0-9a-f]+)\n(.*?)(?=\n// ==== |\Z)', src, re.S)}
slots = collections.defaultdict(dict)
for ln in open(sys.argv[2]):
    p = ln.rstrip('\n').split('\t')
    if p[0] == 'SLOT' and len(p) >= 4:
        slots[p[1]].setdefault(int(p[2]), p[3])
out = []
for cls, s in slots.items():
    f7 = s.get(7)
    m = re.search(r'\{\s*return (0x[0-9a-f]{8});\s*\}', bodies.get(f7, ''))
    if m:
        code = int(m.group(1), 16).to_bytes(4, 'big').decode('latin-1')
        out.append((code, cls, s.get(14, '-'), s.get(18, '-'), s.get(15, '-')))
for x in sorted(out):
    print(' '.join(x))
