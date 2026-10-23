//! The campaign screen (CCampaignMenu): the campaigns from war3skins.txt's
//! CampaignFile_V<n> (UI\CampaignStrings.txt or _exp), each with its backdrop model,
//! fog, and missions; a list of the open campaigns, then of a campaign's open missions
//! (CCampaignListBox).  Progress comes from the profile.

use w3_formats::profile::Profile;
use w3_game::persist::Persist;
use w3_ui::{Draw, FrameDb, Rect};

#[derive(Clone, Debug, Default)]
pub struct Mission {
    pub header: String,
    pub name: String,
    pub map: String,
}

#[derive(Clone, Debug, Default)]
pub struct Campaign {
    pub header: String,
    pub name: String,
    pub default_open: bool,
    /// The war3skins.txt key of the backdrop model (read with the version suffix).
    pub background: String,
    /// BackgroundFogStart, End, Density, and Color (alpha, red, green, blue).
    pub fog: (f32, f32, f32, [f32; 4]),
    pub missions: Vec<Mission>,
    /// A custom campaign's backdrop model, in place of `background`.
    pub model: Option<String>,
}

/// A custom campaign open on the campaign screen: its file, and which mission buttons
/// show from the start.
pub struct CustomCampaign {
    pub path: std::path::PathBuf,
    pub visible: Vec<bool>,
    /// The game's own campaigns, back when the custom one closes.
    saved: Vec<Option<Campaign>>,
}

/// One row of the list box.
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    Blank,
    Campaign(usize),
    Mission(usize, usize),
}

#[derive(Default)]
pub struct Campaigns {
    /// By campaign number (position in CampaignList); empty entries are None.
    pub list: Vec<Option<Campaign>>,
    /// The campaign whose missions are listed, if any.
    pub open: Option<usize>,
    /// The custom campaign shown in place of the game's, if any.
    pub custom: Option<CustomCampaign>,
}

/// CCampaignListBox's rows: CampaignItemHeader and CampaignItemName text beside a
/// 0.032 arrow button 0.004 inside the box's left; rows 0.0321 apart, the first row's
/// top at 0.3318 (measured from the original's campaign screen).
const ROW_PITCH: f32 = 0.0321;
const FIRST_ROW_TOP: f32 = 0.3318;
/// The list box: 0.28 wide with its right 0.01 inside the menu's (CCampaignMenu::vf55).
const LIST_LEFT: f32 = 0.8 - 0.01 - 0.28;

impl Campaigns {
    pub fn load(chain: &w3_mpq::Chain, db: &FrameDb) -> Campaigns {
        let Some(path) = db.skin_value("CampaignFile") else { return Campaigns::default() };
        let p = Profile::parse(&chain.read_string(path.trim_matches('"')).unwrap_or_default());
        let list = p
            .get_list("Index", "CampaignList")
            .iter()
            .map(|section| {
                let section = section.trim().trim_matches('"');
                if section.is_empty() {
                    return None;
                }
                let s = |k: &str| p.get_str(section, k).unwrap_or_default().trim_matches('"').to_string();
                let num = |k: &str| s(k).trim().parse::<f32>().unwrap_or(0.0);
                let color: Vec<f32> = p.get_list(section, "BackgroundFogColor").iter().filter_map(|v| v.trim().parse().ok()).collect();
                let color = if color.len() == 4 { [color[1] / 255.0, color[2] / 255.0, color[3] / 255.0, color[0] / 255.0] } else { [0.0; 4] };
                let mut missions = Vec::new();
                for k in 0..128 {
                    let m = p.get_list(section, &format!("Mission{k}"));
                    if m.len() < 3 {
                        break;
                    }
                    let unq = |v: &String| v.trim().trim_matches('"').to_string();
                    missions.push(Mission { header: unq(&m[0]), name: unq(&m[1]), map: unq(&m[2]) });
                }
                Some(Campaign {
                    header: s("Header"),
                    name: s("Name"),
                    default_open: s("DefaultOpen") == "1",
                    background: s("Background"),
                    fog: (num("BackgroundFogStart"), num("BackgroundFogEnd"), num("BackgroundFogDensity"), color),
                    missions,
                    model: None,
                })
            })
            .collect();
        Campaigns { list, open: None, custom: None }
    }

    /// Shows a custom campaign's missions (CCustomCampaignMenu's OK): its backdrop is
    /// UI\WorldEditData.txt's [CampaignScreens] entry, or its own model, under its fog.
    pub fn open_custom(&mut self, path: &std::path::Path, s: &crate::skirmish::CampaignSummary, world_edit: &Profile) {
        self.close_custom();
        let info = &s.info;
        let model = if info.background < 0 {
            Some(info.background_path.clone()).filter(|p| !p.is_empty())
        } else {
            world_edit.get_list("CampaignScreens", &format!("{:02}", info.background)).get(3).map(|p| p.trim().to_string())
        };
        let fog = if info.fog.0 >= 0 {
            let c = info.fog.4;
            (info.fog.1, info.fog.2, info.fog.3, [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, c[3] as f32 / 255.0])
        } else {
            (0.0, 0.0, 0.0, [0.0; 4])
        };
        let missions = info.missions.iter().map(|m| Mission { header: s.text(&m.chapter), name: s.text(&m.title), map: m.map.clone() }).collect();
        let campaign = Campaign { header: String::new(), name: s.text(&info.name), default_open: true, background: String::new(), fog, missions, model };
        let saved = std::mem::replace(&mut self.list, vec![Some(campaign)]);
        self.custom = Some(CustomCampaign { path: path.to_path_buf(), visible: info.missions.iter().map(|m| m.visible).collect(), saved });
        self.open = Some(0);
    }

    /// Puts the game's campaigns back.
    pub fn close_custom(&mut self) {
        if let Some(c) = self.custom.take() {
            self.list = c.saved;
            self.open = None;
        }
    }

    pub fn available(&self, persist: &Persist, c: usize) -> bool {
        self.list.get(c).and_then(|e| e.as_ref()).is_some_and(|e| e.default_open || persist.campaigns.get(c).is_some_and(|s| s.available))
    }

    /// The first mission is open with its campaign; the rest as games open them.
    pub fn mission_available(&self, persist: &Persist, c: usize, m: usize) -> bool {
        // A custom campaign's buttons show from the start or once a mission shows them
        // (SetCustomCampaignButtonVisible).
        if let Some(cc) = &self.custom {
            return cc.visible.get(m).copied().unwrap_or(false) || persist.custom_campaign_buttons.get(&(m as i32)).copied().unwrap_or(false);
        }
        self.available(persist, c) && (m == 0 || persist.mission_open(c, m))
    }

    /// The campaign the screen's backdrop shows: the one the profile names
    /// (SetCampaignMenuRaceEx), else the first open one.
    pub fn backdrop_campaign(&self, persist: &Persist) -> Option<usize> {
        if self.custom.is_some() {
            return Some(0);
        }
        let named = persist.menu_race.max(0) as usize;
        if persist.menu_race > 0 && self.available(persist, named) {
            return Some(named);
        }
        (0..self.list.len()).find(|&c| self.available(persist, c))
    }

    /// Rows shown: open campaigns (locked ones leave a blank row), or the open
    /// missions of the campaign being looked at.
    pub fn rows(&self, persist: &Persist) -> Vec<Row> {
        match self.open {
            Some(c) => {
                let n = self.list.get(c).and_then(|e| e.as_ref()).map_or(0, |e| e.missions.len());
                (0..n).filter(|&m| self.mission_available(persist, c, m)).map(|m| Row::Mission(c, m)).collect()
            }
            None => self
                .list
                .iter()
                .enumerate()
                .filter(|(_, e)| e.is_some())
                .map(|(c, _)| if self.available(persist, c) { Row::Campaign(c) } else { Row::Blank })
                .collect(),
        }
    }

    fn row_rect(root: Rect, k: usize) -> Rect {
        let top = root.y0 + FIRST_ROW_TOP - k as f32 * ROW_PITCH;
        Rect { x0: root.x0 + LIST_LEFT, x1: root.x0 + LIST_LEFT + 0.28, y0: top - ROW_PITCH, y1: top }
    }

    /// The row under an interface point.
    pub fn row_at(&self, persist: &Persist, root: Rect, x: f32, y: f32) -> Option<Row> {
        self.rows(persist).into_iter().enumerate().find(|(k, r)| *r != Row::Blank && Self::row_rect(root, *k).contains(x, y)).map(|(_, r)| r)
    }

    pub fn draws(&self, persist: &Persist, db: &FrameDb, root: Rect, hover: Option<&Row>, out: &mut Vec<Draw>) {
        let font = db.decorate("MasterFont");
        let height = |k: &str, d: f32| db.misc_ui.get_str("FontHeights", k).and_then(|v| v.trim().parse().ok()).unwrap_or(d);
        let (hh, nh) = (height("CampaignItemHeader", 0.011), height("CampaignItemName", 0.015));
        let shadow = Some(([0.0, 0.0, 0.0, 1.0], 0.001, -0.001));
        for (k, row) in self.rows(persist).iter().enumerate() {
            let (header, name, button) = match row {
                Row::Blank => continue,
                Row::Campaign(c) => {
                    let e = self.list[*c].as_ref().unwrap();
                    (e.header.clone(), e.name.clone(), "CampaignButtonCampaign")
                }
                Row::Mission(c, m) => {
                    let e = &self.list[*c].as_ref().unwrap().missions[*m];
                    let interlude = e.header.eq_ignore_ascii_case("Interlude");
                    (e.header.clone(), e.name.clone(), if interlude { "CampaignButtonInterlude" } else { "CampaignButtonMission" })
                }
            };
            let r = Self::row_rect(root, k);
            let cy = (r.y0 + r.y1) / 2.0;
            let b = Rect { x0: r.x0 + 0.004, x1: r.x0 + 0.036, y0: cy - 0.016, y1: cy + 0.016 };
            let key = if hover == Some(row) { format!("{button}Down") } else { button.to_string() };
            out.push(Draw::Image { texture: db.decorate(&key).trim_matches('"').to_string(), rect: b, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
            // The header 0.008 right of the button and 0.003125 below its top in gold
            // (0xFCD312), the name under it in light gray (0xD2D2D2).
            let hx = b.x1 + 0.008;
            let hy = b.y1 - 0.003125;
            out.push(Draw::Text { font: font.clone(), size: hh, rect: Rect { x0: hx, x1: r.x1, y0: hy - hh * 1.2, y1: hy }, text: header, color: [0xfc as f32 / 255.0, 0xd3 as f32 / 255.0, 0x12 as f32 / 255.0, 1.0], justify_h: w3_ui::Justify::Start, justify_v: w3_ui::Justify::Start, shadow });
            let ny = hy - hh;
            out.push(Draw::Text { font: font.clone(), size: nh, rect: Rect { x0: hx, x1: r.x1, y0: ny - nh * 1.2, y1: ny }, text: name, color: [0xd2 as f32 / 255.0, 0xd2 as f32 / 255.0, 0xd2 as f32 / 255.0, 1.0], justify_h: w3_ui::Justify::Start, justify_v: w3_ui::Justify::Start, shadow });
        }
    }
}
