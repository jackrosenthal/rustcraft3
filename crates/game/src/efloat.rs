//! The engine's own single-precision arithmetic (game.dll's float routines, which its
//! simulation computes with in place of the processor's): integer work on the bits of
//! IEEE singles that truncates where the processor would round, flushes values with a
//! zero exponent to zero, and divides by an interpolated reciprocal.  Values are the
//! singles' bits.

use crate::efloat_recip::RECIP;

const EXP: u32 = 0x7f80_0000;
const FRAC: u32 = 0x007f_ffff;
const SIGN: u32 = 0x8000_0000;

/// Leading zero bits (`FUN_6f06fd30`; 32 for 0).
fn clz(v: u32) -> i32 {
    v.leading_zeros() as i32
}

/// A single's mantissa with its leading one, doubled, signed as the single
/// (`FUN_6f06fbb0`'s operands).
fn signed_mantissa(a: u32) -> i32 {
    let m = (((a & FRAC) | 0x0080_0000) << 1) as i32;
    let s = (a as i32) >> 31;
    (m ^ s).wrapping_sub(s)
}

/// a + b (`FUN_6f06fbb0`): the operand with the smaller exponent shifted right
/// (arithmetically, so a negative one rounds down), dropped when 24 or more apart.
pub fn add(a: u32, b: u32) -> u32 {
    let ea = a & EXP;
    if ea == 0 {
        return b;
    }
    let eb = b & EXP;
    if eb == 0 {
        return a;
    }
    let d = eb.wrapping_sub(ea) as i32;
    let (mut ma, mut mb) = (signed_mantissa(a), signed_mantissa(b));
    let mut e = ea;
    if d < 1 {
        if d < -0x0b7f_ffff {
            return a;
        }
        mb >>= (ea.wrapping_sub(eb) >> 23) & 31;
    } else {
        if d > 0x0b7f_ffff {
            return b;
        }
        ma >>= (d as u32 >> 23) & 31;
        e = eb;
    }
    let sum = ma.wrapping_add(mb) as u32;
    if sum == 0 {
        return 0;
    }
    let s = (sum as i32) >> 31;
    let abs = ((sum as i32 ^ s).wrapping_sub(s)) as u32;
    let lz = clz(abs);
    let sh = 8 - lz;
    let m = if sh < 0 { abs << (-sh as u32 & 31) } else { abs >> (sh as u32 & 31) };
    (((7 - lz) as u32).wrapping_mul(0x0080_0000).wrapping_add(e)) | (m & FRAC) | (sum & SIGN)
}

/// a - b (`FUN_6f06fa90`): a + -b.
pub fn sub(a: u32, b: u32) -> u32 {
    add(a, b ^ SIGN)
}

/// a * b (`FUN_6f06f9c0`): the mantissas' product's high word, a power of two's
/// exactly; zero when the exponent underflows.
pub fn mul(a: u32, b: u32) -> u32 {
    let sign = (a ^ b) & SIGN;
    let (ea, eb) = (a & EXP, b & EXP);
    let (fa, fb) = (a & FRAC, b & FRAC);
    if fa != 0 && fb != 0 {
        let p = ((fb | 0xff80_0000) << 8) as u64 * ((fa | 0xff80_0000) << 8) as u64;
        let hi = (p >> 32) as u32;
        let top = (p >> 63) as u32;
        let e = ea.wrapping_add(0xc080_0000).wrapping_add(eb);
        let carry = ((hi as i32) >> 31).wrapping_mul(-0x0080_0000) as u32;
        let r = ((hi >> top) >> 7 & FRAC) | carry.wrapping_add(e) | sign;
        return r & !((e.wrapping_sub(0x0080_0000) as i32 >> 31) as u32);
    }
    if ea != 0 && eb != 0 {
        let e = ea.wrapping_add(eb).wrapping_add(0xc080_0000);
        return (e | fb | fa | sign) & !((e.wrapping_sub(0x0080_0000) as i32 >> 31) as u32);
    }
    0
}

/// 1 / a (`FUN_6f0711e0`): the reciprocal table's fraction for the mantissa's top ten
/// bits, less the step to the next times the low thirteen spread over a word.
pub fn recip(a: u32) -> u32 {
    let m = a & FRAC;
    let i = (m >> 13) as usize;
    let s = m << 19;
    let c = (((s >> 13) | s) >> 13) | s;
    let step = RECIP[i].wrapping_sub(RECIP[i + 1]);
    let hi = ((step as u64 * c as u64) >> 32) as u32;
    let v = RECIP[i].wrapping_sub(hi).wrapping_sub(a & EXP).wrapping_add(0x7e80_0000);
    let r = (a & SIGN) | v;
    !((v.wrapping_sub(0x0080_0000) as i32 >> 31) as u32) & r
}

/// a / b (`FUN_6f06fcd0`): 1 when the two are the same, else a times b's reciprocal.
pub fn div(a: u32, b: u32) -> u32 {
    if a == b {
        return 0x3f80_0000;
    }
    mul(a, recip(b))
}

/// An integer as a single (`FUN_6f070d80`), its low bits cut off.
pub fn from_int(v: i32) -> u32 {
    if v == 0 {
        return 0;
    }
    let abs = v.unsigned_abs();
    let lz = clz(abs);
    let sh = lz - 8;
    let m = if sh < 0 { abs >> (-sh as u32 & 31) } else { abs << (sh as u32 & 31) };
    ((0x9e - lz) as u32) << 23 | (m & FRAC) | (v as u32 & SIGN)
}

/// A single as an integer, toward zero (`FUN_6f070120`).
pub fn to_int(a: u32) -> i32 {
    let e = (a >> 23) & 0xff;
    if e < 0x7f {
        return 0;
    }
    let m = (a & FRAC) | 0x0080_0000;
    let sh = 0x96 - e as i32;
    let s = (a as i32) >> 31;
    let v = if sh >= 0 { (m >> (sh as u32 & 31)) as i32 } else { (m << (-sh as u32 & 31)) as i32 };
    (v ^ s).wrapping_sub(s)
}

/// The greatest whole number not above a single, as a single (`FUN_6f070c80`).
pub fn floor(a: u32) -> u32 {
    let e = ((a >> 23) & 0xff) as i32 - 0x7f;
    if e < 0 {
        // Below one: -1 for a negative value, else 0.
        return if a.wrapping_mul(2) & ((a as i32 >> 31) as u32) == 0 { 0 } else { 0xbf80_0000 };
    }
    if e < 23 {
        let keep = ((SIGN as i32) >> (((a >> 23) as u8).wrapping_add(0x89) & 31)) as u32;
        if a.wrapping_mul(2) & !((a as i32 >> 31) as u32) != 0 {
            return a & keep;
        }
        let up = ((a & FRAC).wrapping_add(!keep)) & keep;
        return ((up >> 23).wrapping_add(0x7f).wrapping_add(e as u32)) << 23 | (up & FRAC) | SIGN;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(v: f32) -> u32 {
        v.to_bits()
    }

    #[test]
    fn whole_numbers_come_out_whole() {
        assert_eq!(add(f(1.0), f(2.0)), f(3.0));
        assert_eq!(sub(f(1.0), f(2.0)), f(-1.0));
        assert_eq!(add(f(-1.5), f(1.5)), 0);
        assert_eq!(mul(f(3.0), f(2.0)), f(6.0));
        assert_eq!(mul(f(1.5), f(1.5)), f(2.25));
        assert_eq!(mul(f(-2.0), f(0.5)), f(-1.0));
        assert_eq!(div(f(7.0), f(7.0)), f(1.0));
        assert_eq!(recip(f(2.0)), f(0.5));
        assert_eq!(from_int(250), f(250.0));
        assert_eq!(from_int(-3), f(-3.0));
        assert_eq!(to_int(f(-2.75)), -2);
        assert_eq!(to_int(f(431.9)), 431);
        assert_eq!(floor(f(2.5)), f(2.0));
        assert_eq!(floor(f(-2.5)), f(-3.0));
        assert_eq!(floor(f(0.25)), 0);
        assert_eq!(floor(f(-0.25)), f(-1.0));
    }

    /// The engine truncates: 1 + 2^-24 stays 1, and 0.1 + 0.2 lands a bit below the
    /// processor's.
    #[test]
    fn it_truncates() {
        assert_eq!(add(f(1.0), f(5.9604645e-8)), f(1.0));
        assert!(add(f(0.1), f(0.2)) <= f(0.1 + 0.2));
    }
}
