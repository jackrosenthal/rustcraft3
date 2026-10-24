//! Timer dialogs, the multiboard, and the leaderboard at the world frame's upper right
//! (CGameUI): the first timer dialog 0.005 left of and 0.015 below the world frame's
//! top right (0.02 below the screen's), later ones each 0.002 left of the last; the
//! multiboard under the first dialog (or at the corner), and the leaderboard under the
//! multiboard (or the first dialog, or the corner).

use w3_game::types::{Leaderboard, Multiboard, Obj, TimerDialog};
use w3_game::World;
use w3_ui::{Draw, FrameDb, FramePoint as P, FrameTree, Rect};

pub struct BoardsView {
    key: String,
    pub tree: FrameTree,
    /// Leaderboard rows and multiboard cells, drawn over the frames.
    extra: Vec<Draw>,
    /// The shown multiboard, for its minimize button.
    multiboard: Option<u32>,
}

/// The world frame's top right relative to the 4:3 root's: the world frame spans the
/// whole window, its top 0.02 below the screen's.
fn world_top_right(t: &FrameTree) -> (f32, f32) {
    ((t.width - w3_ui::UI_WIDTH) / 2.0, -0.02)
}

fn color(c: [u8; 4]) -> [f32; 4] {
    [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, c[3] as f32 / 255.0]
}

/// Replaces one property of a frame's definition (a backdrop's texture).
fn set_prop(t: &mut FrameTree, name: &str, key: &str, value: &str) {
    let Some(i) = t.get(name) else { return };
    let mut def = (*t.frames[i].def).clone();
    for p in def.props.iter_mut().filter(|p| p.key.eq_ignore_ascii_case(key)) {
        p.args = vec![w3_ui::fdf::Arg::Str(value.into())];
    }
    t.frames[i].def = std::rc::Rc::new(def);
}

struct Shown<'a> {
    dialogs: Vec<(&'a TimerDialog, String)>,
    multiboard: Option<(u32, std::borrow::Cow<'a, Multiboard>)>,
    leaderboard: Option<&'a Leaderboard>,
}

/// The handle standing for the Team Resources board.
pub const TEAM_RESOURCES: u32 = u32::MAX;

/// Alliance type 7, shared advanced control.
const ALLIANCE_SHARED_ADVANCED_CONTROL: usize = 7;

/// The Team Resources board (FUN_6f3dffa0, FUN_6f3deb30): shown while other players
/// share advanced control with the local one, a row for each with the name in its
/// color, gold, lumber, and food used over food made, green, yellow, or red by its
/// upkeep level.
fn team_resources(w: &World, db: &FrameDb, team_rgb: &[[f32; 3]; 16], minimized: bool) -> Option<Multiboard> {
    use w3_game::types::{player_state as ps, MultiboardItem};
    let me = w.local_player as usize;
    let allies: Vec<usize> = (0..12.min(w.players.len())).filter(|&p| p != me && w.players[p].alliances[me][ALLIANCE_SHARED_ADVANCED_CONTROL]).collect();
    if allies.is_empty() {
        return None;
    }
    let mut b = Multiboard { title: db.string("TEAM_RESOURCES"), title_color: Some([0xf0, 0xf0, 0x10, 0xff]), displayed: true, minimized, ..Multiboard::default() };
    b.resize(allies.len(), 4);
    let icons = [String::new(), db.skin_value("GoldIcon").unwrap_or_default(), db.skin_value("LumberIcon").unwrap_or_default(), db.skin_value("SupplyIcon").unwrap_or_default()];
    let widths = [0.1, 0.052, 0.05, 0.05];
    for (row, &p) in allies.iter().enumerate() {
        let pl = &w.players[p];
        let (used, cap) = (pl.get_state(ps::RESOURCE_FOOD_USED), pl.get_state(ps::RESOURCE_FOOD_CAP));
        let [r, g, bl] = team_rgb[pl.color.clamp(0, 15) as usize];
        let food_color = match w.upkeep_level(p as u8) {
            1 => [0xff, 0xff, 0, 0xff],
            2 => [0xff, 0, 0, 0xff],
            _ => [0, 0xff, 0, 0xff],
        };
        let values = [
            (pl.name.clone(), [(r * 255.0) as u8, (g * 255.0) as u8, (bl * 255.0) as u8, 0xff]),
            (pl.get_state(ps::RESOURCE_GOLD).to_string(), [0xff; 4]),
            (pl.get_state(ps::RESOURCE_LUMBER).to_string(), [0xff; 4]),
            (if cap == 0 { used.to_string() } else { format!("{used}/{cap}") }, food_color),
        ];
        for (col, (value, color)) in values.into_iter().enumerate() {
            b.items[row][col] = MultiboardItem { value, icon: icons[col].clone(), color, show_value: true, show_icon: col != 0, width: widths[col] };
        }
    }
    Some(b)
}

fn shown<'a>(w: &'a World, db: &FrameDb, team_rgb: &[[f32; 3]; 16], team_minimized: bool) -> Shown<'a> {
    let mut dialogs: Vec<(u32, &TimerDialog)> = w.handles.iter().filter_map(|(&h, o)| match o {
        Obj::TimerDialog(d) if d.displayed => Some((h, d)),
        _ => None,
    }).collect();
    dialogs.sort_by_key(|d| d.0);
    let multiboard = if w.multiboard_suppressed {
        None
    } else {
        // A map's multiboard is shown over the Team Resources board.
        w.handles.iter().find_map(|(&h, o)| match o {
            Obj::Multiboard(b) if b.displayed => Some((h, std::borrow::Cow::Borrowed(b))),
            _ => None,
        }).or_else(|| team_resources(w, db, team_rgb, team_minimized).map(|b| (TEAM_RESOURCES, std::borrow::Cow::Owned(b))))
    };
    let leaderboard = w.player_leaderboards.get(&w.local_player).and_then(|&h| match w.handles.get(h) {
        Some(Obj::Leaderboard(b)) if b.displayed => Some(b),
        _ => None,
    });
    Shown { dialogs: dialogs.into_iter().map(|(_, d)| (d, w.timer_dialog_text(d))).collect(), multiboard, leaderboard }
}

pub fn sync(view: &mut Option<BoardsView>, db: &FrameDb, w: &World, team_rgb: &[[f32; 3]; 16], aspect: f32, team_minimized: bool) {
    let s = shown(w, db, team_rgb, team_minimized);
    if s.dialogs.is_empty() && s.multiboard.is_none() && s.leaderboard.is_none() {
        *view = None;
        return;
    }
    let key = format!("{aspect}{:?}{:?}{:?}", s.dialogs, s.multiboard, s.leaderboard);
    if view.as_ref().is_some_and(|v| v.key == key) {
        return;
    }
    let player_rgb = |p: u8| {
        let c = w.players.get(p as usize).map_or(p as i32, |pl| pl.color).clamp(0, 15) as usize;
        let [r, g, b] = team_rgb[c];
        [r, g, b, 1.0]
    };
    let mut t = FrameTree::new(aspect);
    let (wx, wy) = world_top_right(&t);
    let font = db.decorate("MasterFont");
    let lb_size = db.misc_ui.get_str("FontHeights", "LeaderBoard").and_then(|v| v.trim().parse().ok()).unwrap_or(0.011f32);
    let shadow = Some(([0.0, 0.0, 0.0, 1.0], 0.001, -0.001));

    for (k, (d, text)) in s.dialogs.iter().enumerate() {
        let scope = format!("#{k}");
        if t.create_scoped(db, "TimerDialog", 0, &scope).is_none() {
            continue;
        }
        if let Some(title) = &d.title {
            t.set_text(&format!("TimerDialogTitle{scope}"), title);
        }
        t.set_text(&format!("TimerDialogValue{scope}"), text);
        if let (Some(c), Some(i)) = (d.title_color, t.get(&format!("TimerDialogTitle{scope}"))) {
            t.frames[i].color = Some(color(c));
        }
        if let (Some(c), Some(i)) = (d.time_color, t.get(&format!("TimerDialogValue{scope}"))) {
            t.frames[i].color = Some(color(c));
        }
        let name = format!("TimerDialog{scope}");
        if k == 0 {
            t.set_point(&name, P::TopRight, "", P::TopRight, wx - 0.005, wy - 0.015);
        } else {
            t.set_point(&name, P::Right, &format!("TimerDialog#{}", k - 1), P::Left, -0.002, 0.0);
        }
    }
    let first_dialog = (!s.dialogs.is_empty()).then_some("TimerDialog#0");

    // CMultiboard: the title backdrop as wide as the title plus 0.03, or the cells (each
    // column's width plus 0.002; FUN_6f3dd850 takes off a width that measures nothing
    // against the original's screen); the backdrop the rows' height (0.013 each, 0.001 apart, plus
    // 0.012); the button shows MultiboardMaximizeButtonEnabled while minimized.
    let mut mb_rows: Option<(&Multiboard, f32)> = None;
    if let Some((_, b)) = s.multiboard.as_ref().map(|(h, b)| (h, b.as_ref())) {
        if t.create(db, "Multiboard", 0).is_some() {
            t.set_text("MultiboardTitle", &b.title);
            if let (Some(c), Some(i)) = (b.title_color, t.get("MultiboardTitle")) {
                t.frames[i].color = Some(color(c));
            }
            match first_dialog {
                Some(d) => t.set_point("Multiboard", P::TopRight, d, P::BottomRight, 0.0, -0.001),
                None => t.set_point("Multiboard", P::TopRight, "", P::TopRight, wx + 0.0001, wy - 0.0032),
            }
            let cells: f32 = b.items.first().map_or(0.0, |r| r.iter().map(|c| c.width + 0.002).sum());
            let height = (0.013 + 0.001) * b.rows as f32 + 0.001 + 0.011;
            let title_w = db.text_width(&font, 0.011, &b.title) + 0.03;
            let title_w = if b.minimized { title_w } else { title_w.max(cells) };
            t.clear_points("MultiboardTitleBackdrop");
            t.set_point("MultiboardTitleBackdrop", P::TopRight, "MultiboardMinimizeButton", P::TopLeft, 0.0057, 0.0);
            t.set_point("MultiboardTitleBackdrop", P::BottomRight, "MultiboardMinimizeButton", P::BottomLeft, 0.0057, 0.0);
            t.set_size("MultiboardTitleBackdrop", Some(title_w), None);
            t.set_size("MultiboardBackdrop", None, Some(height));
            t.set_visible("MultiboardBackdrop", !b.minimized);
            t.set_visible("MultiboardListContainer", !b.minimized);
            if b.minimized {
                set_prop(&mut t, "ButtonBackdropTemplate", "BackdropBackground", "MultiboardMaximizeButtonEnabled");
            } else {
                mb_rows = Some((b, 0.0));
            }
        }
    }

    // CLeaderboard: 0.043 taller than its rows (0.035 without the label), at most 24 rows
    // of 0.012, 0.001 apart, below a 0.001 inset.
    if let Some(b) = s.leaderboard {
        if t.create(db, "Leaderboard", 0).is_some() {
            t.set_text("LeaderboardTitle", &b.label);
            t.set_visible("LeaderboardTitle", b.style[0]);
            let anchor = match (s.multiboard.as_ref(), first_dialog) {
                (Some((_, m)), _) => Some((if m.minimized { "MultiboardMinimizeButton" } else { "MultiboardBackdrop" }, -0.005)),
                (None, Some(d)) => Some((d, -0.001)),
                (None, None) => None,
            };
            match anchor {
                Some((rel, dy)) => t.set_point("Leaderboard", P::TopRight, rel, P::BottomRight, 0.0, dy),
                None => t.set_point("Leaderboard", P::TopRight, "", P::TopRight, wx - 0.005, wy - 0.015),
            }
            let n = b.size.min(24) as f32;
            let extra = if b.style[0] { 0.043 } else { 0.035 };
            t.set_size("Leaderboard", None, Some(0.001 + n * (0.012 + 0.001) + extra));
        }
    }
    t.bind_points();
    t.layout();

    let mut extra = Vec::new();
    let text = |out: &mut Vec<Draw>, s: &str, rect: Rect, color: [f32; 4], justify_h: w3_ui::Justify| {
        out.push(Draw::Text { font: font.clone(), size: lb_size, rect, text: s.to_string(), color, justify_h, justify_v: w3_ui::Justify::Middle, shadow });
    };
    let image = |out: &mut Vec<Draw>, texture: String, rect: Rect| {
        out.push(Draw::Image { texture, rect, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
    };
    // CLeaderboardListBoxItem: the player's color in a 0.01 square under a 0.015
    // EscMenuCheckBoxPushedBackground, centered 0.005 inside the right; the value
    // right-aligned 0.004 left of it (or 0.005 inside the right), the label from 0.005
    // inside the left to 0.02 short of the value (or icon).
    if let (Some(b), Some(c)) = (s.leaderboard, t.get("LeaderboardListContainer").and_then(|i| t.frames[i].rect)) {
        for (k, it) in b.items.iter().take(24).enumerate() {
            let y1 = c.y1 - 0.001 - k as f32 * 0.013;
            let row = Rect { x0: c.x0 + 0.01, x1: c.x1 - 0.01, y0: y1 - 0.012, y1 };
            let cy = (row.y0 + row.y1) / 2.0;
            let cx = row.x1 - 0.005;
            let mut right = row.x1 - 0.005;
            if it.style[2] {
                let color = w.players.get(it.player as usize).map_or(it.player as i32, |p| p.color);
                image(&mut extra, format!("ReplaceableTextures\\TeamColor\\TeamColor{:02}.blp", color.clamp(0, 99)), Rect { x0: cx - 0.005, x1: cx + 0.005, y0: cy - 0.005, y1: cy + 0.005 });
                image(&mut extra, db.decorate("EscMenuCheckBoxPushedBackground"), Rect { x0: cx - 0.0075, x1: cx + 0.0075, y0: cy - 0.0075, y1: cy + 0.0075 });
                right = cx - 0.005 - 0.004;
            }
            if it.style[1] {
                let v = it.value.to_string();
                let w = db.text_width(&font, lb_size, &v);
                let c = it.value_color.map(color).unwrap_or_else(|| player_rgb(it.player));
                text(&mut extra, &v, Rect { x0: right - w - 0.001, x1: right, y0: row.y0, y1: row.y1 }, c, w3_ui::Justify::End);
                right -= w;
            }
            if it.style[0] {
                let x1 = if it.style[1] || it.style[2] { right - 0.02 } else { right };
                let c = it.label_color.map(color).unwrap_or_else(|| player_rgb(it.player));
                text(&mut extra, &it.label, Rect { x0: row.x0 + 0.005, x1, y0: row.y0, y1: row.y1 }, c, w3_ui::Justify::Start);
            }
        }
    }
    // CMultiboardListBoxRow: cells 0.001 apart from 0.001 inside the row, each its
    // width; an 0.011 icon centered 0.005 inside its left, the value from 0.005 right of
    // the icon (or the cell's left) to its right.
    if let (Some((b, _)), Some(c)) = (mb_rows, t.get("MultiboardListContainer").and_then(|i| t.frames[i].rect)) {
        for (r, row) in b.items.iter().enumerate() {
            let y1 = c.y1 - 0.001 - r as f32 * 0.014;
            let (y0, cy) = (y1 - 0.013, y1 - 0.0065);
            let mut x = c.x0 + 0.01 + 0.001;
            for it in row {
                let cell = Rect { x0: x, x1: x + it.width, y0, y1 };
                let mut left = cell.x0;
                if it.show_icon {
                    let icon = if it.icon.is_empty() { db.decorate("MultiboardDefaultIcon") } else { it.icon.clone() };
                    let ix = cell.x0 + 0.005;
                    image(&mut extra, icon, Rect { x0: ix - 0.0055, x1: ix + 0.0055, y0: cy - 0.0055, y1: cy + 0.0055 });
                    left = ix + 0.0055 + 0.005;
                }
                if it.show_value && !it.value.is_empty() {
                    text(&mut extra, &it.value, Rect { x0: left, x1: cell.x1.max(left), y0, y1 }, color(it.color), w3_ui::Justify::Start);
                }
                x = cell.x1 + 0.001;
            }
        }
    }
    *view = Some(BoardsView { key, tree: t, extra, multiboard: s.multiboard.as_ref().map(|m| m.0) });
}

impl BoardsView {
    pub fn draws(&self, db: &FrameDb, out: &mut Vec<Draw>) {
        out.extend(self.tree.draw(db));
        out.extend(self.extra.iter().cloned());
    }

    /// The multiboard whose minimize button is under a point.
    pub fn minimize_at(&self, x: f32, y: f32) -> Option<u32> {
        let i = self.tree.get("MultiboardMinimizeButton")?;
        self.tree.frames[i].rect.filter(|r| r.contains(x, y)).and(self.multiboard)
    }
}
