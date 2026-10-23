//! The interface: frame definitions, frame instances, layout, and draw lists.
//!
//! Interface coordinates follow the original: the screen is 0.8 wide and 0.6 tall with
//! the origin at the bottom left.  Wider displays extend the space horizontally, keeping
//! the 0.8 x 0.6 area centered.

pub mod fdf;

use std::collections::HashMap;
use std::rc::Rc;

use fdf::{Arg, Def, Prop};
use w3_formats::profile::Profile;
use w3_mpq::Chain;

pub const UI_WIDTH: f32 = 0.8;
pub const UI_HEIGHT: f32 = 0.6;

/// All frame templates and interface strings.
#[derive(Default)]
pub struct FrameDb {
    pub templates: HashMap<String, Rc<Def>>,
    pub strings: HashMap<String, String>,
    pub skins: Profile,
    /// UI\MiscUI.txt: font heights and other interface constants.
    pub misc_ui: Profile,
    /// war3skins.txt section consulted first when decorating file names.
    pub skin: String,
    /// Data version for `_V0`/`_V1` skin keys: 0 for Reign of Chaos, 1 for The Frozen
    /// Throne.
    pub version: u32,
    /// The skins' TrueType fonts by lowercase path, for measuring text.
    pub fonts: HashMap<String, Rc<fontdue::Font>>,
}

impl FrameDb {
    /// Loads every file named in UI\FrameDef\FrameDef.toc plus their includes.
    pub fn load(chain: &Chain) -> FrameDb {
        let mut db = FrameDb { skin: "Default".into(), version: 1, ..Default::default() };
        db.skins = Profile::parse(&chain.read_string("UI\\war3skins.txt").unwrap_or_default());
        db.misc_ui = Profile::parse(&chain.read_string("UI\\MiscUI.txt").unwrap_or_default());
        // Every font the skins name, so text can be measured.
        let paths: Vec<String> = db.skins.sections.values().flat_map(|s| s.values().cloned()).collect();
        for v in paths {
            {
                if v.to_ascii_lowercase().ends_with(".ttf") && !db.fonts.contains_key(&v.to_ascii_lowercase()) {
                    if let Some(f) = chain.read(&v).ok().and_then(|b| fontdue::Font::from_bytes(b, fontdue::FontSettings::default()).ok()) {
                        db.fonts.insert(v.to_ascii_lowercase(), Rc::new(f));
                    }
                }
            }
        }
        let toc = chain.read_string("UI\\FrameDef\\FrameDef.toc").unwrap_or_default();
        let mut queue: Vec<String> = toc.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
        let mut seen = std::collections::HashSet::new();
        while let Some(path) = queue.pop() {
            if !seen.insert(path.to_ascii_lowercase()) {
                continue;
            }
            let Ok(src) = chain.read_string(&path) else {
                log::warn!("missing frame definition file {path}");
                continue;
            };
            let f = fdf::parse(&src);
            queue.extend(f.includes.iter().cloned());
            for (k, v) in f.strings {
                db.strings.insert(k, v);
            }
            for d in f.defs {
                if !d.name.is_empty() {
                    db.templates.insert(d.name.clone(), d);
                }
            }
        }
        db
    }

    /// Width of a line in interface units, without color codes (as the renderer
    /// measures it).
    pub fn text_width(&self, font: &str, size: f32, line: &str) -> f32 {
        let Some(f) = self.fonts.get(&font.to_ascii_lowercase()) else { return line.chars().count() as f32 * size * 0.5 };
        strip_color_codes(line).chars().map(|c| f.metrics(c, 64.0).advance_width).sum::<f32>() * size / 64.0
    }

    /// Breaks text at newlines and |n, then words to `max_w` (as the renderer does).
    pub fn wrap(&self, font: &str, size: f32, text: &str, max_w: f32) -> Vec<String> {
        let text = text.replace("|n", "\n").replace("|N", "\n");
        let mut lines = Vec::new();
        for para in text.split('\n') {
            let mut cur = String::new();
            for word in para.split(' ') {
                let cand = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
                if !cur.is_empty() && max_w > 0.0 && self.text_width(font, size, &cand) > max_w {
                    lines.push(std::mem::take(&mut cur));
                    cur = word.to_string();
                } else {
                    cur = cand;
                }
            }
            lines.push(cur);
        }
        lines
    }

    /// Resolves a string key from the interface string tables; unknown keys are returned
    /// unchanged.
    pub fn string(&self, key: &str) -> String {
        self.strings.get(key).cloned().unwrap_or_else(|| key.to_string())
    }

    /// Looks up a decorated file name in war3skins.txt (current skin, then Default),
    /// preferring the key for the data version (`Music_V1`).
    pub fn decorate(&self, name: &str) -> String {
        self.skin_value(name).unwrap_or_else(|| name.to_string())
    }

    pub fn skin_value(&self, name: &str) -> Option<String> {
        let versioned = format!("{name}_V{}", self.version);
        for sec in [self.skin.as_str(), "Default"] {
            if let Some(v) = self.skins.get_str(sec, &versioned).or_else(|| self.skins.get_str(sec, name)) {
                return Some(v);
            }
        }
        None
    }

    /// Flattens inheritance: the base template's properties come first, then the frame's own.
    pub fn resolve(&self, d: &Rc<Def>) -> Rc<Def> {
        let Some(base_name) = &d.inherits else { return d.clone() };
        let Some(base) = self.templates.get(base_name) else { return d.clone() };
        let base = self.resolve(base);
        let mut props: Vec<Prop> = base.props.clone();
        props.extend(d.props.iter().cloned());
        let mut children = if d.with_children { base.children.clone() } else { Vec::new() };
        children.extend(d.children.iter().cloned());
        Rc::new(Def {
            block: d.block.clone(),
            kind: if d.kind.is_empty() { base.kind.clone() } else { d.kind.clone() },
            name: d.name.clone(),
            inherits: None,
            with_children: false,
            props,
            children,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FramePoint {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl FramePoint {
    pub fn parse(s: &str) -> Option<FramePoint> {
        Some(match s {
            "TOPLEFT" => FramePoint::TopLeft,
            "TOP" => FramePoint::Top,
            "TOPRIGHT" => FramePoint::TopRight,
            "LEFT" => FramePoint::Left,
            "CENTER" => FramePoint::Center,
            "RIGHT" => FramePoint::Right,
            "BOTTOMLEFT" => FramePoint::BottomLeft,
            "BOTTOM" => FramePoint::Bottom,
            "BOTTOMRIGHT" => FramePoint::BottomRight,
            _ => return None,
        })
    }
    /// Fractions of width and height from the bottom-left corner.
    fn frac(self) -> (f32, f32) {
        match self {
            FramePoint::TopLeft => (0.0, 1.0),
            FramePoint::Top => (0.5, 1.0),
            FramePoint::TopRight => (1.0, 1.0),
            FramePoint::Left => (0.0, 0.5),
            FramePoint::Center => (0.5, 0.5),
            FramePoint::Right => (1.0, 0.5),
            FramePoint::BottomLeft => (0.0, 0.0),
            FramePoint::Bottom => (0.5, 0.0),
            FramePoint::BottomRight => (1.0, 0.0),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Point {
    pub point: FramePoint,
    /// Frame index; `None` means the parent.
    pub relative: Option<usize>,
    pub relative_point: FramePoint,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Rect {
    pub fn w(&self) -> f32 {
        self.x1 - self.x0
    }
    pub fn h(&self) -> f32 {
        self.y1 - self.y0
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }
}

#[derive(Clone, Debug)]
pub struct Frame {
    pub def: Rc<Def>,
    pub name: String,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub points: Vec<Point>,
    pub all_points: bool,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub rect: Option<Rect>,
    pub visible: bool,
    pub text: Option<String>,
    pub pushed: bool,
    pub hovered: bool,
    pub enabled: bool,
    /// The model a SPRITE frame shows (BackgroundArt, or set by the screen).
    pub model: Option<String>,
    /// Checkbox state.
    pub checked: bool,
    /// Appended to the names of frames made by one scoped instantiation, so a template
    /// can be created several times (player slots); names inside resolve within it.
    pub scope: String,
    /// Popup menu entries (MenuItem lines of its MENU, or set by the screen), the chosen
    /// one, and whether the list is open.
    pub items: Vec<String>,
    pub selected: usize,
    pub open: bool,
    /// Which part of the frame its anchors fixed (0 left or bottom, 0.5 center, 1 right
    /// or top); a text frame without a size grows from there to fit its text.
    pub anchor: (f32, f32),
    /// A text color a screen sets in place of the definition's (SetTextColor).
    pub color: Option<[f32; 4]>,
}

#[derive(Default)]
pub struct FrameTree {
    /// A popup menu whose choice changed with the last press, reported on release.
    pub popup_changed: Option<usize>,
    /// The last pointer position, for highlighting list items.
    pub pointer: (f32, f32),
    pub frames: Vec<Frame>,
    pub by_name: HashMap<String, usize>,
    /// Width of the interface space (0.8 at 4:3, wider on wide displays).
    pub width: f32,
    /// The skins' fonts (from the frame database), for sizing wrapped text.
    pub fonts: HashMap<String, Rc<fontdue::Font>>,
    /// Font skin keys (MasterFont) and the files they name.
    pub font_keys: HashMap<String, String>,
}

impl FrameTree {
    pub fn new(aspect: f32) -> FrameTree {
        let mut t = FrameTree { width: UI_HEIGHT * aspect.max(4.0 / 3.0), ..Default::default() };
        // The root covers the 4:3 area, centered.
        let root = Rc::new(Def {
            block: "Frame".into(),
            kind: "FRAME".into(),
            name: "ConsoleUI_Root".into(),
            inherits: None,
            with_children: false,
            props: vec![],
            children: vec![],
        });
        let x0 = (t.width - UI_WIDTH) / 2.0;
        t.frames.push(Frame {
            def: root,
            name: String::new(),
            parent: None,
            children: vec![],
            points: vec![],
            all_points: false,
            width: Some(UI_WIDTH),
            height: Some(UI_HEIGHT),
            rect: Some(Rect { x0, y0: 0.0, x1: x0 + UI_WIDTH, y1: UI_HEIGHT }),
            visible: true,
            text: None,
            pushed: false,
            hovered: false,
            enabled: true,
            model: None,
            checked: false,
            scope: String::new(),
            items: Vec::new(),
            selected: 0,
            open: false,
            anchor: (0.0, 0.0),
            color: None,
        });
        t
    }

    pub fn get(&self, name: &str) -> Option<usize> {
        self.by_name.get(name).copied()
    }

    /// Instantiates a template (and its children) under `parent`.
    pub fn create(&mut self, db: &FrameDb, template: &str, parent: usize) -> Option<usize> {
        self.create_scoped(db, template, parent, "")
    }

    /// Instantiates a template with `scope` appended to every frame name.
    pub fn create_scoped(&mut self, db: &FrameDb, template: &str, parent: usize, scope: &str) -> Option<usize> {
        let d = db.templates.get(template)?.clone();
        if self.fonts.is_empty() {
            self.fonts = db.fonts.clone();
            for sec in [db.skin.as_str(), "Default"] {
                if let Some(entries) = db.skins.sections.get(sec) {
                    for (k, v) in entries {
                        if v.to_ascii_lowercase().ends_with(".ttf") {
                            self.font_keys.entry(k.clone()).or_insert_with(|| v.clone());
                        }
                    }
                }
            }
        }
        Some(self.instantiate(db, &d, parent, scope))
    }

    fn instantiate(&mut self, db: &FrameDb, d: &Rc<Def>, parent: usize, scope: &str) -> usize {
        let d = db.resolve(d);
        let i = self.frames.len();
        let name = if d.name.is_empty() { String::new() } else { format!("{}{scope}", d.name) };
        self.frames.push(Frame {
            def: d.clone(),
            name: name.clone(),
            parent: Some(parent),
            children: vec![],
            points: vec![],
            all_points: false,
            width: d.num("Width"),
            height: d.num("Height"),
            rect: None,
            visible: true,
            text: d.str("Text").map(|s| db.string(s)),
            pushed: false,
            hovered: false,
            enabled: true,
            model: d.str("BackgroundArt").map(|s| if d.has("DecorateFileNames") { db.decorate(s) } else { s.to_string() }),
            checked: false,
            scope: scope.to_string(),
            items: Vec::new(),
            selected: 0,
            open: false,
            anchor: (0.0, 0.0),
            color: None,
        });
        if !name.is_empty() {
            self.by_name.insert(name, i);
        }
        self.frames[parent].children.push(i);
        for c in d.children.clone() {
            self.instantiate(db, &c, i, scope);
        }
        // A popup menu takes its entries from its MENU's MenuItem lines.
        if d.kind == "POPUPMENU" || d.kind == "GLUEPOPUPMENU" {
            let menu = d.str("PopupMenuFrame").and_then(|m| self.frames[i].children.iter().copied().find(|&c| self.frames[c].def.name == m));
            if let Some(m) = menu {
                let items: Vec<String> = self.frames[m].def.props.iter().filter(|p| p.key == "MenuItem").filter_map(|p| p.args.first().map(|a| db.string(a.as_str()))).collect();
                self.frames[i].items = items;
            }
        }
        i
    }

    /// The file a font skin key names.
    fn skin_font(&self, name: &str) -> String {
        self.font_keys.get(name).cloned().unwrap_or_else(|| name.to_string())
    }

    /// Looks up a name as written inside frame `i`'s definition: within its scope first.
    fn resolve_name(&self, i: usize, name: &str) -> Option<usize> {
        let scope = &self.frames[i].scope;
        if !scope.is_empty() {
            if let Some(&j) = self.by_name.get(&format!("{name}{scope}")) {
                return Some(j);
            }
        }
        self.by_name.get(name).copied()
    }

    /// Reads SetPoint, SetAllPoints, and Anchor properties into points.  Run after every
    /// frame of a screen is created so names resolve.
    pub fn bind_points(&mut self) {
        for i in 1..self.frames.len() {
            if !self.frames[i].points.is_empty() || self.frames[i].all_points {
                continue;
            }
            let d = self.frames[i].def.clone();
            for p in &d.props {
                match p.key.as_str() {
                    "SetAllPoints" => self.frames[i].all_points = true,
                    "SetPoint" => {
                        let a = &p.args;
                        let (Some(pt), Some(rp)) = (
                            a.first().and_then(|x| FramePoint::parse(x.as_str())),
                            a.get(2).and_then(|x| FramePoint::parse(x.as_str())),
                        ) else {
                            continue;
                        };
                        let rel = a.get(1).map(Arg::as_str).and_then(|n| self.resolve_name(i, n));
                        self.frames[i].points.push(Point {
                            point: pt,
                            relative: rel,
                            relative_point: rp,
                            x: a.get(3).map_or(0.0, Arg::num),
                            y: a.get(4).map_or(0.0, Arg::num),
                        });
                    }
                    "Anchor" => {
                        let a = &p.args;
                        let Some(pt) = a.first().and_then(|x| FramePoint::parse(x.as_str())) else { continue };
                        // An Anchor replaces the points before it: a frame's own Anchor
                        // overrides its template's (InfoPanelIconHeroIcon).
                        self.frames[i].points.clear();
                        self.frames[i].points.push(Point {
                            point: pt,
                            relative: None,
                            relative_point: pt,
                            x: a.get(1).map_or(0.0, Arg::num),
                            y: a.get(2).map_or(0.0, Arg::num),
                        });
                    }
                    _ => {}
                }
            }
        }
    }

    /// Computes every frame's rectangle.
    pub fn layout(&mut self) {
        for f in self.frames.iter_mut().skip(1) {
            f.rect = None;
        }
        // Iterate until no frame resolves further; dependencies may be in any order.
        for _ in 0..32 {
            let mut progress = false;
            for i in 1..self.frames.len() {
                if self.frames[i].rect.is_some() {
                    continue;
                }
                if let Some(r) = self.solve(i) {
                    self.frames[i].rect = Some(r);
                    self.frames[i].anchor = self.anchor_of(i);
                    progress = true;
                }
            }
            if !progress {
                break;
            }
        }
    }

    /// The fraction of the frame its anchor points fix, per axis.
    fn anchor_of(&self, i: usize) -> (f32, f32) {
        let f = &self.frames[i];
        let (mut ax, mut ay) = (None, None);
        for p in &f.points {
            let (px, py) = p.point.frac();
            ax = Some(match ax { Some(a) if a != px => 0.0, _ => px });
            ay = Some(match ay { Some(a) if a != py => 0.0, _ => py });
        }
        (ax.unwrap_or(0.0), ay.unwrap_or(1.0))
    }

    fn solve(&self, i: usize) -> Option<Rect> {
        let f = &self.frames[i];
        let parent = f.parent?;
        let pr = self.frames[parent].rect?;
        if f.all_points {
            return Some(pr);
        }
        let (mut x0, mut x1, mut y0, mut y1) = (None, None, None, None);
        let mut cx = None;
        let mut cy = None;
        for p in &f.points {
            let rr = match p.relative {
                Some(r) => self.frames[r].rect?,
                None => pr,
            };
            let (fx, fy) = p.relative_point.frac();
            let ax = rr.x0 + rr.w() * fx + p.x;
            let ay = rr.y0 + rr.h() * fy + p.y;
            let (px, py) = p.point.frac();
            match px {
                x if x == 0.0 => x0 = Some(ax),
                x if x == 1.0 => x1 = Some(ax),
                _ => cx = Some(ax),
            }
            match py {
                y if y == 0.0 => y0 = Some(ay),
                y if y == 1.0 => y1 = Some(ay),
                _ => cy = Some(ay),
            }
        }
        let w = f.width.unwrap_or(0.0);
        // A text frame without a height is as tall as its text wrapped to its width:
        // one font height per line.
        let h = f.height.unwrap_or_else(|| {
            let Some(text) = f.text.as_deref().filter(|t| !t.is_empty() && (f.def.kind == "TEXT" || f.def.block == "String")) else { return 0.0 };
            let font = f.def.prop("FrameFont").or_else(|| f.def.prop("Font"));
            let size = font.and_then(|p| p.args.get(1).map(Arg::num)).unwrap_or(0.0);
            let lines = match (font.and_then(|p| p.args.first()), f.width) {
                (Some(name), Some(width)) => {
                    let path = self.skin_font(name.as_str());
                    wrap_lines(self.fonts.get(&path.to_ascii_lowercase()).map(|f| &**f), size, text, width)
                }
                _ => text.replace("|n", "\n").replace("|N", "\n").split('\n').count(),
            };
            lines.max(1) as f32 * size
        });
        let (x0, x1) = match (x0, x1, cx) {
            (Some(a), Some(b), _) => (a, b),
            (Some(a), None, _) => (a, a + w),
            (None, Some(b), _) => (b - w, b),
            (None, None, Some(c)) => (c - w / 2.0, c + w / 2.0),
            (None, None, None) => {
                if f.points.is_empty() && f.width.is_none() {
                    (pr.x0, pr.x1)
                } else {
                    (pr.x0, pr.x0 + w)
                }
            }
        };
        let (y0, y1) = match (y0, y1, cy) {
            (Some(a), Some(b), _) => (a, b),
            (Some(a), None, _) => (a, a + h),
            (None, Some(b), _) => (b - h, b),
            (None, None, Some(c)) => (c - h / 2.0, c + h / 2.0),
            (None, None, None) => {
                if f.points.is_empty() && f.height.is_none() {
                    (pr.y0, pr.y1)
                } else {
                    (pr.y1 - h, pr.y1)
                }
            }
        };
        Some(Rect { x0, y0, x1, y1 })
    }

    /// Frames that take mouse input.
    pub fn is_control(&self, i: usize) -> bool {
        let kind = self.frames[i].def.kind.as_str();
        // A popup menu's own title and arrow belong to it.
        if let Some(p) = self.frames[i].parent {
            if self.is_popup(p) {
                return false;
            }
        }
        matches!(
            kind,
            "GLUETEXTBUTTON" | "GLUEBUTTON" | "TEXTBUTTON" | "BUTTON" | "SIMPLEBUTTON" | "GLUECHECKBOX" | "CHECKBOX" | "POPUPMENU" | "GLUEPOPUPMENU" | "EDITBOX" | "SLIDER" | "LISTBOX"
        )
    }

    pub fn is_popup(&self, i: usize) -> bool {
        matches!(self.frames[i].def.kind.as_str(), "POPUPMENU" | "GLUEPOPUPMENU")
    }

    /// The open list of a popup menu: its rectangle, item height, and border
    /// (MenuItemHeight and MenuBorder of its MENU).
    pub fn popup_list(&self, i: usize) -> Option<(Rect, f32, f32)> {
        let f = &self.frames[i];
        let r = f.rect?;
        let menu = f.def.str("PopupMenuFrame").and_then(|m| f.children.iter().copied().find(|&c| self.frames[c].def.name == m));
        let md = menu.map(|m| self.frames[m].def.clone());
        let item_h = md.as_ref().and_then(|d| d.num("MenuItemHeight")).unwrap_or(0.014);
        let border = md.as_ref().and_then(|d| d.num("MenuBorder")).unwrap_or(0.009);
        let h = f.items.len() as f32 * item_h + 2.0 * border;
        Some((Rect { x0: r.x0, x1: r.x1, y1: r.y0, y0: r.y0 - h }, item_h, border))
    }

    /// The open popup menu, if any.
    pub fn open_popup(&self) -> Option<usize> {
        (1..self.frames.len()).find(|&i| self.frames[i].open)
    }

    pub fn set_items(&mut self, name: &str, items: Vec<String>, selected: usize) {
        if let Some(i) = self.get(name) {
            self.frames[i].items = items;
            self.frames[i].selected = selected;
        }
    }

    pub fn set_selected(&mut self, name: &str, selected: usize) {
        if let Some(i) = self.get(name) {
            self.frames[i].selected = selected;
        }
    }

    /// The topmost visible, enabled control under a point.
    pub fn control_at(&self, x: f32, y: f32) -> Option<usize> {
        (1..self.frames.len())
            .rev()
            .find(|&i| self.is_control(i) && self.frames[i].enabled && self.frames[i].rect.is_some_and(|r| r.contains(x, y)) && self.is_visible(i))
    }

    /// Moves the pointer: updates which control is under it.
    pub fn pointer_move(&mut self, x: f32, y: f32) {
        self.pointer = (x, y);
        let hit = self.control_at(x, y);
        for (i, f) in self.frames.iter_mut().enumerate() {
            f.hovered = Some(i) == hit;
        }
    }

    /// Presses the pointer; returns the control pressed.
    pub fn pointer_down(&mut self, x: f32, y: f32) -> Option<usize> {
        // An open list takes the click: an item chooses it, anywhere else closes it.
        if let Some(p) = self.open_popup() {
            self.frames[p].open = false;
            if let Some((r, item_h, border)) = self.popup_list(p) {
                if r.contains(x, y) {
                    let k = ((r.y1 - border - y) / item_h).floor();
                    if k >= 0.0 && (k as usize) < self.frames[p].items.len() {
                        self.frames[p].selected = k as usize;
                        self.popup_changed = Some(p);
                    }
                    return Some(p);
                }
            }
        }
        let hit = self.control_at(x, y)?;
        self.frames[hit].pushed = true;
        Some(hit)
    }

    /// Releases the pointer; returns the control clicked (pressed and released over it).
    pub fn pointer_up(&mut self, x: f32, y: f32) -> Option<usize> {
        let hit = self.control_at(x, y);
        let pressed = self.frames.iter().position(|f| f.pushed);
        for f in &mut self.frames {
            f.pushed = false;
        }
        if let Some(p) = self.popup_changed.take() {
            return Some(p);
        }
        let i = pressed.filter(|p| Some(*p) == hit)?;
        if matches!(self.frames[i].def.kind.as_str(), "GLUECHECKBOX" | "CHECKBOX") {
            self.frames[i].checked = !self.frames[i].checked;
        }
        if self.is_popup(i) && !self.frames[i].items.is_empty() {
            self.frames[i].open = true;
            return None;
        }
        Some(i)
    }

    /// The control whose ControlShortcutKey is `key`.
    pub fn shortcut(&self, key: char) -> Option<usize> {
        (1..self.frames.len()).find(|&i| {
            self.frames[i].enabled
                && self.is_visible(i)
                && self.frames[i].def.str("ControlShortcutKey").and_then(|k| k.chars().next()).is_some_and(|k| k.eq_ignore_ascii_case(&key))
        })
    }

    pub fn set_visible(&mut self, name: &str, v: bool) {
        if let Some(i) = self.get(name) {
            self.frames[i].visible = v;
        }
    }

    pub fn set_enabled(&mut self, name: &str, v: bool) {
        if let Some(i) = self.get(name) {
            self.frames[i].enabled = v;
        }
    }

    pub fn set_text(&mut self, name: &str, text: &str) {
        if let Some(i) = self.get(name) {
            self.frames[i].text = Some(text.to_string());
        }
    }

    /// Replaces a frame's anchors with one point (FUN_6f111600 after clearing).
    pub fn set_point(&mut self, name: &str, point: FramePoint, relative: &str, relative_point: FramePoint, x: f32, y: f32) {
        let rel = self.get(relative);
        if let Some(i) = self.get(name) {
            self.frames[i].all_points = false;
            self.frames[i].points.retain(|p| p.point != point);
            self.frames[i].points.push(Point { point, relative: rel, relative_point, x, y });
        }
    }

    pub fn clear_points(&mut self, name: &str) {
        if let Some(i) = self.get(name) {
            self.frames[i].points.clear();
            self.frames[i].all_points = false;
        }
    }

    pub fn set_size(&mut self, name: &str, w: Option<f32>, h: Option<f32>) {
        if let Some(i) = self.get(name) {
            if w.is_some() {
                self.frames[i].width = w;
            }
            if h.is_some() {
                self.frames[i].height = h;
            }
        }
    }

    pub fn set_model(&mut self, name: &str, model: &str) {
        if let Some(i) = self.get(name) {
            self.frames[i].model = Some(model.to_string());
        }
    }

    /// Visible SPRITE frames with models, in drawing order, with the point their model
    /// origin sits on (the frame's lower left; model units are interface units).
    pub fn sprites(&self) -> Vec<(usize, String, f32, f32)> {
        (1..self.frames.len())
            .filter(|&i| self.frames[i].def.kind == "SPRITE" && self.is_visible(i))
            .filter_map(|i| {
                let f = &self.frames[i];
                let r = f.rect?;
                Some((i, f.model.clone()?, r.x0, r.y0))
            })
            .collect()
    }

    pub fn is_visible(&self, mut i: usize) -> bool {
        loop {
            if !self.frames[i].visible {
                return false;
            }
            match self.frames[i].parent {
                Some(p) => i = p,
                None => return true,
            }
        }
    }
}

/// A draw command in interface coordinates.
#[derive(Clone, Debug)]
pub enum Draw {
    Image {
        texture: String,
        rect: Rect,
        /// Left, top, right, bottom texture coordinates.
        uv: [f32; 4],
        color: [f32; 4],
        blend: Blend,
        /// The image is stored rotated a quarter turn: screen x follows texture v.
        rotated: bool,
    },
    Text {
        font: String,
        size: f32,
        rect: Rect,
        text: String,
        color: [f32; 4],
        justify_h: Justify,
        justify_v: Justify,
        /// Shadow color and offset.
        shadow: Option<([f32; 4], f32, f32)>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blend {
    Alpha,
    AlphaKey,
    Add,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Justify {
    Start,
    Middle,
    End,
}

fn texcoord(d: &Def) -> [f32; 4] {
    let tc = d.prop("TexCoord").map(|p| p.args.iter().map(Arg::num).collect::<Vec<_>>());
    match tc.as_deref() {
        Some([l, r, t, b]) => [*l, *t, *r, *b],
        _ => [0.0, 0.0, 1.0, 1.0],
    }
}

fn color_prop(p: Option<&Prop>, default: [f32; 4]) -> [f32; 4] {
    match p {
        Some(p) if p.args.len() >= 3 => {
            let a: Vec<f32> = p.args.iter().map(Arg::num).collect();
            [a[0], a[1], a[2], a.get(3).copied().unwrap_or(1.0)]
        }
        _ => default,
    }
}

impl FrameTree {
    /// Builds draw commands for visible frames in tree order.
    pub fn draw(&self, db: &FrameDb) -> Vec<Draw> {
        let mut out = Vec::new();
        self.draw_frame(db, 0, &mut out);
        // An open popup list covers everything else.
        if let Some(p) = self.open_popup() {
            self.draw_popup_list(db, p, &mut out);
        }
        out
    }

    /// The parts of a popup menu: its MENU frame, title text frame, and arrow frame.
    fn popup_parts(&self, i: usize) -> (Option<usize>, Option<usize>, Option<usize>) {
        let f = &self.frames[i];
        let child = |key: &str| f.def.str(key).and_then(|n| f.children.iter().copied().find(|&c| self.frames[c].def.name == n));
        (child("PopupMenuFrame"), child("PopupTitleFrame"), child("PopupArrowFrame"))
    }

    fn popup_font(&self, db: &FrameDb, d: &Def) -> (String, f32) {
        match d.prop("FrameFont") {
            Some(p) => (db.decorate(p.args.first().map(Arg::as_str).unwrap_or("MasterFont")), p.args.get(1).map_or(0.011, Arg::num)),
            None => (db.decorate("MasterFont"), 0.011),
        }
    }

    /// A closed popup menu: its backdrop, the chosen entry as its title, and its arrow
    /// at the right inside PopupButtonInset.
    fn draw_popup(&self, db: &FrameDb, i: usize, out: &mut Vec<Draw>) {
        let f = &self.frames[i];
        let Some(r) = f.rect else { return };
        let d = f.def.clone();
        let file = |n: &str| if d.has("DecorateFileNames") { db.decorate(n) } else { n.to_string() };
        let key = if !f.enabled { "ControlDisabledBackdrop" } else if f.pushed || f.open { "ControlPushedBackdrop" } else { "ControlBackdrop" };
        let name = d.str(key).or_else(|| d.str("ControlBackdrop"));
        if let Some(bd) = name.and_then(|n| f.children.iter().map(|&c| &self.frames[c]).find(|c| c.def.name == n)) {
            self.draw_backdrop(db, &bd.def, r, &file, out);
        }
        let (_, title, arrow) = self.popup_parts(i);
        let text = f.items.get(f.selected).cloned().or_else(|| f.text.clone()).unwrap_or_default();
        if let Some(t) = title {
            let td = self.frames[t].def.clone();
            let (font, size) = self.popup_font(db, &td);
            // The title's text frame offsets it by FontJustificationOffset.
            let text_def = td.str("ButtonText").and_then(|n| self.frames[t].children.iter().map(|&c| &self.frames[c]).find(|c| c.def.name == n)).map(|c| c.def.clone());
            let off = text_def.as_ref().and_then(|d| d.prop("FontJustificationOffset")).and_then(|p| p.args.first().map(Arg::num)).unwrap_or(0.01);
            let color = if f.enabled { [1.0; 4] } else { [0.5, 0.5, 0.5, 1.0] };
            out.push(Draw::Text { font, size, rect: Rect { x0: r.x0 + off, ..r }, text, color, justify_h: Justify::Start, justify_v: Justify::Middle, shadow: None });
        }
        if let Some(a) = arrow {
            let ad = self.frames[a].def.clone();
            let inset = d.num("PopupButtonInset").unwrap_or(0.01);
            let (w, h) = (ad.num("Width").unwrap_or(0.011), ad.num("Height").unwrap_or(0.011));
            let cy = (r.y0 + r.y1) / 2.0;
            let ar = Rect { x0: r.x1 - inset - w, x1: r.x1 - inset, y0: cy - h / 2.0, y1: cy + h / 2.0 };
            if let Some(bd) = ad.str("ControlBackdrop").and_then(|n| self.frames[a].children.iter().map(|&c| &self.frames[c]).find(|c| c.def.name == n)) {
                let afile = |n: &str| if bd.def.has("DecorateFileNames") { db.decorate(n) } else { n.to_string() };
                self.draw_backdrop(db, &bd.def, ar, &afile, out);
            }
        }
    }

    fn draw_popup_list(&self, db: &FrameDb, p: usize, out: &mut Vec<Draw>) {
        let Some((r, item_h, border)) = self.popup_list(p) else { return };
        let (menu, _, _) = self.popup_parts(p);
        let Some(m) = menu else { return };
        let md = self.frames[m].def.clone();
        let file = |n: &str| if md.has("DecorateFileNames") { db.decorate(n) } else { n.to_string() };
        if let Some(bd) = md.str("ControlBackdrop").and_then(|n| self.frames[m].children.iter().map(|&c| &self.frames[c]).find(|c| c.def.name == n)) {
            self.draw_backdrop(db, &bd.def, r, &file, out);
        }
        let (font, size) = self.popup_font(db, &md);
        let highlight = color_prop(md.prop("MenuTextHighlightColor"), [1.0, 0.8, 0.0, 1.0]);
        for (k, item) in self.frames[p].items.iter().enumerate() {
            let y1 = r.y1 - border - k as f32 * item_h;
            let row = Rect { x0: r.x0 + border, x1: r.x1 - border, y0: y1 - item_h, y1 };
            let color = if self.pointer_in(row) { highlight } else { [1.0; 4] };
            out.push(Draw::Text { font: font.clone(), size, rect: row, text: item.clone(), color, justify_h: Justify::Start, justify_v: Justify::Middle, shadow: None });
        }
    }

    fn pointer_in(&self, r: Rect) -> bool {
        r.contains(self.pointer.0, self.pointer.1)
    }

    fn draw_frame(&self, db: &FrameDb, i: usize, out: &mut Vec<Draw>) {
        let f = &self.frames[i];
        if !f.visible {
            return;
        }
        if let Some(r) = f.rect {
            self.draw_self(db, f, r, out);
        }
        if self.is_popup(i) {
            self.draw_popup(db, i, out);
            return;
        }
        for &c in &f.children {
            // Control state backdrops are drawn by their owning control.
            if self.is_state_backdrop(i, c) {
                continue;
            }
            self.draw_frame(db, c, out);
        }
    }

    fn is_state_backdrop(&self, owner: usize, child: usize) -> bool {
        let name = &self.frames[child].def.name;
        let d = &self.frames[owner].def;
        ["ControlBackdrop", "ControlPushedBackdrop", "ControlDisabledBackdrop", "ControlDisabledPushedBackdrop", "ControlMouseOverHighlight", "ControlFocusHighlight"]
            .iter()
            .any(|k| d.str(k).is_some_and(|n| n == name))
    }

    fn draw_self(&self, db: &FrameDb, f: &Frame, r: Rect, out: &mut Vec<Draw>) {
        let d = &f.def;
        let decorate = d.has("DecorateFileNames");
        let file = |name: &str| if decorate { db.decorate(name) } else { name.to_string() };
        match (d.block.as_str(), d.kind.as_str()) {
            ("Texture", _) => {
                // A screen may replace the file (set_model), as the info panel's icons do.
                let Some(name) = f.model.as_deref().or(d.str("File")) else { return };
                let uv = texcoord(d);
                let blend = match d.str("AlphaMode") {
                    Some("ALPHAKEY") => Blend::AlphaKey,
                    Some("ADD") => Blend::Add,
                    _ => Blend::Alpha,
                };
                let parent_decorates = f.parent.is_some_and(|p| self.frames[p].def.has("DecorateFileNames"));
                let path = if decorate || parent_decorates { db.decorate(name) } else { name.to_string() };
                out.push(Draw::Image { texture: path, rect: r, uv, color: [1.0; 4], blend, rotated: false });
            }
            (_, "BACKDROP") => self.draw_backdrop(db, d, r, &file, out),
            (_, "SIMPLEBUTTON") => {
                let key = if !f.enabled { "DisabledTexture" } else if f.pushed { "PushedTexture" } else { "NormalTexture" };
                for k in [key, if f.hovered && f.enabled { "UseHighlight" } else { "" }] {
                    let Some(t) = d.str(k).and_then(|n| db.templates.get(n)) else { continue };
                    let t = db.resolve(t);
                    let Some(name) = t.str("File") else { continue };
                    let uv = texcoord(&t);
                    let blend = if t.str("AlphaMode") == Some("ADD") { Blend::Add } else { Blend::Alpha };
                    out.push(Draw::Image { texture: db.decorate(name), rect: r, uv, color: [1.0; 4], blend, rotated: false });
                }
                let tkey = if !f.enabled { "DisabledText" } else if f.hovered { "HighlightText" } else { "NormalText" };
                if let Some(p) = d.prop(tkey) {
                    let tmpl = p.args.first().map(Arg::as_str).and_then(|n| db.templates.get(n)).map(|t| db.resolve(t));
                    let text = f.text.clone().or_else(|| p.args.get(1).map(|a| db.string(a.as_str()))).unwrap_or_default();
                    let (font, size) = tmpl.as_ref().and_then(|t| t.prop("Font")).map_or((db.decorate("MasterFont"), 0.01), |p| {
                        (db.decorate(p.args.first().map(Arg::as_str).unwrap_or("MasterFont")), p.args.get(1).map_or(0.01, Arg::num))
                    });
                    let color = color_prop(tmpl.as_ref().and_then(|t| t.prop("FontColor")), [1.0; 4]);
                    out.push(Draw::Text { font, size, rect: r, text, color, justify_h: Justify::Middle, justify_v: Justify::Middle, shadow: None });
                }
            }
            (_, "TEXT") | ("String", _) => {
                let Some(text) = &f.text else { return };
                let font = d.prop("FrameFont").or_else(|| d.prop("Font"));
                let (font, size) = match font {
                    Some(p) => (
                        db.decorate(p.args.first().map(Arg::as_str).unwrap_or("MasterFont")),
                        p.args.get(1).map_or(0.012, Arg::num),
                    ),
                    None => (db.decorate("MasterFont"), 0.012),
                };
                let justify = |k: &str, def: Justify| match d.str(k) {
                    Some("JUSTIFYLEFT") | Some("JUSTIFYTOP") => Justify::Start,
                    Some("JUSTIFYRIGHT") | Some("JUSTIFYBOTTOM") => Justify::End,
                    Some(_) => Justify::Middle,
                    None => def,
                };
                // Text owned by a button takes the button's state color.
                let owner = f.parent.map(|p| &self.frames[p]).filter(|p| p.def.str("ButtonText") == Some(f.def.name.as_str()));
                let color_key = match owner {
                    Some(o) if !o.enabled => "FontDisabledColor",
                    Some(_) => "FontHighlightColor",
                    None => "FontColor",
                };
                // CTextFrame starts white (0xffffffff) until FontColor sets it.
                let color = f.color.unwrap_or_else(|| color_prop(d.prop(color_key).or_else(|| d.prop("FontColor")), [1.0; 4]));
                // Without a size, the text sizes itself around the anchored point.
                let (mut jh, mut jv) = (justify("FontJustificationH", Justify::Middle), justify("FontJustificationV", Justify::Middle));
                let mut r = r;
                if r.w() < 1e-6 {
                    let (ax, x) = (f.anchor.0, r.x0);
                    r.x0 = x - 2.0 * ax;
                    r.x1 = x + 2.0 * (1.0 - ax);
                    jh = if ax < 0.25 { Justify::Start } else if ax > 0.75 { Justify::End } else { Justify::Middle };
                }
                if r.h() < 1e-6 {
                    let (ay, y) = (f.anchor.1, r.y0);
                    r.y0 = y - 2.0 * ay;
                    r.y1 = y + 2.0 * (1.0 - ay);
                    jv = if ay > 0.75 { Justify::Start } else if ay < 0.25 { Justify::End } else { Justify::Middle };
                }
                // A pressed button moves its text by ButtonPushedTextOffset.
                let r = match owner.filter(|o| o.pushed).and_then(|o| o.def.prop("ButtonPushedTextOffset")) {
                    Some(p) => {
                        let (dx, dy) = (p.args.first().map_or(0.0, Arg::num), p.args.get(1).map_or(0.0, Arg::num));
                        Rect { x0: r.x0 + dx, x1: r.x1 + dx, y0: r.y0 + dy, y1: r.y1 + dy }
                    }
                    None => r,
                };
                let shadow = d.prop("FontShadowOffset").map(|p| {
                    let c = color_prop(d.prop("FontShadowColor"), [0.0, 0.0, 0.0, 1.0]);
                    (c, p.args.first().map_or(0.0, Arg::num), p.args.get(1).map_or(0.0, Arg::num))
                });
                out.push(Draw::Text {
                    font,
                    size,
                    rect: r,
                    text: text.clone(),
                    color,
                    justify_h: jh,
                    justify_v: jv,
                    shadow,
                });
            }
            (_, "GLUETEXTBUTTON") | (_, "GLUEBUTTON") | (_, "TEXTBUTTON") | (_, "BUTTON") | (_, "GLUECHECKBOX") | (_, "CONTROL") => {
                let key = if !f.enabled {
                    "ControlDisabledBackdrop"
                } else if f.pushed {
                    "ControlPushedBackdrop"
                } else {
                    "ControlBackdrop"
                };
                let name = d.str(key).or_else(|| d.str("ControlBackdrop"));
                if let Some(bd) = name.and_then(|n| f.children.iter().map(|&c| &self.frames[c]).find(|c| c.def.name == n)) {
                    self.draw_backdrop(db, &bd.def, r, &file, out);
                }
                if f.hovered && f.enabled {
                    if let Some(h) = d.str("ControlMouseOverHighlight").and_then(|n| f.children.iter().map(|&c| &self.frames[c]).find(|c| c.def.name == n)) {
                        if let Some(tex) = h.def.str("HighlightAlphaFile") {
                            out.push(Draw::Image { texture: tex.to_string(), rect: r, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: Blend::Add, rotated: false });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Draws a backdrop definition built by a screen (CToolTipWar3's box) over `r`.
    pub fn backdrop(&self, db: &FrameDb, d: &Def, r: Rect, out: &mut Vec<Draw>) {
        let file = |n: &str| db.decorate(n);
        self.draw_backdrop(db, d, r, &file, out);
    }

    fn draw_backdrop(&self, db: &FrameDb, d: &Def, r: Rect, owner_file: &dyn Fn(&str) -> String, out: &mut Vec<Draw>) {
        let color = [1.0; 4];
        // A backdrop decorates its file names when it or its owner says so.
        let own = |n: &str| db.decorate(n);
        let file: &dyn Fn(&str) -> String = if d.has("DecorateFileNames") { &own } else { owner_file };
        let insets: Vec<f32> = d.prop("BackdropBackgroundInsets").map(|p| p.args.iter().map(Arg::num).collect()).unwrap_or_default();
        let ins = |i: usize| insets.get(i).copied().unwrap_or(0.0);
        if let Some(bg) = d.str("BackdropBackground") {
            let inner = Rect { x0: r.x0 + ins(0), x1: r.x1 - ins(1), y1: r.y1 - ins(2), y0: r.y0 + ins(3) };
            let mut uv = [0.0, 0.0, 1.0, 1.0];
            if d.has("BackdropTileBackground") {
                let s = d.num("BackdropBackgroundSize").unwrap_or(inner.w()).max(1e-4);
                uv = [0.0, 0.0, inner.w() / s, inner.h() / s];
            }
            if d.has("BackdropMirrored") {
                uv = [uv[2], uv[1], uv[0], uv[3]];
            }
            out.push(Draw::Image { texture: file(bg), rect: inner, uv, color, blend: Blend::Alpha, rotated: false });
        }
        let Some(edge) = d.str("BackdropEdgeFile") else { return };
        let edge = file(edge);
        let c = d.num("BackdropCornerSize").unwrap_or(0.0);
        if c <= 0.0 {
            return;
        }
        let flags = d.str("BackdropCornerFlags").unwrap_or("UL|UR|BL|BR|T|L|B|R");
        let has = |f: &str| flags.split('|').any(|x| x == f);
        // The edge texture holds eight square pieces side by side: left, right, top,
        // bottom, upper-left, upper-right, lower-left, lower-right.
        let piece = |i: f32| [i / 8.0, 0.0, (i + 1.0) / 8.0, 1.0];
        let mut push = |rect: Rect, uv: [f32; 4], rotated: bool| out.push(Draw::Image { texture: edge.clone(), rect, uv, color, blend: Blend::Alpha, rotated });
        let (vx, vy) = ((r.h() - 2.0 * c) / c, (r.w() - 2.0 * c) / c);
        if has("L") {
            let u = piece(0.0);
            push(Rect { x0: r.x0, x1: r.x0 + c, y0: r.y0 + c, y1: r.y1 - c }, [u[0], 0.0, u[2], vx], false);
        }
        if has("R") {
            let u = piece(1.0);
            push(Rect { x0: r.x1 - c, x1: r.x1, y0: r.y0 + c, y1: r.y1 - c }, [u[0], 0.0, u[2], vx], false);
        }
        // Top and bottom pieces are stored rotated a quarter turn, their outer side at the
        // piece's left for the top edge and its right for the bottom edge.
        if has("T") {
            let u = piece(2.0);
            push(Rect { x0: r.x0 + c, x1: r.x1 - c, y0: r.y1 - c, y1: r.y1 }, [u[2], 0.0, u[0], vy], true);
        }
        if has("B") {
            let u = piece(3.0);
            push(Rect { x0: r.x0 + c, x1: r.x1 - c, y0: r.y0, y1: r.y0 + c }, [u[2], 0.0, u[0], vy], true);
        }
        if has("UL") {
            push(Rect { x0: r.x0, x1: r.x0 + c, y0: r.y1 - c, y1: r.y1 }, piece(4.0), false);
        }
        if has("UR") {
            push(Rect { x0: r.x1 - c, x1: r.x1, y0: r.y1 - c, y1: r.y1 }, piece(5.0), false);
        }
        if has("BL") {
            push(Rect { x0: r.x0, x1: r.x0 + c, y0: r.y0, y1: r.y0 + c }, piece(6.0), false);
        }
        if has("BR") {
            push(Rect { x0: r.x1 - c, x1: r.x1, y0: r.y0, y1: r.y0 + c }, piece(7.0), false);
        }
    }
}

/// How many lines text takes when broken at newlines and |n and then wrapped to `max_w`
/// (as the renderer and FrameDb::wrap break it).
fn wrap_lines(font: Option<&fontdue::Font>, size: f32, text: &str, max_w: f32) -> usize {
    let width = |s: &str| match font {
        Some(f) => strip_color_codes(s).chars().map(|c| f.metrics(c, 64.0).advance_width).sum::<f32>() * size / 64.0,
        None => s.chars().count() as f32 * size * 0.5,
    };
    let text = text.replace("|n", "\n").replace("|N", "\n");
    let mut n = 0;
    for para in text.split('\n') {
        let mut cur = String::new();
        n += 1;
        for word in para.split(' ') {
            let cand = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
            if !cur.is_empty() && width(&cand) > max_w {
                n += 1;
                cur = word.to_string();
            } else {
                cur = cand;
            }
        }
    }
    n
}

/// Text without |cAARRGGBB and |r color codes.
pub fn strip_color_codes(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '|' {
            match it.peek() {
                Some('c') | Some('C') => {
                    it.next();
                    for _ in 0..8 {
                        it.next();
                    }
                    continue;
                }
                Some('r') | Some('R') => {
                    it.next();
                    continue;
                }
                _ => {}
            }
        }
        out.push(c);
    }
    out
}
