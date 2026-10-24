//! JASS native implementations, registered by the names common.j declares.

use crate::collections::HashMap;

use w3_jass::{NativeResult, Value};

use crate::world::NativeFn;

pub mod basic;
pub mod game;
pub mod geometry;
pub mod objects;
pub mod players;
pub mod boards;
pub mod presentation;
pub mod triggers;
pub mod misc;
pub mod units;
pub mod visuals;

pub fn table() -> HashMap<&'static str, NativeFn> {
    let mut t: HashMap<&'static str, NativeFn> = HashMap::default();
    basic::register(&mut t);
    boards::register(&mut t);
    game::register(&mut t);
    geometry::register(&mut t);
    objects::register(&mut t);
    players::register(&mut t);
    presentation::register(&mut t);
    triggers::register(&mut t);
    misc::register(&mut t);
    units::register(&mut t);
    visuals::register(&mut t);
    t
}

pub(crate) fn none() -> NativeResult {
    NativeResult::Return(Value::Null)
}
pub(crate) fn int(v: i32) -> NativeResult {
    NativeResult::Return(Value::Int(v))
}
pub(crate) fn real(v: f32) -> NativeResult {
    NativeResult::Return(Value::Real(v))
}
pub(crate) fn boolean(v: bool) -> NativeResult {
    NativeResult::Return(Value::Bool(v))
}
pub(crate) fn string(v: impl AsRef<str>) -> NativeResult {
    NativeResult::Return(Value::string(v))
}
pub(crate) fn handle(h: u32) -> NativeResult {
    NativeResult::Return(Value::opt_handle(h))
}

/// Argument accessors.
pub(crate) trait Args {
    fn i(&self, n: usize) -> i32;
    fn r(&self, n: usize) -> f32;
    fn b(&self, n: usize) -> bool;
    fn s(&self, n: usize) -> String;
    fn h(&self, n: usize) -> u32;
    fn code(&self, n: usize) -> Option<w3_jass::FuncId>;
}

impl Args for [Value] {
    fn i(&self, n: usize) -> i32 {
        self.get(n).map_or(0, Value::int)
    }
    fn r(&self, n: usize) -> f32 {
        self.get(n).map_or(0.0, Value::real)
    }
    fn b(&self, n: usize) -> bool {
        self.get(n).is_some_and(Value::bool)
    }
    fn s(&self, n: usize) -> String {
        self.get(n).and_then(|v| v.str()).unwrap_or("").to_string()
    }
    fn h(&self, n: usize) -> u32 {
        self.get(n).map_or(0, Value::handle)
    }
    fn code(&self, n: usize) -> Option<w3_jass::FuncId> {
        self.get(n).and_then(Value::code)
    }
}

/// Registers natives: `reg!(t, Name => function, ...)`.
macro_rules! reg {
    ($t:ident, $($name:ident => $f:expr),* $(,)?) => {
        $( $t.insert(stringify!($name), $f as crate::world::NativeFn); )*
    };
}
pub(crate) use reg;
