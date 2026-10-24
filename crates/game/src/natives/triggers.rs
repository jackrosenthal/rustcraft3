//! Triggers, events, conditions, timers, and thread control.

use crate::collections::HashMap;

use w3_jass::{NativeResult, Value, Vm};

use super::{Args, boolean, handle, int, none, real, reg};
use crate::types::*;
use crate::world::{NativeFn, World};

fn register_event(w: &mut World, trig: u32, kind: EventKind) -> NativeResult {
    if w.trigger(trig).is_none() {
        return none();
    }
    let e = w.handles.insert(Obj::Event(EventReg { trigger: trig, kind }));
    w.events.push(e);
    if let Some(t) = w.trigger_mut(trig) {
        t.events.push(e);
    }
    handle(e)
}

fn boolexpr(w: &mut World, e: BoolExpr) -> NativeResult {
    handle(w.handles.insert(Obj::BoolExpr(e)))
}

fn destroy_boolexpr(w: &mut World, _: &mut Vm, a: &[Value]) -> NativeResult {
    if matches!(w.handles.get(a.h(0)), Some(Obj::BoolExpr(_))) {
        w.handles.remove(a.h(0));
    }
    none()
}

fn opt(h: u32) -> Option<u32> {
    (h != 0).then_some(h)
}

fn player_unit_event(w: &mut World, _: &mut Vm, a: &[Value]) -> NativeResult {
    let Some(p) = w.player_of(a.h(1)) else { return none() };
    register_event(w, a.h(0), EventKind::PlayerUnit { player: p, event: a.h(2) as i32, filter: opt(a.h(3)) })
}

fn timer_start(w: &mut World, _: &mut Vm, a: &[Value]) -> NativeResult {
    let (timeout, periodic, cb) = (a.r(1), a.b(2), a.code(3));
    if let Some(t) = w.timer_mut(a.h(0)) {
        t.timeout = timeout;
        t.remaining = timeout;
        t.periodic = periodic;
        t.running = true;
        t.callback = cb;
    }
    none()
}

pub fn register(t: &mut HashMap<&'static str, NativeFn>) {
    reg!(t,
        CreateTrigger => |w, _, _| handle(w.handles.insert(Obj::Trigger(Trigger::default()))),
        DestroyTrigger => |w, _, a| {
            let h = a.h(0);
            let Some(tr) = w.trigger(h).cloned() else { return none() };
            for e in tr.events.iter().chain(&tr.conditions).chain(&tr.actions) {
                w.handles.remove(*e);
            }
            w.events.retain(|e| !tr.events.contains(e));
            w.handles.remove(h);
            none()
        },
        ResetTrigger => |w, _, a| {
            if let Some(t) = w.trigger_mut(a.h(0)) { t.eval_count = 0; t.exec_count = 0; }
            none()
        },
        EnableTrigger => |w, _, a| { if let Some(t) = w.trigger_mut(a.h(0)) { t.enabled = true; } none() },
        DisableTrigger => |w, _, a| { if let Some(t) = w.trigger_mut(a.h(0)) { t.enabled = false; } none() },
        IsTriggerEnabled => |w, _, a| boolean(w.trigger(a.h(0)).is_some_and(|t| t.enabled)),
        TriggerWaitOnSleeps => |w, _, a| { let v = a.b(1); if let Some(t) = w.trigger_mut(a.h(0)) { t.wait_on_sleeps = v; } none() },
        IsTriggerWaitOnSleeps => |w, _, a| boolean(w.trigger(a.h(0)).is_some_and(|t| t.wait_on_sleeps)),
        GetTriggerEvalCount => |w, _, a| int(w.trigger(a.h(0)).map_or(0, |t| t.eval_count)),
        GetTriggerExecCount => |w, _, a| int(w.trigger(a.h(0)).map_or(0, |t| t.exec_count)),
        TriggerAddCondition => |w, _, a| {
            let (trig, expr) = (a.h(0), a.h(1));
            if w.trigger(trig).is_none() { return none(); }
            let c = w.handles.insert(Obj::TriggerCondition { expr });
            if let Some(t) = w.trigger_mut(trig) { t.conditions.push(c); }
            handle(c)
        },
        TriggerRemoveCondition => |w, _, a| {
            let c = a.h(1);
            if let Some(t) = w.trigger_mut(a.h(0)) { t.conditions.retain(|x| *x != c); }
            w.handles.remove(c);
            none()
        },
        TriggerClearConditions => |w, _, a| {
            let conds = w.trigger_mut(a.h(0)).map(|t| std::mem::take(&mut t.conditions)).unwrap_or_default();
            for c in conds { w.handles.remove(c); }
            none()
        },
        TriggerAddAction => |w, _, a| {
            let trig = a.h(0);
            let Some(func) = a.code(1) else { return none() };
            if w.trigger(trig).is_none() { return none(); }
            let h = w.handles.insert(Obj::TriggerAction { func });
            if let Some(t) = w.trigger_mut(trig) { t.actions.push(h); }
            handle(h)
        },
        TriggerRemoveAction => |w, _, a| {
            let c = a.h(1);
            if let Some(t) = w.trigger_mut(a.h(0)) { t.actions.retain(|x| *x != c); }
            w.handles.remove(c);
            none()
        },
        TriggerClearActions => |w, _, a| {
            let acts = w.trigger_mut(a.h(0)).map(|t| std::mem::take(&mut t.actions)).unwrap_or_default();
            for c in acts { w.handles.remove(c); }
            none()
        },
        TriggerEvaluate => |w, vm, a| boolean(w.trigger_evaluate(vm, a.h(0))),
        TriggerExecute => |w, vm, a| {
            let ctx = EventCtx { trigger: a.h(0), ..w.ctx.clone() };
            w.trigger_execute(vm, a.h(0), ctx);
            none()
        },
        TriggerExecuteWait => |w, vm, a| {
            let ctx = EventCtx { trigger: a.h(0), ..w.ctx.clone() };
            w.trigger_execute(vm, a.h(0), ctx);
            none()
        },
        TriggerSleepAction => |_, _, a| NativeResult::Sleep(a.r(0)),
        TriggerWaitForSound => |_, _, _| NativeResult::Sleep(0.0),
        TriggerSyncStart => |_, _, _| none(),
        TriggerSyncReady => |_, _, _| NativeResult::Sleep(0.0),
        ExecuteFunc => |w, vm, a| {
            let name = a.s(0);
            if let Some(f) = vm.module.function(&name) {
                let ctx = w.ctx.clone();
                let t = vm.new_thread(f, &[], true);
                w.run_thread(vm, t, ctx);
            }
            none()
        },
        Condition => |w, _, a| match a.code(0) { Some(f) => boolexpr(w, BoolExpr::Func(f)), None => none() },
        Filter => |w, _, a| match a.code(0) { Some(f) => boolexpr(w, BoolExpr::Func(f)), None => none() },
        And => |w, _, a| boolexpr(w, BoolExpr::And(a.h(0), a.h(1))),
        Or => |w, _, a| boolexpr(w, BoolExpr::Or(a.h(0), a.h(1))),
        Not => |w, _, a| boolexpr(w, BoolExpr::Not(a.h(0))),
        DestroyBoolExpr => destroy_boolexpr,
        DestroyCondition => destroy_boolexpr,
        DestroyFilter => destroy_boolexpr,
        GetTriggeringTrigger => |w, _, _| handle(w.ctx.trigger),
        GetTriggerEventId => |w, _, _| NativeResult::Return(Value::Handle(w.ctx.event_id as u32)),
        GetTriggerPlayer => |w, _, _| {
            if w.ctx.has_player { NativeResult::Return(w.player_handle(w.ctx.player)) } else { none() }
        },
        GetTriggerUnit => |w, _, _| handle(w.ctx.unit),
        GetTriggerWidget => |w, _, _| handle(w.ctx.widget),
        GetTriggerDestructable => |w, _, _| handle(w.ctx.destructable),
        GetTriggeringRegion => |w, _, _| handle(w.ctx.region),
        GetEnteringUnit => |w, _, _| handle(w.ctx.unit),
        GetLeavingUnit => |w, _, _| handle(w.ctx.unit),
        GetExpiredTimer => |w, _, _| handle(w.ctx.timer),
        GetEventPlayerChatString => |w, _, _| super::string(w.ctx.chat.clone()),
        GetEventPlayerChatStringMatched => |w, _, _| super::string(w.ctx.matched_chat.clone()),
        GetEventPlayerState => |w, _, _| NativeResult::Return(Value::Handle(w.ctx.state as u32)),
        GetEventGameState => |w, _, _| NativeResult::Return(Value::Handle(w.ctx.state as u32)),
        GetEventUnitState => |w, _, _| NativeResult::Return(Value::Handle(w.ctx.state as u32)),
        GetEventDamage => |w, _, _| real(w.ctx.damage),
        GetEventDamageSource => |w, _, _| handle(w.ctx.other_unit),
        GetEventTargetUnit => |w, _, _| handle(w.ctx.target_unit),
        GetClickedButton => |w, _, _| handle(w.ctx.button),
        GetClickedDialog => |w, _, _| handle(w.ctx.dialog),
        GetTriggeringTrackable => |w, _, _| handle(w.ctx.trackable),
        // Event responses: which context field each event fills.
        GetLevelingUnit => |w, _, _| handle(w.ctx.unit),
        GetLearningUnit => |w, _, _| handle(w.ctx.unit),
        GetLearnedSkill => |w, _, _| int(w.ctx.spell_ability),
        GetLearnedSkillLevel => |w, _, _| int(w.ctx.state),
        GetRescuer => |w, _, _| handle(w.ctx.other_unit),
        GetDecayingUnit => |w, _, _| handle(w.ctx.unit),
        GetConstructingStructure => |w, _, _| handle(w.ctx.unit),
        GetCancelledStructure => |w, _, _| handle(w.ctx.unit),
        GetConstructedStructure => |w, _, _| handle(w.ctx.unit),
        GetResearchingUnit => |w, _, _| handle(w.ctx.unit),
        GetResearched => |w, _, _| int(w.ctx.research),
        GetTrainedUnitType => |w, _, _| int(w.ctx.train_unit_type),
        GetTrainedUnit => |w, _, _| handle(w.ctx.target_unit),
        GetDetectedUnit => |w, _, _| handle(w.ctx.unit),
        GetSummoningUnit => |w, _, _| handle(w.ctx.other_unit),
        GetSummonedUnit => |w, _, _| handle(w.ctx.unit),
        GetTransportUnit => |w, _, _| handle(w.ctx.other_unit),
        GetLoadedUnit => |w, _, _| handle(w.ctx.unit),
        GetChangingUnit => |w, _, _| handle(w.ctx.unit),
        GetChangingUnitPrevOwner => |w, _, _| NativeResult::Return(w.player_handle(w.ctx.state as u8)),
        GetManipulatingUnit => |w, _, _| handle(w.ctx.unit),
        GetOrderTargetDestructable => |w, _, _| handle(w.ctx.destructable),
        GetOrderTargetItem => |w, _, _| handle(w.ctx.item),
        GetSpellAbilityUnit => |w, _, _| handle(w.ctx.unit),
        GetSpellAbilityId => |w, _, _| int(w.ctx.spell_ability),
        GetSpellAbility => |_, _, _| handle(0),
        GetSpellTargetLoc => |w, _, _| { let (x, y) = (w.ctx.order_x, w.ctx.order_y); handle(w.handles.insert(Obj::Location(x, y))) },
        GetSpellTargetX => |w, _, _| real(w.ctx.order_x),
        GetSpellTargetY => |w, _, _| real(w.ctx.order_y),
        GetSpellTargetDestructable => |w, _, _| handle(w.ctx.destructable),
        GetSpellTargetItem => |w, _, _| handle(w.ctx.item),
        GetSpellTargetUnit => |w, _, _| handle(w.ctx.target_unit),
        GetEventDetectingPlayer => |w, _, _| NativeResult::Return(w.player_handle(w.ctx.player)),
        GetWinningPlayer => |w, _, _| NativeResult::Return(w.player_handle(w.ctx.player)),
        TriggerRegisterTimerEvent => |w, _, a| {
            // A private timer drives the event.
            let timer = w.handles.insert(Obj::Timer(Timer { remaining: a.r(1), timeout: a.r(1), periodic: a.b(2), running: true, callback: None }));
            register_event(w, a.h(0), EventKind::TimerExpire(timer))
        },
        TriggerRegisterTimerExpireEvent => |w, _, a| register_event(w, a.h(0), EventKind::TimerExpire(a.h(1))),
        TriggerRegisterGameStateEvent => |w, _, a| register_event(w, a.h(0), EventKind::GameState { state: a.h(1) as i32, op: a.h(2) as i32, value: a.r(3) }),
        TriggerRegisterVariableEvent => |w, _, a| register_event(w, a.h(0), EventKind::Variable { name: a.s(1), op: a.h(2) as i32, value: a.r(3) }),
        TriggerRegisterDialogEvent => |w, _, a| register_event(w, a.h(0), EventKind::Dialog(a.h(1))),
        TriggerRegisterDialogButtonEvent => |w, _, a| register_event(w, a.h(0), EventKind::DialogButton(a.h(1))),
        TriggerRegisterGameEvent => |w, _, a| register_event(w, a.h(0), EventKind::Game(a.h(1) as i32)),
        TriggerRegisterEnterRegion => |w, _, a| register_event(w, a.h(0), EventKind::EnterRegion { region: a.h(1), filter: opt(a.h(2)) }),
        TriggerRegisterLeaveRegion => |w, _, a| register_event(w, a.h(0), EventKind::LeaveRegion { region: a.h(1), filter: opt(a.h(2)) }),
        TriggerRegisterTrackableHitEvent => |w, _, a| register_event(w, a.h(0), EventKind::Trackable { trackable: a.h(1), hit: true }),
        TriggerRegisterTrackableTrackEvent => |w, _, a| register_event(w, a.h(0), EventKind::Trackable { trackable: a.h(1), hit: false }),
        TriggerRegisterPlayerEvent => |w, _, a| {
            let Some(p) = w.player_of(a.h(1)) else { return none() };
            register_event(w, a.h(0), EventKind::Player { player: p, event: a.h(2) as i32 })
        },
        TriggerRegisterPlayerUnitEvent => player_unit_event,
        TriggerRegisterPlayerAllianceChange => |w, _, a| {
            let Some(p) = w.player_of(a.h(1)) else { return none() };
            register_event(w, a.h(0), EventKind::Player { player: p, event: crate::event_ids::EVENT_PLAYER_ALLIANCE_CHANGED })
        },
        TriggerRegisterPlayerStateEvent => |w, _, a| {
            let Some(p) = w.player_of(a.h(1)) else { return none() };
            register_event(w, a.h(0), EventKind::PlayerState { player: p, state: a.h(2) as i32, op: a.h(3) as i32, value: a.r(4) })
        },
        TriggerRegisterPlayerChatEvent => |w, _, a| {
            let Some(p) = w.player_of(a.h(1)) else { return none() };
            register_event(w, a.h(0), EventKind::PlayerChat { player: p, text: a.s(2), exact: a.b(3) })
        },
        TriggerRegisterDeathEvent => |w, _, a| register_event(w, a.h(0), EventKind::Death(a.h(1))),
        TriggerRegisterUnitStateEvent => |w, _, a| register_event(w, a.h(0), EventKind::UnitState { unit: a.h(1), state: a.h(2) as i32, op: a.h(3) as i32, value: a.r(4) }),
        TriggerRegisterUnitEvent => |w, _, a| register_event(w, a.h(0), EventKind::Unit { unit: a.h(1), event: a.h(2) as i32 }),
        TriggerRegisterFilterUnitEvent => |w, _, a| register_event(w, a.h(0), EventKind::Unit { unit: a.h(1), event: a.h(2) as i32 }),
        TriggerRegisterUnitInRange => |w, _, a| register_event(w, a.h(0), EventKind::UnitInRange { unit: a.h(1), range: a.r(2), filter: opt(a.h(3)) }),
        CreateTimer => |w, _, _| handle(w.handles.insert(Obj::Timer(Timer::default()))),
        DestroyTimer => |w, _, a| { if w.timer_mut(a.h(0)).is_some() { w.handles.remove(a.h(0)); } none() },
        TimerStart => timer_start,
        PauseTimer => |w, _, a| { if let Some(t) = w.timer_mut(a.h(0)) { t.running = false; } none() },
        ResumeTimer => |w, _, a| { if let Some(t) = w.timer_mut(a.h(0)) { t.running = true; } none() },
        TimerGetElapsed => |w, _, a| real(w.timer_mut(a.h(0)).map_or(0.0, |t| t.timeout - t.remaining)),
        TimerGetRemaining => |w, _, a| real(w.timer_mut(a.h(0)).map_or(0.0, |t| t.remaining)),
        TimerGetTimeout => |w, _, a| real(w.timer_mut(a.h(0)).map_or(0.0, |t| t.timeout)),
    );
}
