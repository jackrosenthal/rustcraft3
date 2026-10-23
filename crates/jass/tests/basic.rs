use w3_jass::*;

struct NoHost(Vec<String>);
impl Host for NoHost {
    fn call_native(&mut self, vm: &mut Vm, n: u32, args: &[Value]) -> NativeResult {
        let name = vm.module.natives[n as usize].name.to_string();
        if name == "Print" {
            self.0.push(args[0].str().unwrap_or("").to_string());
        }
        NativeResult::Return(Value::Null)
    }
}

#[test]
fn arithmetic_loops_and_strings() {
    let src = r#"
native Print takes string s returns nothing
globals
    integer array squares
    real r = 3
    constant string hello = "hi"
endglobals
function fact takes integer n returns integer
    if n <= 1 then
        return 1
    endif
    return n * fact(n - 1)
endfunction
function main takes nothing returns real
    local integer i = 0
    loop
        exitwhen i >= 5
        set squares[i] = i * i
        set i = i + 1
    endloop
    call Print(hello + " " + "there")
    if squares[4] == 16 and fact(5) == 120 and not (r < 2) then
        return r / 2
    endif
    return 0
endfunction
"#;
    let m = compile([("t.j", src)]).unwrap();
    let main = m.function("main").unwrap();
    let mut vm = Vm::new(m);
    let mut host = NoHost(vec![]);
    vm.init_globals(&mut host).unwrap();
    match vm.call(&mut host, main, &[]) {
        Outcome::Finished(v) => assert_eq!(v, Value::Real(1.5)),
        o => panic!("{o:?}"),
    }
    assert_eq!(host.0, vec!["hi there"]);
}
