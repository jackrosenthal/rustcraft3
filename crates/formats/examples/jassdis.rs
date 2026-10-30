//! `jassdis <save.w3z> <code block> [function]`: a saved JASS code block as the
//! original's VM reads it (`FUN_6f91b460`), each instruction its registers, opcode,
//! and argument (a name from the block's strings where the opcode takes one); with a
//! function name, that function alone.

/// Opcode names, by what the interpreter does with each (`FUN_6f91b460`'s switch;
/// the declarations are read before it runs).
const OPS: &[&str] = &[
    "?", "endprogram", "oldjump", "function", "endfunction", "local", "global", "constant", "funcarg", "extends", "type", "popn", "literal", "movrr", "movrv",
    "code", "movra", "movvr", "movar", "push", "pop", "native", "call", "i2r", "and", "or", "eq", "ne", "le", "ge", "lt", "gt", "add", "sub", "mul", "div", "mod",
    "neg", "not", "return", "label", "jumpiftrue", "jumpiffalse", "jump",
];

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let s = w3_formats::w3z::SaveGame::read(&std::fs::read(&a[1]).unwrap()).unwrap();
    let d = s.script_data().unwrap();
    let block: usize = a[2].parse().unwrap();
    let code = d.code[block - 1].as_ref().unwrap();
    let name = |id: u32| code.strings.get(id as usize - 1).map_or(format!("#{id}"), |s| s.clone());
    let mut on = a.get(3).is_none();
    for (i, w) in code.words.chunks(2).enumerate() {
        let [b0, b1, b2, op] = w[0].to_le_bytes();
        let arg = w[1];
        let op_name = OPS.get(op as usize).copied().unwrap_or("?");
        if let (Some(f), 3) = (a.get(3), op) {
            on = name(arg) == *f;
        }
        if !on {
            continue;
        }
        let shown = match op {
            3 | 5 | 6 | 7 | 8 | 9 | 0xa | 0xe | 0xf | 0x10 | 0x11 | 0x12 | 0x15 | 0x16 => name(arg),
            0xc if b1 == 6 => format!("{:?}", name(arg)),
            0xc if b1 == 5 => format!("{}", f32::from_bits(arg)),
            _ => format!("{}", arg as i32),
        };
        println!("{i:6}: {b0:3} {b1:3} {b2:3} {op_name:12} {shown}");
        if op == 4 && a.get(3).is_some() {
            on = false;
        }
    }
}
