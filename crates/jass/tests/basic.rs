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

/// common.ai's PickMeleeHero: a local array's elements swapped by indices held in
/// locals (read: an undead computer's picks 2, 1, 2 give the Dreadlord, the Death
/// Knight, and the Crypt Lord).
#[test]
fn local_array_swaps() {
    let src = r#"
globals
    integer hero_id = 0
    integer hero_id2 = 0
    integer hero_id3 = 0
endglobals
function pick takes integer first, integer second, integer third returns integer
    local integer last
    local integer array heroes
    set heroes[1] = 10
    set heroes[2] = 20
    set heroes[3] = 30
    set heroes[4] = 40
    set last = 4
    set hero_id        = heroes[first]
    set heroes[first]  = heroes[last]
    set hero_id2       = heroes[second]
    set heroes[second] = heroes[last-1]
    set hero_id3       = heroes[third]
    return hero_id * 10000 + hero_id2 * 100 + hero_id3
endfunction
function main takes nothing returns integer
    return pick(2, 1, 2)
endfunction
"#;
    let m = compile([("t.j", src)]).unwrap();
    let main = m.function("main").unwrap();
    let mut vm = Vm::new(m);
    let mut host = NoHost(vec![]);
    vm.init_globals(&mut host).unwrap();
    match vm.call(&mut host, main, &[]) {
        Outcome::Finished(v) => assert_eq!(v, Value::Int(20 * 10000 + 10 * 100 + 40)),
        o => panic!("{o:?}"),
    }
}

/// PickMeleeHero as common.ai has it: its bound set in a branch, the draws from a native.
#[test]
fn pick_melee_hero_shape() {
    let src = r#"
native Rand takes integer lo, integer hi returns integer
native Version takes nothing returns boolean
globals
    integer hero_id = 0
    integer hero_id2 = 0
    integer hero_id3 = 0
endglobals
function pick takes integer raceid returns integer
    local integer first
    local integer second
    local integer third
    local integer last
    local integer array heroes
    if raceid == 1 then
        set heroes[1] = 1
        set heroes[2] = 2
        set heroes[3] = 3
        set heroes[4] = 4
    elseif raceid == 4 then
        set heroes[1] = 10
        set heroes[2] = 20
        set heroes[3] = 30
        set heroes[4] = 40
    else
        set hero_id = 0
    endif
    if Version() then
        set last = 4
    else
        set last = 3
    endif
    set first  = Rand(1,last)
    set second = Rand(1,last-1)
    set third  = Rand(1,last-2)
    set hero_id        = heroes[first]
    set heroes[first]  = heroes[last]
    set hero_id2       = heroes[second]
    set heroes[second] = heroes[last-1]
    set hero_id3       = heroes[third]
    return hero_id
endfunction
function main takes nothing returns integer
    call pick(4)
    return hero_id * 10000 + hero_id2 * 100 + hero_id3
endfunction
"#;
    struct Picks(Vec<i32>);
    impl Host for Picks {
        fn call_native(&mut self, vm: &mut Vm, n: u32, _args: &[Value]) -> NativeResult {
            match &*vm.module.natives[n as usize].name {
                "Rand" => NativeResult::Return(Value::Int(self.0.remove(0))),
                _ => NativeResult::Return(Value::Bool(true)),
            }
        }
    }
    let m = compile([("t.j", src)]).unwrap();
    let main = m.function("main").unwrap();
    let mut vm = Vm::new(m);
    let mut host = Picks(vec![2, 1, 2]);
    vm.init_globals(&mut host).unwrap();
    match vm.call(&mut host, main, &[]) {
        Outcome::Finished(v) => assert_eq!(v, Value::Int(20 * 10000 + 10 * 100 + 40)),
        o => panic!("{o:?}"),
    }
}
