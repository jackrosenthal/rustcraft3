//! The score screen (CScoreScreen, UI\FrameDef\Glue\ScoreScreen.fdf): four tabs of
//! columns, a row per player, sorted by the checked column.  The rows are bar models
//! (ScoreScreenBarTop, then ScoreScreen-CenterBar, then ScoreScreen-BottomBar, each in
//! the player's TeamColorNN sequence) with the win or loss banner, the race icon, the
//! team number, the name, and the values on them (FUN_6f3052e0, FUN_6f30d290,
//! FUN_6f306650, FUN_6f3074b0, FUN_6f303f60).

use w3_game::score::kind;
use w3_game::world::World;

/// A player's row: who, how it ended, and each tab's columns.
#[derive(Clone, Debug)]
pub struct Row {
    pub player: u8,
    pub name: String,
    pub race: i32,
    pub color: i32,
    pub team: i32,
    pub won: bool,
    /// The local player's own row, and its allies'.
    pub local: bool,
    pub ally: bool,
    /// Per tab, the values (traded resources show as given/received).
    pub values: [[i32; 5]; 4],
    pub traded: (i32, i32),
    /// The heroes' icons and levels (at most three shown).
    pub heroes: Vec<(String, i32)>,
}

#[derive(Clone, Debug)]
pub struct ScoreData {
    pub rows: Vec<Row>,
    /// Whether the local player won (the race art shows victory, else defeat).
    pub victory: bool,
    pub local_race: i32,
    pub elapsed: f32,
    /// Whether the game kept a replay to save (not a campaign's, nor under
    /// DoNotSaveReplay): otherwise Save Replay is hidden (FUN_6f2dc950).
    pub replay: bool,
}

/// The column the tab sorts by until another is chosen (as captured from the original).
pub const DEFAULT_SORT: [usize; 4] = [3, 4, 0, 0];

/// Each tab's column header strings.
pub const HEADERS: [&str; 4] = ["OVERVIEW_COLUMN", "UNIT_COLUMN", "HEROES_COLUMN", "RESOURCES_COLUMN"];

impl ScoreData {
    /// The players on the score screen (SetPlayerOnScoreScreen) that took part.
    pub fn from_world(w: &World) -> ScoreData {
        let mut rows = Vec::new();
        for (i, p) in w.players.iter().enumerate().take(12) {
            if !p.on_score_screen || p.slot_state == w3_game::types::slot_state::EMPTY {
                continue;
            }
            let player = i as u8;
            let s = |k| w.player_score(player, k);
            let sc = &p.score;
            let heroes: Vec<(String, i32)> = sc
                .heroes
                .iter()
                .filter_map(|&h| w.unit(h).map(|u| (u.type_id, u.level)).or_else(|| sc.hero_cache.get(&h).copied()))
                .map(|(t, level)| (w.objects.units.get(t).map(|o| o.str("Art")).unwrap_or_default(), level))
                .collect();
            rows.push(Row {
                player,
                name: p.name.clone(),
                race: p.race,
                color: p.color,
                team: p.team,
                won: w.player_results.get(&player) == Some(&0),
                local: player == w.local_player,
                ally: player != w.local_player && w3_game::natives::players::is_ally(w, w.local_player, player),
                values: [
                    [s(kind::UNIT_TOTAL), s(kind::HERO_TOTAL), s(kind::RESOURCE_TOTAL), s(kind::TOTAL), 0],
                    [s(kind::UNITS_TRAINED), s(kind::UNITS_KILLED), s(kind::STRUCT_BUILT), s(kind::STRUCT_RAZED), s(kind::FOOD_MAXUSED)],
                    [sc.heroes.len() as i32, s(kind::HEROES_KILLED), s(kind::ITEMS_GAINED), s(kind::MERCS_HIRED), sc.experience],
                    [s(kind::GOLD_MINED_TOTAL), s(kind::LUMBER_TOTAL), 0, s(kind::TECH_PERCENT), s(kind::GOLD_LOST_UPKEEP)],
                ],
                traded: (sc.gold_given + sc.lumber_given, sc.gold_received + sc.lumber_received),
                heroes,
            });
        }
        let local = w.local_player;
        ScoreData {
            rows,
            victory: w.player_results.get(&local) == Some(&0),
            local_race: w.players.get(local as usize).map_or(1, |p| p.race),
            elapsed: w.time,
            replay: !w.no_replay,
        }
    }

    /// The rows in display order: by the column, highest first, then by player; by team
    /// first when teams are grouped.
    pub fn order(&self, tab: usize, column: usize, teams: bool) -> Vec<usize> {
        let mut idx: Vec<usize> = (0..self.rows.len()).collect();
        idx.sort_by(|&a, &b| {
            let (ra, rb) = (&self.rows[a], &self.rows[b]);
            let team = if teams { ra.team.cmp(&rb.team) } else { std::cmp::Ordering::Equal };
            team.then(rb.values[tab][column].cmp(&ra.values[tab][column])).then(ra.player.cmp(&rb.player))
        });
        idx
    }
}

impl Row {
    /// The bar's color: the player's, or with ally colors the local player's blue, its
    /// allies' teal, and others' red (as captured from the original).
    pub fn bar_color(&self, ally_colors: bool) -> i32 {
        match (ally_colors, self.local, self.ally) {
            (false, ..) => self.color,
            (true, true, _) => 1,
            (true, false, true) => 2,
            _ => 0,
        }
    }
}

/// common.j RACE_* as the skins' ScoreScreenPlayer<race> suffix.
pub fn race_word(race: i32) -> &'static str {
    match race {
        2 => "Orc",
        3 => "Undead",
        4 => "NightElf",
        _ => "Human",
    }
}

/// The skins section of a race (war3skins.txt), for the race's victory art.
pub fn race_skin(race: i32) -> &'static str {
    race_word(race)
}

/// H:MM:SS.
pub fn elapsed_text(secs: f32) -> String {
    let s = secs.max(0.0) as u32;
    format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

/// The rows' bars: each row's top and bottom (interface units, from the bottom) and the
/// model and its height.  The first is ScoreScreenBarTop (0.0373 tall, its top 0.11 below
/// the screen's), middle rows CenterBar (0.0328) with their tops 0.0015 above the bottom
/// of the row before, and the last BottomBar, its bottom 0.00075 above the row before's.
pub fn row_bars(n: usize, top: f32) -> Vec<(&'static str, f32, f32)> {
    let mut out: Vec<(&'static str, f32, f32)> = Vec::new();
    if n == 0 {
        return out;
    }
    let t0 = top - 0.11;
    out.push(("ScoreScreenBarTop", t0, t0 - 0.0373));
    if n > 1 {
        for _ in 1..n - 1 {
            let prev = out.last().unwrap().2;
            let t = prev + 0.0015;
            out.push(("UI\\Glues\\ScoreScreen\\ScoreScreen-CenterBar\\ScoreScreen-CenterBar.mdl", t, t - 0.0328));
        }
        let prev = out.last().unwrap().2;
        let b = prev + 0.00075;
        out.push(("UI\\Glues\\ScoreScreen\\ScoreScreen-BottomBar\\ScoreScreen-BottomBar.mdl", b + 0.0375, b));
    }
    out
}

/// A row's vertical offset from its bar's middle (FUN_6f2df310).
pub fn row_offset(k: usize, n: usize) -> f32 {
    if k == 0 {
        -0.004
    } else if k + 1 == n {
        -0.037
    } else {
        -0.002
    }
}

/// The columns' centers from the screen's middle: the four-column tab's (the FDF's
/// ScoreScreen4ColumnButton positions) and the others' (0x6faa97b0).
pub const COLUMNS_4: [f32; 4] = [-0.172, -0.0273, 0.1182, 0.2647];
pub const COLUMNS_5: [f32; 5] = [-0.1865, -0.0703, 0.0462, 0.1627, 0.28];
