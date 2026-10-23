//! Runs a map's `config` and `main` with stub natives and prints each native called, in
//! first-call order, with call counts.
use std::collections::HashMap;
use w3_jass::*;

struct Tracer {
    counts: HashMap<String, usize>,
    order: Vec<String>,
    next_handle: u32,
}

impl Host for Tracer {
    fn call_native(&mut self, vm: &mut Vm, n: u32, _args: &[Value]) -> NativeResult {
        let decl = &vm.module.natives[n as usize];
        let name = decl.name.to_string();
        let c = self.counts.entry(name.clone()).or_insert(0);
        if *c == 0 {
            self.order.push(name.clone());
        }
        *c += 1;
        let ret = decl.ret.clone();
        NativeResult::Return(match ret.as_deref() {
            None => Value::Null,
            Some(t) => match vm.module.base_type(t) {
                BaseType::Integer => Value::Int(0),
                BaseType::Real => Value::Real(0.0),
                BaseType::Boolean => Value::Bool(false),
                BaseType::String => Value::string(""),
                BaseType::Code => Value::Null,
                BaseType::Handle => {
                    self.next_handle += 1;
                    Value::Handle(0x100000 + self.next_handle)
                }
            },
        })
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let common = std::fs::read_to_string(&a[1]).unwrap();
    let blizz = std::fs::read_to_string(&a[2]).unwrap();
    let ar = w3_mpq::Archive::open(&a[3]).unwrap();
    let script = ar.read("war3map.j").or_else(|_| ar.read("Scripts\\war3map.j")).unwrap();
    let script = String::from_utf8_lossy(&script);
    let m = compile([("common.j", &*common), ("Blizzard.j", &*blizz), ("war3map.j", &*script)]).unwrap();
    let mut vm = Vm::new(m);
    let mut t = Tracer { counts: HashMap::new(), order: vec![], next_handle: 0 };
    vm.init_globals(&mut t).unwrap();
    for f in ["config", "main"] {
        let id = vm.module.function(f).unwrap();
        println!("== {f}: {:?}", vm.call(&mut t, id, &[]));
    }
    for n in &t.order {
        println!("{:6} {n}", t.counts[n]);
    }
}
