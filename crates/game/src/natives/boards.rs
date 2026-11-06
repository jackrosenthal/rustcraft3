//! Timer dialogs, leaderboards, and multiboards: the on-screen panels maps fill.

use crate::collections::HashMap;

use w3_jass::Value;

use super::{Args, boolean, handle, int, none, reg, string};
use crate::types::*;
use crate::world::{NativeFn, World};
use crate::codes;

fn rgba(a: &[Value], first: usize) -> [u8; 4] {
    let c = |i: usize| a.i(first + i).clamp(0, 255) as u8;
    [c(0), c(1), c(2), c(3)]
}

/// The C runtime's qsort, as the leaderboard's sorts call it (`FUN_6f3d2980`; the Visual
/// C++ runtime's, msvcr80 on, provisional which): a median-of-three quicksort leaving
/// runs of eight or fewer to a selection sort that takes the first of the greatest to
/// the end, so that ties fall as the original's do (it is not a stable sort).
fn crt_qsort<T>(v: &mut [T], cmp: impl Fn(&T, &T) -> i32) {
    if v.len() < 2 {
        return;
    }
    let mut stack: Vec<(isize, isize)> = Vec::new();
    let (mut lo, mut hi) = (0isize, v.len() as isize - 1);
    let c = |v: &[T], a: isize, b: isize| cmp(&v[a as usize], &v[b as usize]);
    'recurse: loop {
        let size = hi - lo + 1;
        if size <= 8 {
            let mut top = hi;
            while top > lo {
                let mut max = lo;
                for p in lo + 1..=top {
                    if c(v, p, max) > 0 {
                        max = p;
                    }
                }
                v.swap(max as usize, top as usize);
                top -= 1;
            }
        } else {
            let mut mid = lo + size / 2;
            for (x, y) in [(lo, mid), (lo, hi), (mid, hi)] {
                if c(v, x, y) > 0 {
                    v.swap(x as usize, y as usize);
                }
            }
            let (mut loguy, mut higuy) = (lo, hi);
            loop {
                if mid > loguy {
                    loguy += 1;
                    while loguy < mid && c(v, loguy, mid) <= 0 {
                        loguy += 1;
                    }
                }
                if mid <= loguy {
                    loguy += 1;
                    while loguy <= hi && c(v, loguy, mid) <= 0 {
                        loguy += 1;
                    }
                }
                higuy -= 1;
                while higuy > mid && c(v, higuy, mid) > 0 {
                    higuy -= 1;
                }
                if higuy < loguy {
                    break;
                }
                v.swap(loguy as usize, higuy as usize);
                if mid == higuy {
                    mid = loguy;
                }
            }
            higuy += 1;
            if mid < higuy {
                higuy -= 1;
                while higuy > mid && c(v, higuy, mid) == 0 {
                    higuy -= 1;
                }
            }
            if mid >= higuy {
                higuy -= 1;
                while higuy > lo && c(v, higuy, mid) == 0 {
                    higuy -= 1;
                }
            }
            if higuy - lo >= hi - loguy {
                if lo < higuy {
                    stack.push((lo, higuy));
                }
                if loguy < hi {
                    lo = loguy;
                    continue 'recurse;
                }
            } else {
                if loguy < hi {
                    stack.push((loguy, hi));
                }
                if lo < higuy {
                    hi = higuy;
                    continue 'recurse;
                }
            }
        }
        match stack.pop() {
            Some((l, h)) => (lo, hi) = (l, h),
            None => return,
        }
    }
}

/// Sorts a leaderboard's items (`FUN_6f3d2980`): by a key's difference, the second's
/// less the first's when descending.
fn sort_items(b: &mut Leaderboard, ascending: bool, cmp: impl Fn(&LeaderboardItem, &LeaderboardItem) -> i32) {
    if ascending {
        crt_qsort(&mut b.items, cmp);
    } else {
        crt_qsort(&mut b.items, |x, y| cmp(y, x));
    }
}

fn timer_dialog(w: &mut World, h: u32) -> Option<&mut TimerDialog> {
    match w.handles.get_mut(h) {
        Some(Obj::TimerDialog(d)) => Some(d),
        _ => None,
    }
}

fn leaderboard(w: &mut World, h: u32) -> Option<&mut Leaderboard> {
    match w.handles.get_mut(h) {
        Some(Obj::Leaderboard(b)) => Some(b),
        _ => None,
    }
}

fn leaderboard_ref(w: &World, h: u32) -> Option<&Leaderboard> {
    match w.handles.get(h) {
        Some(Obj::Leaderboard(b)) => Some(b),
        _ => None,
    }
}

/// A leaderboard's item by index: none for one out of range, a negative one too (the -1
/// LeaderboardGetPlayerIndex gives for a player without an item; `FUN_6f2af270`).
fn leaderboard_item(w: &mut World, h: u32, i: i32) -> Option<&mut LeaderboardItem> {
    leaderboard(w, h)?.items.get_mut(usize::try_from(i).ok()?)
}

fn multiboard(w: &mut World, h: u32) -> Option<&mut Multiboard> {
    match w.handles.get_mut(h) {
        Some(Obj::Multiboard(b)) => Some(b),
        _ => None,
    }
}

fn multiboard_ref(w: &World, h: u32) -> Option<&Multiboard> {
    match w.handles.get(h) {
        Some(Obj::Multiboard(b)) => Some(b),
        _ => None,
    }
}

/// A multiboard item handle's cell.
fn item_cell(w: &mut World, h: u32) -> Option<&mut MultiboardItem> {
    let (board, row, col) = match w.handles.get(h) {
        Some(Obj::MultiboardItem(b, r, c)) => (*b, *r, *c),
        _ => return None,
    };
    multiboard(w, board)?.items.get_mut(row)?.get_mut(col)
}

pub fn register(t: &mut HashMap<&'static str, NativeFn>) {
    reg!(t,
        CreateTimerDialog => |w, _, a| {
            let slot = w.tags.take(codes::class::TIMER_DIALOG);
            let h = w.handles.insert(Obj::TimerDialog(TimerDialog { timer: a.h(0), ..Default::default() }));
            w.tags.handle_slots.insert(h, slot);
            handle(h)
        },
        DestroyTimerDialog => |w, _, a| { if timer_dialog(w, a.h(0)).is_some() { w.handles.remove(a.h(0)); } none() },
        TimerDialogSetTitle => |w, _, a| { let t = w.localized(&a.s(1)); if let Some(d) = timer_dialog(w, a.h(0)) { d.title = Some(t); } none() },
        TimerDialogSetTitleColor => |w, _, a| { let c = rgba(a, 1); if let Some(d) = timer_dialog(w, a.h(0)) { d.title_color = Some(c); } none() },
        TimerDialogSetTimeColor => |w, _, a| { let c = rgba(a, 1); if let Some(d) = timer_dialog(w, a.h(0)) { d.time_color = Some(c); } none() },
        TimerDialogSetSpeed => |w, _, a| { let s = a.r(1); if let Some(d) = timer_dialog(w, a.h(0)) { d.speed = s; } none() },
        TimerDialogDisplay => |w, _, a| { let on = a.b(1); if let Some(d) = timer_dialog(w, a.h(0)) { d.displayed = on; } none() },
        IsTimerDialogDisplayed => |w, _, a| boolean(timer_dialog(w, a.h(0)).is_some_and(|d| d.displayed)),
        TimerDialogSetRealTimeRemaining => |w, _, a| { let r = a.r(1); if let Some(d) = timer_dialog(w, a.h(0)) { d.real_time = Some(r); } none() },

        CreateLeaderboard => |w, _, _| {
            let slot = w.tags.take(codes::class::LEADERBOARD);
            let h = w.handles.insert(Obj::Leaderboard(Leaderboard::default()));
            w.tags.handle_slots.insert(h, slot);
            handle(h)
        },
        DestroyLeaderboard => |w, _, a| { if leaderboard(w, a.h(0)).is_some() { w.handles.remove(a.h(0)); } none() },
        LeaderboardDisplay => |w, _, a| { let on = a.b(1); if let Some(b) = leaderboard(w, a.h(0)) { b.displayed = on; } none() },
        IsLeaderboardDisplayed => |w, _, a| boolean(leaderboard_ref(w, a.h(0)).is_some_and(|b| b.displayed)),
        LeaderboardGetItemCount => |w, _, a| int(leaderboard_ref(w, a.h(0)).map_or(0, |b| b.items.len() as i32)),
        LeaderboardSetSizeByItemCount => |w, _, a| { let n = a.i(1).max(0) as usize; if let Some(b) = leaderboard(w, a.h(0)) { b.size = n; } none() },
        LeaderboardAddItem => |w, _, a| {
            let (label, value, player) = (w.localized(&a.s(1)), a.i(2), w.player_of(a.h(3)).unwrap_or(0));
            if let Some(b) = leaderboard(w, a.h(0)) {
                // New items follow the board's name and value style, without an icon,
                // in the player's color.
                let style = [b.style[1], b.style[2], false];
                b.items.push(LeaderboardItem { label, value, player, style, ..Default::default() });
                b.size = b.size.max(b.items.len());
            }
            none()
        },
        LeaderboardRemoveItem => |w, _, a| { let i = a.i(1); if let Some(b) = leaderboard(w, a.h(0)) { if i >= 0 && (i as usize) < b.items.len() { b.items.remove(i as usize); } } none() },
        LeaderboardRemovePlayerItem => |w, _, a| { let p = w.player_of(a.h(1)); if let (Some(p), Some(b)) = (p, leaderboard(w, a.h(0))) { b.items.retain(|i| i.player != p); } none() },
        LeaderboardClear => |w, _, a| { if let Some(b) = leaderboard(w, a.h(0)) { b.items.clear(); } none() },
        LeaderboardSortItemsByValue => |w, _, a| { let asc = a.b(1); if let Some(b) = leaderboard(w, a.h(0)) { sort_items(b, asc, |x, y| x.value.wrapping_sub(y.value)); } none() },
        LeaderboardSortItemsByPlayer => |w, _, a| { let asc = a.b(1); if let Some(b) = leaderboard(w, a.h(0)) { sort_items(b, asc, |x, y| x.player as i32 - y.player as i32); } none() },
        // (The original compares the labels with Storm's ordinal 508; here by their
        // bytes, provisional.)
        LeaderboardSortItemsByLabel => |w, _, a| { let asc = a.b(1); if let Some(b) = leaderboard(w, a.h(0)) { sort_items(b, asc, |x, y| x.label.cmp(&y.label) as i32); } none() },
        LeaderboardHasPlayerItem => |w, _, a| { let p = w.player_of(a.h(1)); boolean(p.is_some_and(|p| leaderboard_ref(w, a.h(0)).is_some_and(|b| b.items.iter().any(|i| i.player == p)))) },
        LeaderboardGetPlayerIndex => |w, _, a| { let p = w.player_of(a.h(1)); int(p.and_then(|p| leaderboard_ref(w, a.h(0)).and_then(|b| b.items.iter().position(|i| i.player == p))).map_or(-1, |i| i as i32)) },
        LeaderboardSetLabel => |w, _, a| { let l = w.localized(&a.s(1)); if let Some(b) = leaderboard(w, a.h(0)) { b.label = l; } none() },
        LeaderboardGetLabelText => |w, _, a| string(leaderboard_ref(w, a.h(0)).map(|b| b.label.clone()).unwrap_or_default()),
        PlayerSetLeaderboard => |w, _, a| { let (p, b) = (w.player_of(a.h(0)), a.h(1)); if let Some(p) = p { w.player_leaderboards.insert(p, b); } none() },
        PlayerGetLeaderboard => |w, _, a| handle(w.player_of(a.h(0)).and_then(|p| w.player_leaderboards.get(&p).copied()).unwrap_or(0)),
        // The board's colors and style apply to every item it has.
        LeaderboardSetLabelColor => |w, _, a| { let c = rgba(a, 1); if let Some(b) = leaderboard(w, a.h(0)) { b.items.iter_mut().for_each(|i| i.label_color = Some(c)); } none() },
        LeaderboardSetValueColor => |w, _, a| { let c = rgba(a, 1); if let Some(b) = leaderboard(w, a.h(0)) { b.items.iter_mut().for_each(|i| i.value_color = Some(c)); } none() },
        LeaderboardSetStyle => |w, _, a| {
            let s = [a.b(1), a.b(2), a.b(3), a.b(4)];
            if let Some(b) = leaderboard(w, a.h(0)) {
                b.style = s;
                b.items.iter_mut().for_each(|i| i.style = [s[1], s[2], s[3]]);
            }
            none()
        },
        LeaderboardSetItemValue => |w, _, a| { let (i, v) = (a.i(1), a.i(2)); if let Some(it) = leaderboard_item(w, a.h(0), i) { it.value = v; } none() },
        LeaderboardSetItemLabel => |w, _, a| { let (i, v) = (a.i(1), w.localized(&a.s(2))); if let Some(it) = leaderboard_item(w, a.h(0), i) { it.label = v; } none() },
        LeaderboardSetItemStyle => |w, _, a| { let (i, s) = (a.i(1), [a.b(2), a.b(3), a.b(4)]); if let Some(it) = leaderboard_item(w, a.h(0), i) { it.style = s; } none() },
        LeaderboardSetItemLabelColor => |w, _, a| { let (i, c) = (a.i(1), rgba(a, 2)); if let Some(it) = leaderboard_item(w, a.h(0), i) { it.label_color = Some(c); } none() },
        LeaderboardSetItemValueColor => |w, _, a| { let (i, c) = (a.i(1), rgba(a, 2)); if let Some(it) = leaderboard_item(w, a.h(0), i) { it.value_color = Some(c); } none() },

        CreateMultiboard => |w, _, _| {
            w.tags.take(codes::class::MULTIBOARD);
            handle(w.handles.insert(Obj::Multiboard(Multiboard::default())))
        },
        DestroyMultiboard => |w, _, a| { if multiboard(w, a.h(0)).is_some() { w.handles.remove(a.h(0)); } none() },
        MultiboardDisplay => |w, _, a| {
            let (h, on) = (a.h(0), a.b(1));
            // One multiboard shows at a time.
            if on {
                for (_, o) in w.handles.iter_mut() {
                    if let Obj::Multiboard(b) = o { b.displayed = false; }
                }
            }
            if let Some(b) = multiboard(w, h) { b.displayed = on; }
            none()
        },
        IsMultiboardDisplayed => |w, _, a| boolean(multiboard_ref(w, a.h(0)).is_some_and(|b| b.displayed)),
        MultiboardMinimize => |w, _, a| { let on = a.b(1); if let Some(b) = multiboard(w, a.h(0)) { b.minimized = on; } none() },
        IsMultiboardMinimized => |w, _, a| boolean(multiboard_ref(w, a.h(0)).is_some_and(|b| b.minimized)),
        MultiboardClear => |w, _, a| { if let Some(b) = multiboard(w, a.h(0)) { b.items.clear(); b.rows = 0; b.cols = 0; } none() },
        MultiboardSetTitleText => |w, _, a| { let t = w.localized(&a.s(1)); if let Some(b) = multiboard(w, a.h(0)) { b.title = t; } none() },
        MultiboardGetTitleText => |w, _, a| string(multiboard_ref(w, a.h(0)).map(|b| b.title.clone()).unwrap_or_default()),
        MultiboardSetTitleTextColor => |w, _, a| { let c = rgba(a, 1); if let Some(b) = multiboard(w, a.h(0)) { b.title_color = Some(c); } none() },
        MultiboardGetRowCount => |w, _, a| int(multiboard_ref(w, a.h(0)).map_or(0, |b| b.rows as i32)),
        MultiboardGetColumnCount => |w, _, a| int(multiboard_ref(w, a.h(0)).map_or(0, |b| b.cols as i32)),
        MultiboardSetColumnCount => |w, _, a| { let n = a.i(1).max(0) as usize; if let Some(b) = multiboard(w, a.h(0)) { b.resize(b.rows, n); } none() },
        MultiboardSetRowCount => |w, _, a| { let n = a.i(1).max(0) as usize; if let Some(b) = multiboard(w, a.h(0)) { b.resize(n, b.cols); } none() },
        MultiboardSetItemsStyle => |w, _, a| { let (v, i) = (a.b(1), a.b(2)); if let Some(b) = multiboard(w, a.h(0)) { b.each(|it| { it.show_value = v; it.show_icon = i; }); } none() },
        MultiboardSetItemsValue => |w, _, a| { let s = w.localized(&a.s(1)); if let Some(b) = multiboard(w, a.h(0)) { b.each(|it| it.value = s.clone()); } none() },
        MultiboardSetItemsValueColor => |w, _, a| { let c = rgba(a, 1); if let Some(b) = multiboard(w, a.h(0)) { b.each(|it| it.color = c); } none() },
        MultiboardSetItemsWidth => |w, _, a| { let x = a.r(1); if let Some(b) = multiboard(w, a.h(0)) { b.each(|it| it.width = x); } none() },
        MultiboardSetItemsIcon => |w, _, a| { let s = a.s(1); if let Some(b) = multiboard(w, a.h(0)) { b.each(|it| it.icon = s.clone()); } none() },
        MultiboardGetItem => |w, _, a| { let (b, r, c) = (a.h(0), a.i(1).max(0) as usize, a.i(2).max(0) as usize); handle(w.handles.insert(Obj::MultiboardItem(b, r, c))) },
        MultiboardReleaseItem => |w, _, a| { if matches!(w.handles.get(a.h(0)), Some(Obj::MultiboardItem(..))) { w.handles.remove(a.h(0)); } none() },
        MultiboardSetItemStyle => |w, _, a| { let (v, i) = (a.b(1), a.b(2)); if let Some(it) = item_cell(w, a.h(0)) { it.show_value = v; it.show_icon = i; } none() },
        MultiboardSetItemValue => |w, _, a| { let s = w.localized(&a.s(1)); if let Some(it) = item_cell(w, a.h(0)) { it.value = s; } none() },
        MultiboardSetItemValueColor => |w, _, a| { let c = rgba(a, 1); if let Some(it) = item_cell(w, a.h(0)) { it.color = c; } none() },
        MultiboardSetItemWidth => |w, _, a| { let x = a.r(1); if let Some(it) = item_cell(w, a.h(0)) { it.width = x; } none() },
        MultiboardSetItemIcon => |w, _, a| { let s = a.s(1); if let Some(it) = item_cell(w, a.h(0)) { it.icon = s; } none() },
        MultiboardSuppressDisplay => |w, _, a| { w.multiboard_suppressed = a.b(0); none() },
    );
}

#[cfg(test)]
mod tests {
    use super::crt_qsort;

    /// Equal keys move as the C runtime's selection sort moves them: each pass takes the
    /// first of the greatest to the end.
    #[test]
    fn ties_fall_as_the_c_runtimes() {
        let mut v = [(0, 'a'), (0, 'b'), (0, 'c'), (0, 'd')];
        crt_qsort(&mut v, |x, y| x.0 - y.0);
        assert_eq!(v.map(|x| x.1), ['b', 'c', 'd', 'a']);
        // Descending, as the leaderboards sort by value.
        let mut v = [(0, 'a'), (1, 'b'), (0, 'c')];
        crt_qsort(&mut v, |x, y| y.0 - x.0);
        assert_eq!(v.map(|x| x.1), ['b', 'c', 'a']);
    }

    /// Longer runs go by the quicksort, sorted all the same.
    #[test]
    fn long_runs_sort() {
        let mut seed = 12345u32;
        for n in 0..40 {
            let mut v: Vec<i32> = (0..n)
                .map(|_| {
                    seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                    (seed >> 16) as i32 % 7
                })
                .collect();
            let mut want = v.clone();
            want.sort();
            crt_qsort(&mut v, |x, y| x - y);
            assert_eq!(v, want);
        }
    }
}
