"""For each kind, its classes' own and save functions translated (xlate.py) and each
reading of their timers fitted against an original's save with `w3zobj fit`; the
layouts that read every record of the kind exactly, to <fitted.txt>.

    W3_DECOMP=<game4.c> python3 fitall.py <w3zobj> <save.w3z> <classmap.txt> <fitted.txt> "<kinds>"
"""
import os, sys, subprocess, collections
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import xlate
w3zobj, save, classmap, fitted = sys.argv[1:5]
kinds = sys.argv[5].split()
cmap = {}
for ln in open(classmap):
    c, cls, sv, own, rd = ln.split()
    cmap.setdefault(c, (cls, sv, own))
def fit(kind, own, tail):
    out = subprocess.run([w3zobj, save, 'fit', kind, own, tail], capture_output=True, text=True).stdout
    p = out.split()
    return int(p[2]), int(p[4])
results = {}
for k in kinds:
    if k not in cmap:
        print(k, 'NO CLASS'); continue
    cls, sv, own = cmap[k]
    o = xlate.translate('FUN_' + own, 'own'); t = xlate.translate('FUN_' + sv, 'save')
    if o is None or t is None:
        print(k, cls, 'UNTRANSLATED', own, sv, o, '|', t); continue
    found = None
    for oo in xlate.expand(o):
        for tt in xlate.expand(t):
            ok, n = fit(k, oo, tt)
            if n and ok == n:
                found = (oo, tt); break
        if found: break
    if found:
        results[k] = found
        print(k, cls, 'FIT', found[0], '|', found[1])
    else:
        print(k, cls, 'NOFIT', o, '|', t)
open(fitted, 'w').write('\n'.join(f'{k}\t{v[0]}\t{v[1]}' for k, v in sorted(results.items())))
