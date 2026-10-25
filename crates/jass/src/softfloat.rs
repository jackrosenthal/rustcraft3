//! The original's software floats.  JASS reals are added, multiplied, divided, parsed, and
//! printed by game.dll's own float routines, which truncate where IEEE arithmetic rounds
//! and divide by a tabled reciprocal (measured: 1.1 * 1.1 prints 1.209999680 there, and
//! R2S(0.9995) prints 1.000).  These work on the bit patterns as those routines do.

use std::sync::OnceLock;

static TABLE: OnceLock<Box<[u32; 1025]>> = OnceLock::new();

/// Uses game.dll's reciprocal table, found by its first entries; true if found.  Without
/// it, division uses a computed table that is off by one in the last bit of some entries.
pub fn load_reciprocal_table(dll: &[u8]) -> bool {
    const HEAD: [u32; 4] = [0x0080_0000, 0x007f_c011, 0x007f_8041, 0x007f_4091];
    let head: Vec<u8> = HEAD.iter().flat_map(|v| v.to_le_bytes()).collect();
    let Some(at) = dll.windows(head.len()).position(|w| w == head) else { return false };
    let Some(bytes) = dll.get(at..at + 1025 * 4) else { return false };
    let mut t = Box::new([0u32; 1025]);
    for (i, c) in bytes.chunks_exact(4).enumerate() {
        t[i] = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
    }
    let same = TABLE.get().is_some_and(|have| **have == *t);
    TABLE.set(t).is_ok() || same
}

fn table() -> &'static [u32; 1025] {
    TABLE.get_or_init(|| {
        let mut t = Box::new([0u32; 1025]);
        for (i, v) in t.iter_mut().enumerate() {
            let (n, d) = ((1u64 << 23) * (1024 - i as u64), 1024 + i as u64);
            *v = ((2 * n + d) / (2 * d)) as u32;
        }
        t
    })
}

fn sar(x: u32, n: u32) -> u32 {
    ((x as i32) >> (n & 31)) as u32
}

/// Multiplies (FUN_6f06f9c0): the mantissas' product truncated.
pub fn mul(a: u32, b: u32) -> u32 {
    let sign = (a ^ b) & 0x8000_0000;
    let (ea, eb) = (a & 0x7f80_0000, b & 0x7f80_0000);
    let (ma, mb) = (a & 0x7f_ffff, b & 0x7f_ffff);
    if ma != 0 && mb != 0 {
        let l = (((mb | 0xff80_0000) << 8) as u64) * (((ma | 0xff80_0000) << 8) as u64);
        let hi = (l >> 32) as u32;
        let top = (l >> 63) as u32;
        let e = ea.wrapping_add(0xc080_0000).wrapping_add(eb);
        let carry: u32 = if hi & 0x8000_0000 != 0 { 0x80_0000 } else { 0 };
        let r = ((hi >> top) >> 7 & 0x7f_ffff) | carry.wrapping_add(e) | sign;
        return r & !sar(e.wrapping_sub(0x80_0000), 31);
    }
    if ea != 0 && eb != 0 {
        let e = ea.wrapping_add(eb).wrapping_add(0xc080_0000);
        return (e | ma | mb | sign) & !sar(e.wrapping_sub(0x80_0000), 31);
    }
    0
}

fn add_bits(a: u32, b: u32) -> u32 {
    let ea = a & 0x7f80_0000;
    if ea == 0 {
        return b;
    }
    let eb = b & 0x7f80_0000;
    if eb == 0 {
        return a;
    }
    let signed = |x: u32| ((((x & 0x7f_ffff) | 0x80_0000) * 2) ^ sar(x, 31)).wrapping_sub(sar(x, 31)) as i32;
    let (mut ma, mut mb) = (signed(a), signed(b));
    let d = eb.wrapping_sub(ea) as i32;
    let mut e = ea;
    if d < 1 {
        if d < -0xb7f_ffff {
            return a;
        }
        mb >>= (ea.wrapping_sub(eb) >> 23) & 31;
    } else {
        if d > 0xb7f_ffff {
            return b;
        }
        ma >>= (d as u32 >> 23) & 31;
        e = eb;
    }
    let s = mb.wrapping_add(ma) as u32;
    if s == 0 {
        return 0;
    }
    let mag = (s ^ sar(s, 31)).wrapping_sub(sar(s, 31));
    let z = mag.leading_zeros() as i32;
    let k = 8 - z;
    let v = if k < 0 { mag << (-k) } else { mag >> k };
    ((7 - z) as u32).wrapping_mul(0x80_0000).wrapping_add(e) | (v & 0x7f_ffff) | (s & 0x8000_0000)
}

/// Adds (FUN_6f06fbb0): the smaller aligned by shifting, its lost bits dropped.
pub fn add(a: u32, b: u32) -> u32 {
    add_bits(a, b)
}

/// Subtracts (FUN_6f06fa90).
pub fn sub(a: u32, b: u32) -> u32 {
    add_bits(a, b ^ 0x8000_0000)
}

/// The reciprocal (FUN_6f0711e0), interpolated in the table.
pub fn recip(b: u32) -> u32 {
    let t = table();
    let i = ((b & 0x7f_ffff) >> 13) as usize;
    let (t0, t1) = (t[i], t[i + 1]);
    let x = ((((b & 0x1fff) << 6) | (b << 19)) >> 13) | (b << 19);
    let i3 = t0.wrapping_sub(((t0.wrapping_sub(t1) as u64 * x as u64) >> 32) as u32).wrapping_sub(b & 0x7f80_0000);
    !sar(i3.wrapping_add(0x7e00_0000), 31) & ((b & 0x8000_0000) | i3.wrapping_add(0x7e80_0000))
}

/// Divides (FUN_6f06fcd0): by multiplying by the reciprocal; a number by itself is 1.
pub fn div(a: u32, b: u32) -> u32 {
    if a == b {
        return 0x3f80_0000;
    }
    mul(a, recip(b))
}

/// An integer as a real (FUN_6f070d80), its low bits dropped.
pub fn itof(n: i32) -> u32 {
    let n = n as u32;
    if n == 0 {
        return 0;
    }
    let a = (n ^ sar(n, 31)).wrapping_sub(sar(n, 31));
    let z = a.leading_zeros() as i32;
    let k = z - 8;
    let v = if k < 0 { a >> (-k) } else { a << k };
    ((0x9e - z) as u32).wrapping_mul(0x80_0000) | (v & 0x7f_ffff) | (n & 0x8000_0000)
}

/// A real as an integer, toward zero (FUN_6f070120).
pub fn ftoi(b: u32) -> i32 {
    let e = (b >> 23) & 0xff;
    if e < 0x7f {
        return 0;
    }
    let m = (b & 0x7f_ffff) | 0x80_0000;
    let k = 0x96 - e as i32;
    let s = sar(b, 31);
    let v = if k >= 0 { m.checked_shr(k as u32).unwrap_or(0) } else { m.checked_shl((-k) as u32).unwrap_or(0) };
    (s ^ v).wrapping_sub(s) as i32
}

/// Rounds down (FUN_6f070c80).
pub fn floor(b: u32) -> u32 {
    let e = ((b >> 23) & 0xff) as i32 - 0x7f;
    if e < 0 {
        return if (b << 1) & sar(b, 31) != 0 { 0xbf80_0000 } else { 0 };
    }
    if e < 23 {
        let mut m = sar(0x8000_0000, ((b >> 23).wrapping_add(0x89)) & 31);
        if (b << 1) & !sar(b, 31) != 0 {
            return b & m;
        }
        m &= (b & 0x7f_ffff).wrapping_add(!m);
        return (((m >> 23) as i32 + 0x7f + e) as u32).wrapping_mul(0x80_0000) | (m & 0x7f_ffff) | 0x8000_0000;
    }
    b
}

/// What lies beyond the whole part (FUN_6f070d20).
fn fraction(b: u32) -> u32 {
    let e = ((b >> 23) & 0xff) as i32 - 0x7f;
    if e < 0 {
        return b;
    }
    if e < 23 {
        return sub(b, sar(0x8000_0000, (e + 8) as u32) & b);
    }
    0
}

/// Ten to a power (FUN_6f071180).
fn pow10(mut n: i32) -> u32 {
    let (mut r, mut x) = (0x3f80_0000, 10f32.to_bits());
    while n != 0 {
        if n & 1 != 0 {
            r = mul(r, x);
        }
        x = mul(x, x);
        n >>= 1;
    }
    r
}

/// Reads a real as S2R does (FUN_6f070de0): a sign, then digits and one point up to a
/// space, semicolon, tab, or anything else; nine significant digits, the rest only
/// scaling the whole part.
pub fn parse(s: &str) -> u32 {
    let b = s.as_bytes();
    let (mut i, mut sign) = (0, 1i32);
    match b.first() {
        Some(b'-') => (i, sign) = (1, -1),
        Some(b'+') => i = 1,
        _ => {}
    }
    let (mut mant, mut sig, mut started, mut scale, mut dot, mut step) = (0i32, 0, false, 0i32, false, 0i32);
    while let Some(&c) = b.get(i) {
        i += 1;
        if c == b' ' || c == b';' || c == b'\t' {
            break;
        }
        if c.is_ascii_digit() {
            started = started || c != b'0';
            sig += started as i32;
            if sig <= 9 {
                scale += step;
                mant = mant.wrapping_mul(10).wrapping_add((c - b'0') as i32);
            } else {
                if sig == 10 {
                    step -= 1;
                }
                scale += step;
            }
        } else if c == b'.' && !dot {
            dot = true;
            step += 1;
        } else {
            break;
        }
    }
    let v = itof(mant.wrapping_mul(sign));
    if scale >= 0 { div(v, pow10(scale)) } else { mul(v, pow10(-scale)) }
}

/// A real literal in a script: its whole part as a wrapping integer, plus its fraction
/// as S2R reads it (measured: 3.14159 is 0x40490fcf, where S2R reads 0x40490fd1).
pub fn literal(whole: i32, frac: &str) -> u32 {
    let w = itof(whole);
    if frac.is_empty() { w } else { add(w, parse(&format!("0.{frac}"))) }
}

/// Prints a real as R2SW does (FUN_6f0701d0): a minus sign apart; the whole part padded
/// to the width less the precision (at least 1); the fraction to the precision (at most
/// 9; a negative one is 6 with trailing zeros dropped down to one); and whole parts of
/// 2^31 or more as their leading digits and zeros.
pub fn format(b: u32, width: i32, precision: i32) -> String {
    let mut out = String::new();
    if (b << 1) & sar(b, 31) != 0 {
        out.push('-');
    }
    let a = b & 0x7fff_ffff;
    if a & 0x7f80_0000 >= 0x4f00_0000 {
        if f32::from_bits(a) < f32::MAX {
            let mut n = 0;
            let mut x = a;
            loop {
                n += 1;
                x = div(x, 10f32.to_bits());
                if x < 0x4f00_0000 {
                    break;
                }
            }
            out += &format!("{}{}.{}", ftoi(x), "0".repeat(n), "0".repeat(precision.max(1) as usize));
            return out;
        }
        out += "inf";
        return out;
    }
    let trim = precision < 0;
    let mut p = if trim { 6 } else { precision.min(9) };
    let mut whole = ftoi(a);
    let pw = 10i32.pow(p as u32);
    let mut f = ftoi(floor(add(mul(fraction(a), itof(pw)), 0.5f32.to_bits())));
    if pw <= f {
        whole += 1;
        f -= pw;
    }
    if trim {
        while p > 1 && f % 10 == 0 {
            p -= 1;
            f /= 10;
        }
    }
    let w = (width - p).max(1) as usize;
    out += &format!("{whole:>w$}.{f:0p$}", p = p as usize);
    out
}

const ONE: u32 = 0x3f80_0000;
const HALF_PI: u32 = 0x3fc9_0fdb;
const PI: u32 = 0x4049_0fdb;

/// sin(i/1024 of a right angle) in 31 bits, toward zero (game.dll's table at 0x6fa820c8).
fn sine_table() -> &'static [i32; 1025] {
    static T: OnceLock<Box<[i32; 1025]>> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = Box::new([0i32; 1025]);
        for (i, v) in t.iter_mut().enumerate() {
            *v = ((i as f64 * std::f64::consts::PI / 2048.0).sin() * 2f64.powi(31)).floor().min(i32::MAX as f64) as i32;
        }
        t
    })
}

/// Sine or cosine (FUN_6f071280, FUN_6f070790): the turn in 2^20 parts, read from the
/// quarter-wave table between neighbors.
fn sin_cos(x: u32, cos: bool) -> u32 {
    let t = sine_table();
    let n = ftoi(mul(x, 0x4822_f983));
    let q = ((n >> 18) + cos as i32) & 3;
    let i = ((n >> 8) & 0x3ff) as usize;
    let f = (n & 0xff) as u32 * 0x0101_0101;
    let r = if q & 1 == 0 {
        let (a, b) = (t[i], t[i + 1]);
        a.wrapping_add(((b.wrapping_sub(a) as u32 as u64 * f as u64) >> 32) as i32)
    } else {
        let (a, c) = (t[1024 - i], t[1023 - i]);
        a.wrapping_sub(((a.wrapping_sub(c) as u32 as u64 * f as u64) >> 32) as i32)
    };
    let r = if q & 2 != 0 { r.wrapping_neg() } else { r };
    let v = itof(r);
    if v & 0x7f80_0000 != 0 { v.wrapping_sub(0x0f80_0000) } else { v }
}

pub fn sin(x: u32) -> u32 {
    sin_cos(x, false)
}

pub fn cos(x: u32) -> u32 {
    sin_cos(x, true)
}

/// Tangent: the sine over the cosine (FUN_6f071590).
pub fn tan(x: u32) -> u32 {
    div(sin(x), cos(x))
}

/// The integer square root of 32 bits (FUN_6f071530).
fn isqrt(mut p: u32) -> u32 {
    let (mut rem, mut root) = (0u32, 0u32);
    let mut two = 0;
    for _ in 0..16 {
        two = root.wrapping_mul(2);
        rem = rem.wrapping_mul(4) | (p >> 30);
        p <<= 2;
        if two < rem {
            rem = rem.wrapping_sub(root.wrapping_mul(2)).wrapping_sub(1);
            two = two.wrapping_add(2);
        }
        root = two;
    }
    two >> 1
}

/// Square root (FUN_6f071480): of the mantissa by integers, scaled, times half the
/// exponent's power of two (and the square root of 2 for an odd exponent).
pub fn sqrt(b: u32) -> u32 {
    if (b << 1) & !sar(b, 31) == 0 {
        return 0;
    }
    let m = b & 0x7f_ffff;
    let e = ((b >> 23) & 0xff) as i32 - 0x7f;
    let s = isqrt(((m | 0xff80_0000) << 8) | (m >> 15));
    let a = (s.wrapping_sub(0xb504).wrapping_mul(0xb505) >> 8) | 0x3f80_0000;
    let odd = -(e & 1);
    let half = (e + odd) / 2;
    let p = (((half + 0x7f) as u32) << 23) | (odd as u32 & 0x35_04f3);
    mul(p, a)
}

/// Arctangent (FUN_6f0705b0): beyond 1 by its reciprocal, beyond tan 15 degrees shifted
/// by 30, then a ratio of polynomials.
pub fn atan(x: u32) -> u32 {
    let gt = |a: u32, b: u32| f32::from_bits(a) > f32::from_bits(b);
    let mut a = x & 0x7fff_ffff;
    let inv = gt(a, ONE);
    if inv {
        a = recip(a);
    }
    let big = gt(a, 0x3e89_30a3);
    if big {
        a = div(add(a, 0xbf13_cd3a), add(ONE, mul(0x3f13_cd3a, a)));
    }
    let s = mul(a, a);
    let mut r = div(mul(a, add(0x3f7f_fff0, mul(0x3e84_15a6, s))), add(ONE, mul(0x3f17_592e, s)));
    if big {
        r = add(r, 0x3f06_0a92);
    }
    if inv {
        r = sub(HALF_PI, r);
    }
    if (x << 1) & sar(x, 31) != 0 {
        r ^= 0x8000_0000;
    }
    r
}

/// The angle of (x, y) (FUN_6f070530).
pub fn atan2(y: u32, x: u32) -> u32 {
    let mut r = if x & 0x7f80_0000 != 0 { atan(div(y, x) & 0x7fff_ffff) } else { HALF_PI };
    if (x << 1) & sar(x, 31) != 0 {
        r = sub(PI, r);
    }
    if (y << 1) & sar(y, 31) != 0 {
        r ^= 0x8000_0000;
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    // Measured in the original; the reciprocal table comes from the install beside the
    // repository when it is there.
    #[test]
    fn matches_the_original() {
        let dll = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Warcraft III/game.dll");
        let Ok(bytes) = std::fs::read(dll) else { return };
        assert!(load_reciprocal_table(&bytes));
        assert_eq!(format(parse("0.9995"), 0, 3), "1.000");
        assert_eq!(format(literal(0, "0025"), 0, 3), "0.002");
        assert_eq!(literal(3, "14159"), 0x4049_0fcf);
        assert_eq!(parse("3.14159"), 0x4049_0fd1);
        assert_eq!(format(mul(literal(1, "1"), literal(1, "1")), 1, 9), "1.209999680");
        assert_eq!(format(mul(literal(0, "1"), itof(3)), 1, 9), "0.300000000");
        assert_eq!(format(literal(3, "14159"), 10, 2), "       3.14");
        assert_eq!(format(literal(3, "14159") ^ 0x8000_0000, 2, 0), "- 3.0");
        assert_eq!(format(0.5f32.to_bits(), -3, -1), "0.5");
        assert_eq!(parse(" 2.5"), 0);
        assert_eq!(format(itof(123_456_789), 0, 3), "123456784.000");
        let p = |b: u32| format(b, 1, 9);
        let l = |w: i32, f: &str| literal(w, f);
        assert_eq!([p(sin(l(1, "0"))), p(sin(l(3, "0"))), p(sin(l(2, "0") ^ 0x8000_0000)), p(sin(l(10, "0")))], ["0.841473600", "0.141116768", "-0.909297984", "-0.544023296"]);
        assert_eq!([p(cos(l(1, "0"))), p(cos(l(0, "5"))), p(cos(l(100, "0")))], ["0.540297920", "0.877579776", "0.862317184"]);
        assert_eq!([p(tan(l(0, "5"))), p(tan(l(1, "0")))], ["0.546309760", "1.557425600"]);
        assert_eq!([p(sqrt(l(2, "0"))), p(sqrt(l(1000, "0"))), p(sqrt(l(0, "3")))], ["1.414213536", "31.623027776", "0.547722368"]);
        assert_eq!([p(atan(l(0, "5"))), p(atan(l(3, "0"))), p(atan2(l(1, "0"), l(2, "0"))), p(atan2(l(3, "0") ^ 0x8000_0000, l(1, "0") ^ 0x8000_0000))], ["0.463647616", "1.249045840", "0.463647616", "-1.892546880"]);
    }
}
