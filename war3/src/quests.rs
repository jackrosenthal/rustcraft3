//! The quest log (CQuestDialog, UI\FrameDef\UI\QuestDialog.fdf), opened by the Quests
//! button or F9.  The required quests list under QUESTSMAIN and the optional ones under
//! QUESTSOPTIONAL, each enabled quest as a QuestListItem 0.035 tall from its list's top
//! (0.001 in): those under way, then the failed, then the completed (FUN_6f3d7950).  A
//! quest shows its icon, its title cut to 0.145, and QUESTCOMPLETED or QUESTFAILED with
//! its button pushed (completed) or pushed and disabled (failed) (CQuestListBoxItem
//! vf70); an undiscovered one shows UndiscoveredQuestIcon and UNDISCOVERED_QUEST.  The
//! chosen quest's title (cut to 0.25), its requirements (" - " and the text, with
//! " ( QUESTCOMPLETED )" and greyed once met, 0.013 apart), and its description fill
//! the lower box (FUN_6f3c5760, CQuestItemListBoxItem vf70).

use w3_game::types::{Obj, Quest};
use w3_game::world::World;
use w3_ui::{Draw, FrameDb, FramePoint as P, FrameTree};

const ROW: f32 = 0.035;
const ITEM_ROW: f32 = 0.013;
const INSET: f32 = 0.001;

pub struct QuestView {
    pub tree: FrameTree,
    /// The quests in each list (required, optional), in display order.
    lists: [Vec<u32>; 2],
    selected: Option<u32>,
    /// The number of requirement rows made.
    item_rows: usize,
    /// Each listed quest's completed and failed states, as last shown.
    states: Vec<(u32, bool, bool)>,
    /// A click changed the choice; shown at the next sync.
    pub dirty: bool,
}

fn quest(world: &World, h: u32) -> Option<&Quest> {
    match world.handles.get(h) {
        Some(Obj::Quest(q)) => Some(q),
        _ => None,
    }
}

/// `text` cut to `width` as FUN_6f2c0160 does: what fits, or what fits less three
/// characters and "...".
fn fit(db: &FrameDb, font: &(String, f32), text: &str, width: f32) -> String {
    if db.text_width(&font.0, font.1, text) <= width {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut n = chars.len();
    while n > 0 && db.text_width(&font.0, font.1, &chars[..n].iter().collect::<String>()) > width {
        n -= 1;
    }
    let keep = n.saturating_sub(3);
    format!("{}...", chars[..keep].iter().collect::<String>())
}

impl QuestView {
    /// The dialog centered 0.064 above the screen's middle (CQuestMode vf9), titled by
    /// the map's loading screen title and subtitle when it has both.
    pub fn open(db: &FrameDb, aspect: f32, world: &World, titles: (&str, &str)) -> Option<QuestView> {
        let mut tree = FrameTree::new(aspect);
        tree.plain_button_text = true;
        tree.create(db, "QuestDialog", 0)?;
        tree.set_point("QuestDialog", P::Center, "", P::Center, 0.0, 0.064);
        // The map info's loading screen title and subtitle (read by FUN_6f78b790), else a
        // blank title over QUESTS (CQuestDialog vf55).
        let (title, subtitle) = (world.localized(titles.0), world.localized(titles.1));
        if title.is_empty() || subtitle.is_empty() {
            tree.set_text("QuestTitleValue", " ");
            tree.set_text("QuestSubtitleValue", &db.string("QUESTS"));
        } else {
            tree.set_text("QuestTitleValue", &title);
            tree.set_text("QuestSubtitleValue", &subtitle);
        }
        let mut lists = [Vec::new(), Vec::new()];
        for (k, list) in lists.iter_mut().enumerate() {
            let required = k == 0;
            let of = |pred: &dyn Fn(&Quest) -> bool| -> Vec<u32> {
                world.quests.iter().copied().filter(|&h| quest(world, h).is_some_and(|q| q.enabled && q.required == required && pred(q))).collect()
            };
            list.extend(of(&|q| !q.completed && !q.failed));
            list.extend(of(&|q| q.failed && !q.completed));
            list.extend(of(&|q| q.completed));
        }
        let mut serial = 0;
        for (k, list) in lists.iter().enumerate() {
            let container = if k == 0 { "QuestMainContainer" } else { "QuestOptionalContainer" };
            let Some(c) = tree.get(container) else { continue };
            for (row, _) in list.iter().enumerate() {
                let scope = format!("#{serial}");
                serial += 1;
                tree.create_scoped(db, "QuestListItem", c, &scope);
                let item = format!("QuestListItem{scope}");
                tree.set_point(&item, P::TopLeft, container, P::TopLeft, INSET, -INSET - ROW * row as f32);
                tree.set_point(&item, P::TopRight, container, P::TopRight, -INSET, -INSET - ROW * row as f32);
                tree.set_size(&item, None, Some(ROW));
            }
        }
        // The list scroll bars stay hidden until a list outgrows its box.
        for n in ["QuestMainListScrollBar", "QuestOptionalListScrollBar", "QuestItemListScrollBar"] {
            tree.set_visible(n, false);
        }
        let mut view = QuestView { tree, lists, selected: None, item_rows: 0, states: Vec::new(), dirty: false };
        view.selected = view.lists[0].first().or(view.lists[1].first()).copied();
        view.tree.place_text_area_scroll_bars();
        view.tree.bind_points();
        view.tree.fit_text_widths();
        view.sync(db, world);
        view.tree.layout();
        Some(view)
    }

    fn item_names(&self) -> Vec<(u32, String)> {
        self.lists.iter().flatten().enumerate().map(|(i, &h)| (h, format!("#{i}"))).collect()
    }

    /// Shows each quest's state and the chosen one's details.
    pub fn sync(&mut self, db: &FrameDb, world: &World) {
        let title_font = self.item_names().first().and_then(|(_, s)| self.tree.text_font(db, &format!("QuestListItemTitle{s}"))).unwrap_or((db.decorate("MasterFont"), 0.012));
        let completed_text = db.string("QUESTCOMPLETED");
        let failed_text = db.string("QUESTFAILED");
        self.states.clear();
        for (h, s) in self.item_names() {
            let Some(q) = quest(world, h) else { continue };
            self.states.push((h, q.completed, q.failed));
            let icon = if q.discovered { q.icon.clone() } else { db.decorate("UndiscoveredQuestIcon") };
            self.tree.set_texture(&format!("QuestListItemIconContainer{s}"), &icon);
            let title = if q.discovered { q.title.clone() } else { db.string("UNDISCOVERED_QUEST") };
            self.tree.set_text(&format!("QuestListItemTitle{s}"), &fit(db, &title_font, &title, 0.145));
            let button = format!("QuestListItemButton{s}");
            if let Some(b) = self.tree.get(&button) {
                let f = &mut self.tree.frames[b];
                f.pushed = q.completed || q.failed;
                f.enabled = q.completed || !q.failed;
            }
            let done = format!("QuestListItemComplete{s}");
            self.tree.set_visible(&done, q.completed || q.failed);
            self.tree.set_text(&done, if q.completed { &completed_text } else { &failed_text });
        }
        // The chosen quest.
        let chosen = self.selected.and_then(|h| quest(world, h));
        let details_font = self.tree.text_font(db, "QuestDetailsTitle").unwrap_or((db.decorate("MasterFont"), 0.013));
        let (title, text) = match chosen {
            Some(q) if q.discovered => (q.title.clone(), q.description.clone()),
            Some(_) => (db.string("UNDISCOVERED_QUEST"), db.string("QUESTNOTDISCOVERED")),
            None => (db.string("UNDISCOVERED_QUEST"), String::new()),
        };
        self.tree.set_text("QuestDetailsTitle", &fit(db, &details_font, &title, 0.25));
        self.tree.set_text("QuestDisplay", &text);
        let items: Vec<(String, bool)> = chosen
            .filter(|q| q.discovered)
            .map(|q| {
                q.items
                    .iter()
                    .filter_map(|&i| match world.handles.get(i) {
                        Some(Obj::QuestItem(it)) => Some(it),
                        _ => None,
                    })
                    .map(|it| {
                        let mut line = format!(" - {}", it.description);
                        if it.completed {
                            line.push_str(&format!(" ( {completed_text} )"));
                        }
                        (line, it.completed)
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.show_items(db, &items);
    }

    /// The chosen quest's requirement rows (QuestItemListItem, 0.013 apart, 0.001 in).
    fn show_items(&mut self, db: &FrameDb, items: &[(String, bool)]) {
        let Some(c) = self.tree.get("QuestItemListContainer") else { return };
        while self.item_rows < items.len() {
            let scope = format!("#i{}", self.item_rows);
            self.tree.create_scoped(db, "QuestItemListItem", c, &scope);
            let row = format!("QuestItemListItem{scope}");
            let y = -INSET - ITEM_ROW * self.item_rows as f32;
            self.tree.set_point(&row, P::TopLeft, "QuestItemListContainer", P::TopLeft, INSET, y);
            self.tree.set_point(&row, P::TopRight, "QuestItemListContainer", P::TopRight, -0.003, y);
            self.tree.set_size(&row, None, Some(ITEM_ROW));
            self.item_rows += 1;
            self.tree.bind_points();
        }
        for k in 0..self.item_rows {
            let row = format!("QuestItemListItem#i{k}");
            let title = format!("QuestItemListItemTitle#i{k}");
            self.tree.set_visible(&row, k < items.len());
            if let Some((text, done)) = items.get(k) {
                self.tree.set_text(&title, text);
                self.tree.set_enabled(&title, !done);
            }
        }
        self.tree.layout();
    }

    /// A click in the dialog: a quest chooses it; Accept closes (returns true).
    pub fn click(&mut self, x: f32, y: f32) -> bool {
        self.tree.pointer_down(x, y);
        let up = self.tree.pointer_up(x, y);
        if let Some(i) = up {
            let name = self.tree.frames[i].name.clone();
            if name == "QuestAcceptButton" {
                return true;
            }
            if let Some((h, _)) = self.item_names().into_iter().find(|(_, s)| name == format!("QuestListItemButton{s}")) {
                self.selected = Some(h);
            }
        }
        self.dirty = true;
        false
    }

    pub fn pointer_move(&mut self, x: f32, y: f32) {
        self.tree.pointer_move(x, y);
    }

    /// The dialog, and over the chosen quest its QuestListItemSelectedHighlight; a
    /// hovered completed or failed quest shows its own highlight.
    pub fn draws(&self, db: &FrameDb) -> Vec<Draw> {
        let mut out = self.tree.draw(db);
        for (h, s) in self.item_names() {
            let Some(b) = self.tree.get(&format!("QuestListItemButton{s}")) else { continue };
            let Some(r) = self.tree.frames[b].rect else { continue };
            let state = self.states.iter().find(|s| s.0 == h).map_or((false, false), |s| (s.1, s.2));
            let mut highlight = |name: &str| {
                if let Some(f) = self.tree.get(&format!("{name}{s}")) {
                    if let Some(tex) = self.tree.frames[f].def.str("HighlightAlphaFile") {
                        out.push(Draw::Image { texture: db.decorate(tex), rect: r, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Add, rotated: false });
                    }
                }
            };
            if self.selected == Some(h) {
                highlight("QuestListItemSelectedHighlight");
            }
            if self.tree.frames[b].hovered {
                if state.0 {
                    highlight("QuestListItemCompletedHighlight");
                } else if state.1 {
                    highlight("QuestListItemFailedHighlight");
                }
            }
        }
        out
    }
}
