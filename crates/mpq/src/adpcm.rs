//! Storm's IMA-ADPCM variant used for compressed WAVE sectors.

const NEXT_STEP: [i32; 32] = [
    -1, 0, -1, 4, -1, 2, -1, 6, -1, 1, -1, 5, -1, 3, -1, 7, //
    -1, 1, -1, 5, -1, 3, -1, 7, -1, 2, -1, 4, -1, 6, -1, 8,
];

const STEP_SIZE: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408,
    449, 494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066,
    2272, 2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630,
    9493, 10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794,
    32767,
];

pub fn decompress(input: &[u8], channels: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len() * 4);
    if input.len() < 2 + channels * 2 {
        return out;
    }
    let shift = input[1] as u32;
    let mut pos = 2;
    let mut predicted = [0i32; 2];
    let mut step = [0x2Ci32; 2];
    for p in predicted.iter_mut().take(channels) {
        let s = i16::from_le_bytes([input[pos], input[pos + 1]]);
        pos += 2;
        *p = s as i32;
        out.extend_from_slice(&s.to_le_bytes());
    }
    let mut ch = channels - 1;
    let toggle = |c: usize| if channels == 2 { c ^ 1 } else { c };
    while pos < input.len() {
        let e = input[pos] as i32;
        pos += 1;
        ch = toggle(ch);
        if e & 0x80 != 0 {
            match e & 0x7F {
                0 => {
                    if step[ch] != 0 {
                        step[ch] -= 1;
                    }
                    out.extend_from_slice(&(predicted[ch] as i16).to_le_bytes());
                }
                1 => {
                    step[ch] = (step[ch] + 8).min(0x58);
                    ch = toggle(ch);
                }
                2 => {}
                _ => {
                    step[ch] = (step[ch] - 8).max(0);
                    ch = toggle(ch);
                }
            }
        } else {
            let ss = STEP_SIZE[step[ch] as usize];
            let mut diff = ss >> shift;
            for b in 0..6 {
                if e & (1 << b) != 0 {
                    diff += ss >> b;
                }
            }
            predicted[ch] = if e & 0x40 != 0 {
                (predicted[ch] - diff).max(-32768)
            } else {
                (predicted[ch] + diff).min(32767)
            };
            out.extend_from_slice(&(predicted[ch] as i16).to_le_bytes());
            step[ch] = (step[ch] + NEXT_STEP[(e & 0x1F) as usize]).clamp(0, 88);
        }
    }
    out
}
