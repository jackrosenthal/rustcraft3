"""Names for four-character codes from crates/formats/src/codes.rs, and a rewrite of a
file's raw byte-string codes (b"AEar") into those names (codes::ability::id::...)."""
import os, re, sys

CODES = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', 'crates', 'formats', 'src', 'codes.rs')

def names():
    out = {}
    cur = None
    for ln in open(CODES):
        m = re.match(r'pub mod (\w+) \{', ln)
        if m:
            cur = m.group(1)
        m = re.match(r'\s+([A-Z0-9_]+) = "(.{4})";', ln)
        if m and cur:
            out.setdefault(m.group(2), f'codes::{cur}::id::{m.group(1)}')
        m = re.match(r'\s+pub const ([A-Z0-9_]+): &Code = b"(.{4})";', ln)
        if m and cur:
            out.setdefault(m.group(2), f'codes::{cur}::{m.group(1)}')
    return out

def rewrite(text, n):
    def sub(m):
        star, code = m.group(1), m.group(2)
        if code not in n:
            return m.group(0)
        return (f'*{n[code]}' if star else n[code])
    return re.sub(r'(\*)?b"(.{4})"', sub, text)

if __name__ == '__main__':
    n = names()
    for path in sys.argv[1:]:
        s = open(path).read()
        t = rewrite(s, n)
        open(path, 'w').write(t)
        left = sorted(set(re.findall(r'b"(.{4})"', t)))
        print(path.split('/')[-1], 'left raw:', left)
