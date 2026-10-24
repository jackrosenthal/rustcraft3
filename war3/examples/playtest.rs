//! Scripted play test: loads a map, runs commands, steps the simulation, and writes a PNG.
//!
//! `playtest <datadir> <map> <out.png> <width> <height> <commands...>`
//! Commands: `select x0 y0 x1 y1` (screen box), `click x y` (right click), `wait secs`,
//! `camera x y`.

use std::path::Path;

use war3::data::GameData;
use war3::scene::{Assets, Scene};

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let a: Vec<String> = std::env::args().collect();
    let (w, h): (u32, u32) = if a.len() > 5 { (a[4].parse()?, a[5].parse()?) } else { (1920, 1080) };
    let data = GameData::open(Path::new(&a[1]), false)?;
    // W3CAMPAIGN: a custom campaign (.w3n) whose maps and data the map is played with.
    if let Ok(c) = std::env::var("W3CAMPAIGN") {
        data.chain.mount(std::sync::Arc::new(w3_mpq::Archive::open(&c)?), None, war3::data::PRIORITY_CAMPAIGN);
    }
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    desc.backends = wgpu::Backends::PRIMARY.with_env();
    let instance = wgpu::Instance::new(desc);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut assets = Assets::new(&device, &queue, &data);
    // A saved game (.w3z) starts its map with the saved state.
    let (map, restore) = if a[2].to_ascii_lowercase().ends_with(".w3z") {
        let save = w3_game::save::SaveGame::parse(&std::fs::read(&a[2])?)?;
        (save.map.clone(), Some(save))
    } else {
        (a[2].clone(), None)
    };
    // W3LOBBY="L1,C3" fills the slots in order: L the local player, C a computer, E
    // empty, O the local player observing, each followed by its race (1 Human, 2 Orc, 3
    // Undead, 4 Night Elf, none random).
    let lobby: Option<Vec<w3_game::LobbySlot>> = std::env::var("W3LOBBY").ok().map(|v| {
        v.split(',')
            .enumerate()
            .map(|(i, s)| {
                let fill = match s.chars().next() {
                    Some('L') => w3_game::SlotFill::Local,
                    Some('C') => w3_game::SlotFill::Computer,
                    Some('O') => w3_game::SlotFill::Observer(true),
                    _ => w3_game::SlotFill::Empty,
                };
                // An optional "t<team>" after the race puts the slot on that team.
                let (race, team) = match s[1..].split_once('t') {
                    Some((r, t)) => (r.parse().ok(), t.parse().unwrap_or(i as i32)),
                    None => (s[1..].parse().ok(), i as i32),
                };
                let team = if s.starts_with('O') { 12 } else { team };
                w3_game::LobbySlot { player: i as u8, fill, name: None, race, team, color: i as i32, handicap: 100.0, difficulty: 1 }
            })
            .collect()
    });
    // W3REPLAY: a replay to watch on the map instead of playing.
    let replay = match std::env::var("W3REPLAY") {
        Ok(f) => Some(w3_game::replay::Replay::parse(&std::fs::read(f)?)?),
        Err(_) => None,
    };
    let mut scene = Scene::load(&device, &queue, format, &data, &mut assets, Path::new(&map), w as f32 / h as f32, lobby.as_deref(), restore, replay)?;
    scene.sync_units(&device, &queue, &mut assets);
    scene.sync_destructables(&device, &queue, &mut assets);
    scene.sync_items(&device, &queue, &mut assets);
    // W3SAVES: where the game menu keeps saved games, for testing.
    if let Ok(d) = std::env::var("W3SAVES") {
        scene.saves_dir = d.clone().into();
        if let Some(g) = scene.game.as_mut() {
            g.world.saves_dir = Some(d.into());
        }
    }
    let mut i = 6;
    let num = |s: &str| s.parse::<f32>().unwrap();
    while i < a.len() {
        scene.update_menu(&assets.frames, w as f32 / h as f32);
        scene.sync_hud(&assets.frames, w as f32 / h as f32);
        match a[i].as_str() {
            "select" => {
                scene.select((num(&a[i + 1]), num(&a[i + 2])), (num(&a[i + 3]), num(&a[i + 4])), (w, h), false);
                eprintln!("selected {:?}", scene.selection);
                i += 5;
            }
            // doodads <text>: doodad instances whose model path contains the text.
            "doodads" => {
                for inst in &scene.instances {
                    if inst.model.path.to_ascii_lowercase().contains(&a[i + 1].to_ascii_lowercase()) {
                        eprintln!("doodad {} at {:?} seq {:?} visible {}", inst.model.path, inst.transform.w_axis, inst.sequence, inst.visible);
                    }
                }
                i += 2;
            }
            "camdump" => {
                if let Some(g) = scene.game.as_ref() {
                    let c = &g.world.camera;
                    eprintln!("camera target {} {} fields {:?}", c.x, c.y, c.fields);
                }
                i += 1;
            }
            // hover <x> <y>: moves the pointer (screen pixels).
            "hover" => {
                scene.hover(num(&a[i + 1]), num(&a[i + 2]), (w, h));
                i += 3;
            }
            // ended: whether the game has ended (EndGame), and with a score screen.
            "ended" => {
                eprintln!("ended {} score {}", scene.ended, scene.final_score.is_some());
                i += 1;
            }
            "lclick" => {
                let consumed = scene.left_click(num(&a[i + 1]), num(&a[i + 2]), (w, h));
                eprintln!("left click consumed: {consumed}");
                i += 3;
            }
            // chatdialog: as in a network game, opens the chat dialog, to look at.
            "chatdialog" => {
                if let Some(g) = scene.game.as_mut() {
                    g.world.network = true;
                }
                scene.open_log();
                i += 1;
            }
            // starts: each player's start location.
            "starts" => {
                if let Some(g) = &scene.game {
                    for p in 0..12 {
                        let pl = &g.world.players[p];
                        if pl.slot_state == 1 {
                            eprintln!("start p{p} team {} at {}", pl.team, pl.start_location);
                        }
                    }
                }
                i += 1;
            }
            // allies: opens the Allies dialog, to look at.
            "allies" => {
                let aspect = w as f32 / h as f32;
                if let (Some(hud), Some(g)) = (scene.hud.as_mut(), scene.game.as_ref()) {
                    hud.alliance = war3::alliance::AllianceDialog::new(&assets.frames, &g.world, aspect);
                }
                i += 1;
            }
            // alliances: each player's alliance flags toward the others, and allied victory.
            "alliances" => {
                if let Some(g) = scene.game.as_ref() {
                    for p in [0usize, 1, 2, 3, 12, 15] {
                        let pl = &g.world.players[p];
                        let flags: Vec<String> = [0usize, 1, 2, 3, 12, 15].into_iter().map(|q| pl.alliances[q].iter().map(|&b| if b { '1' } else { '0' }).collect()).collect();
                        eprintln!("p{p} team {} allied_victory {} toward 0,1,2,3,12,15 {:?}", pl.team, pl.get_state(w3_game::types::player_state::ALLIED_VICTORY), flags);
                    }
                }
                i += 1;
            }
            // netdialog: shows the host's waiting dialog for player 2, to look at.
            "netdialog" => {
                let aspect = w as f32 / h as f32;
                if let Some(hud) = scene.hud.as_mut() {
                    hud.net_dialog = war3::netdialog::NetDialog::suspend(&assets.frames, aspect, &[(1, "Player 2".into(), 1), (2, "Player 3".into(), 2)]);
                }
                i += 1;
            }
            // replaysave <file>: the replay so far.
            "replaysave" => {
                if let Some(r) = scene.game.as_ref().and_then(|g| g.recorder.as_ref()) {
                    std::fs::write(&a[i + 1], r.replay.to_bytes()?)?;
                }
                i += 2;
            }
            // zoom <steps>: Page Up (positive) or Page Down (negative) that many times.
            "zoom" => {
                let n = num(&a[i + 1]) as i32;
                for _ in 0..n.abs() {
                    scene.zoom_step(n > 0);
                }
                i += 2;
            }
            // rotate <0|1|2>: holds Insert (1) or Delete (2), or lets go (0).
            "rotate" => {
                match num(&a[i + 1]) as usize {
                    0 => {
                        scene.rotate_key(1, false);
                        scene.rotate_key(2, false);
                    }
                    s => scene.rotate_key(s, true),
                }
                i += 2;
            }
            "key" => {
                scene.key(a[i + 1].chars().next().unwrap());
                i += 2;
            }
            "click" => {
                scene.command(num(&a[i + 1]), num(&a[i + 2]), (w, h));
                i += 3;
            }
            "camera" => {
                if let Some(g) = scene.game.as_mut() {
                    g.world.camera.pan_to(num(&a[i + 1]), num(&a[i + 2]), 0.0);
                }
                scene.sync_camera();
                i += 3;
            }
            // camstart <dx> <dy>: centers the camera near player 0's start.
            "camstart" => {
                if let Some(g) = scene.game.as_ref() {
                    let (sx, sy) = g.world.start_of(0);
                    let (dx, dy) = (num(&a[i + 1]), num(&a[i + 2]));
                    if let Some(g) = scene.game.as_mut() {
                        g.world.camera.pan_to(sx + dx, sy + dy, 0.0);
                    }
                    scene.sync_camera();
                }
                i += 3;
            }
            "wait" => {
                let mut t = num(&a[i + 1]);
                while t > 0.0 {
                    scene.update(0.02);
                    scene.sync_units(&device, &queue, &mut assets);
                    scene.sync_destructables(&device, &queue, &mut assets);
                    scene.sync_items(&device, &queue, &mut assets);
                    t -= 0.02;
                }
                i += 2;
            }
            "card" => {
                for b in scene.card_now(&assets.frames) {
                    eprintln!("card {:?} ({},{}) enabled {} {} {}", b.action, b.x, b.y, b.enabled, b.tip, b.requires.replace('\n', " "));
                }
                i += 1;
            }
            "all" => {
                if let Some(g) = &scene.game {
                    let w = &g.world;
                    for &h in &w.unit_order {
                        if let Some(u) = w.unit(h).filter(|u| u.owner == w.local_player) {
                            let v = scene.units.get(&h).map(|v| (v.inst.sequence, v.inst.frame as i32, v.inst.visible, v.inst.tint));
                            eprintln!("{h:#x} {} at ({:.0},{:.0}) {:?} anim {} hidden {} path {:?} next {:?} n {} facing {:.0} view {:?}", String::from_utf8_lossy(&u.type_id), u.x, u.y, u.action, u.anim, u.hidden, u.path.last(), u.path.first(), u.path.len(), u.facing, v);
                        }
                    }
                }
                i += 1;
            }
            "loadreq" => {
                eprintln!("load requested: {:?}", scene.load_requested);
                i += 1;
            }
            "save" => {
                std::fs::write(&a[i + 1], scene.save_game()?)?;
                i += 2;
            }
            "res" => {
                if let Some(g) = &scene.game {
                    let p = &g.world.players[g.world.local_player as usize];
                    let st = |k: i32| p.state.get(&k).copied().unwrap_or(0);
                    eprintln!("t {:.1} gold {} lumber {} food {}/{} tokens {}", g.world.time, st(w3_game::types::player_state::RESOURCE_GOLD), st(w3_game::types::player_state::RESOURCE_LUMBER), st(w3_game::types::player_state::RESOURCE_FOOD_USED), st(w3_game::types::player_state::RESOURCE_FOOD_CAP), st(w3_game::types::player_state::RESOURCE_HERO_TOKENS));
                }
                i += 1;
            }
            "dump" => {
                if let Some(g) = &scene.game {
                    for &h in &scene.selection {
                        if let Some(u) = g.world.unit(h) {
                            eprintln!("{h:#x} at ({:.0},{:.0}) {:?} gold {} lumber {} anim {} path {:?}", u.x, u.y, u.action, u.carry_gold, u.carry_lumber, u.anim, u.path.len());
                        }
                    }
                }
                i += 1;
            }
            "pick" => {
                let (x, y) = (num(&a[i + 1]), num(&a[i + 2]));
                if let Some(g) = &scene.game {
                    let wd = &g.world;
                    eprintln!("{} destructables", wd.destructable_order.len());
                    if let Some((gx, gy)) = scene.ground_at(x, y, (w, h)) {
                        let mut best = (f32::MAX, 0u32);
                        for &d in &wd.destructable_order {
                            if let Some(w3_game::types::Obj::Destructable(dd)) = wd.handles.get(d) {
                                let dist = (dd.x - gx).hypot(dd.y - gy);
                                if dist < best.0 { best = (dist, d); }
                            }
                        }
                        if let Some(w3_game::types::Obj::Destructable(dd)) = wd.handles.get(best.1) {
                            let o = wd.objects.destructables.get(dd.type_id).unwrap();
                            eprintln!("nearest dest {:.0} away at ({:.0},{:.0},{:.0}) life {} vis {} sel {} occH {}", best.0, dd.x, dd.y, dd.z, dd.life, dd.visible, o.str("selSize"), o.str("occH"));
                        }
                    }
                }
                eprintln!("unit {:?} dest {:?} ground {:?}", scene.pick_unit(x, y, (w, h)), scene.pick_destructable(x, y, (w, h)), scene.ground_at(x, y, (w, h)));
                i += 3;
            }
            "players" => {
                if let Some(g) = &scene.game {
                    let w = &g.world;
                    for p in 0..16u8 {
                        let n = w.unit_order.iter().filter(|&&h| w.unit(h).is_some_and(|u| u.owner == p && !u.dead)).count();
                        if p >= 2 && n == 0 {
                            continue;
                        }
                        let mut types: std::collections::BTreeMap<String, i32> = Default::default();
                        for &h in &w.unit_order {
                            if let Some(u) = w.unit(h) { if u.owner == p && !u.dead { *types.entry(w3_game::objects::id_str(u.type_id)).or_default() += 1; } }
                        }
                        eprintln!("p{p} gold {} lumber {} food {}/{} units {n} {:?}", w.players[p as usize].get_state(1), w.players[p as usize].get_state(2), w.players[p as usize].get_state(5), w.players[p as usize].get_state(4), types);
                    }
                }
                i += 1;
            }
            // spawn <type> <player> <dx> <dy>: creates a unit near player 0's start.
            "spawn" => {
                if let Some(g) = scene.game.as_mut() {
                    let (sx, sy) = g.world.start_of(0);
                    let id = w3_game::objects::id_from_str(&a[i + 1]);
                    let hd = w3_game::natives::units::create_unit(&mut g.world, num(&a[i + 2]) as u8, id, sx + num(&a[i + 3]), sy + num(&a[i + 4]), 270.0);
                    eprintln!("spawned {} = {hd}", a[i + 1]);
                }
                i += 5;
            }
            // spawnat <type> <owner> <x> <y>: creates a unit at a map point.
            "spawnat" => {
                if let Some(g) = scene.game.as_mut() {
                    let id = w3_game::objects::id_from_str(&a[i + 1]);
                    let hd = w3_game::natives::units::create_unit(&mut g.world, num(&a[i + 2]) as u8, id, num(&a[i + 3]), num(&a[i + 4]), 270.0);
                    eprintln!("spawned {} = {hd}", a[i + 1]);
                }
                i += 5;
            }
            // tags: the engine's own text tags now (gold, bounty, miss).
            "tags" => {
                if let Some(g) = scene.game.as_ref() {
                    let texts: Vec<&str> = g.world.local_text_tags.iter().map(|t| t.text.as_str()).collect();
                    eprintln!("tags {:?}", texts);
                }
                i += 1;
            }
            // give <unit> <item type>: creates an item in the unit's inventory.
            "give" => {
                if let Some(g) = scene.game.as_mut() {
                    let u = num(&a[i + 1]) as u32;
                    let id = w3_game::objects::id_from_str(&a[i + 2]);
                    let it = w3_game::natives::objects::create_item(&mut g.world, id, 0.0, 0.0);
                    let ok = g.world.give_item(&mut g.vm, u, it, None);
                    eprintln!("gave {} = {it}: {ok}", a[i + 2]);
                }
                i += 3;
            }
            // useslot <n>: uses the selected unit's item in a slot.
            "useslot" => {
                scene.use_inventory_slot(num(&a[i + 1]) as usize);
                i += 2;
            }
            // books <unit>: prints the unit's spell books and their spells.
            "books" => {
                if let Some(g) = scene.game.as_ref() {
                    if let Some(u) = g.world.unit(num(&a[i + 1]) as u32) {
                        for (b, s) in &u.extra.books {
                            eprintln!("book {} spells {:?}", w3_game::objects::id_str(*b), s.iter().map(|x| w3_game::objects::id_str(*x)).collect::<Vec<_>>());
                        }
                    }
                }
                i += 2;
            }
            // tree <dx> <dy>: prints the standing tree nearest a point (relative to the
            // start) and where it is.
            "tree" => {
                if let Some(g) = scene.game.as_ref() {
                    let (sx, sy) = g.world.start_of(0);
                    let (px, py) = (sx + num(&a[i + 1]), sy + num(&a[i + 2]));
                    let best = g.world.handles.iter().filter_map(|(h, o)| match o {
                        w3_game::types::Obj::Destructable(d) if d.life > 0.0 && g.world.objects.destructables.get(d.type_id).is_some_and(|o| o.str("category").eq_ignore_ascii_case("D")) => Some((h, d.x, d.y)),
                        _ => None,
                    }).min_by(|p, q| (p.1 - px).hypot(p.2 - py).total_cmp(&(q.1 - px).hypot(q.2 - py)));
                    if let Some((h, x, y)) = best {
                        eprintln!("tree {h} at {} {}", x - sx, y - sy);
                    }
                }
                i += 3;
            }
            // hero <handle> <xp> <ability>: grants experience and learns an ability.
            "hero" => {
                if let Some(g) = scene.game.as_mut() {
                    let h = num(&a[i + 1]) as u32;
                    g.world.add_hero_xp(h, num(&a[i + 2]) as i32);
                    let ok = g.world.learn_skill(h, w3_game::objects::id_from_str(&a[i + 3]));
                    let u = g.world.unit(h).unwrap();
                    eprintln!("hero level {} xp {} str {} agi {} int {} life {}/{} mana {}/{} learned {ok} skill points {}", u.level, u.hero_xp, u.str_, u.agi, u.int, u.life, u.max_life, u.mana, u.max_mana, u.skill_points);
                }
                i += 4;
            }
            // orderpt <unit> <order> <dx> <dy>: an order at a point near player 0's start.
            "orderpt" => {
                if let Some(g) = scene.game.as_mut() {
                    let (sx, sy) = g.world.start_of(0);
                    let id = w3_game::natives::basic::order_id(&a[i + 2]);
                    g.world.order_unit(num(&a[i + 1]) as u32, id, sx + num(&a[i + 3]), sy + num(&a[i + 4]), 0);
                }
                i += 5;
            }
            // order <unit> <order> <target>: issues an order at a unit.
            "order" => {
                if let Some(g) = scene.game.as_mut() {
                    let id = w3_game::natives::basic::order_id(&a[i + 2]);
                    g.world.order_error = None;
                    g.world.order_unit(num(&a[i + 1]) as u32, id, 0.0, 0.0, num(&a[i + 3]) as u32);
                    if let Some(e) = g.world.order_error.take() {
                        eprintln!("order refused: {e}");
                    }
                }
                i += 4;
            }
            "unit" => {
                if let Some(g) = scene.game.as_ref() {
                    if let Some(u) = g.world.unit(num(&a[i + 1]) as u32) {
                        eprintln!("unit {} life {}/{} mana {} action {:?} buffs {:?}{}", w3_game::objects::id_str(u.type_id), u.life, u.max_life, u.mana, u.action, u.buffs.iter().map(|b| (w3_game::objects::id_str(b.id), b.remaining, b.stun)).collect::<Vec<_>>(), if u.extra.corpses.is_empty() { String::new() } else { format!(" corpses {}", u.extra.corpses.len()) } + if u.extra.sleeping { " asleep" } else { "" });
                        eprintln!("  at {:.0} {:.0}", u.x, u.y);
                    }
                }
                i += 2;
            }
            "mine" => {
                if let Some(g) = &scene.game {
                    let w = &g.world;
                    let m = num(&a[i + 1]) as u32;
                    if let Some(u) = w.unit(m) { eprintln!("mine at {:.0},{:.0} inside {} gold {}", u.x, u.y, u.inside, u.resource); }
                    for &h in &w.unit_order {
                        if let Some(u) = w.unit(h) { if matches!(u.action, w3_game::types::Action::Harvest{target} if target == m || m == 0) || u.carry_gold > 0 || u.carry_lumber > 0 { eprintln!("  t={:.1} {h} at {:.0},{:.0} {:?} inside {} path {} lumber {} anim {} facing {:.0}", w.time, u.x, u.y, u.action, u.inside, u.path.len(), u.carry_lumber, u.anim, u.facing); } }
                    }
                }
                i += 2;
            }
            // neutrals: prints the neutral players' structures.
            "neutrals" => {
                if let Some(g) = &scene.game {
                    for &h in &g.world.unit_order {
                        if let Some(u) = g.world.unit(h).filter(|u| u.is_building && u.owner >= 12) {
                            eprintln!("  {h} {} owner {} at {:.0},{:.0} hidden {}", w3_game::objects::id_str(u.type_id), u.owner, u.x, u.y, u.hidden);
                        }
                    }
                }
                i += 1;
            }
            "structs" => {
                if let Some(g) = &scene.game {
                    let w = &g.world;
                    for (h, o) in w.handles.iter() {
                        if let w3_game::types::Obj::Unit(u) = o {
                            if u.owner == 1 && u.is_building { eprintln!("  {h} {} at {:.0},{:.0} constructing {:?} dead {} in order {}", w3_game::objects::id_str(u.type_id), u.x, u.y, u.constructing, u.dead, w.unit_order.contains(&h)); }
                        }
                    }
                }
                i += 1;
            }
            "error" => {
                scene.error(&a[i + 1]);
                i += 2;
            }
            "msg" => {
                if let Some(h) = scene.hud.as_mut() { h.unit_message(&a[i + 1], 0.0, 0.0, -1.0); }
                i += 2;
            }
            // item <type> <dx> <dy>: creates an item near player 0's start.
            "item" => {
                if let Some(g) = scene.game.as_mut() {
                    let (sx, sy) = g.world.start_of(0);
                    let id = w3_game::objects::id_from_str(&a[i + 1]);
                    let h = w3_game::natives::objects::create_item(&mut g.world, id, sx + num(&a[i + 2]), sy + num(&a[i + 3]));
                    eprintln!("item {} = {h}", a[i + 1]);
                }
                i += 4;
            }
            "buy" => {
                if let Some(g) = scene.game.as_mut() {
                    let r = g.buy_item(0, num(&a[i + 1]) as u32, w3_game::objects::id_from_str(&a[i + 2]));
                    let (shop, id) = (num(&a[i + 1]) as u32, w3_game::objects::id_from_str(&a[i + 2]));
                    eprintln!("buy: {r:?} gold {} stock {:?}", g.world.players[0].get_state(1), g.world.shops.stock.get(&(shop, id)));
                }
                i += 3;
            }
            // hire <shop> <unit type>: hires a unit for player 0.
            "hire" => {
                if let Some(g) = scene.game.as_mut() {
                    let r = g.hire_unit(0, num(&a[i + 1]) as u32, w3_game::objects::id_from_str(&a[i + 2]));
                    eprintln!("hire: {r:?} gold {} lumber {}", g.world.players[0].get_state(1), g.world.players[0].get_state(2));
                }
                i += 3;
            }
            // shop <h>: the local player's shopper there, and each good's stock and cost.
            "shop" => {
                if let Some(g) = scene.game.as_ref() {
                    let w = &g.world;
                    let h = num(&a[i + 1]) as u32;
                    let p = w.local_player;
                    eprintln!("shopper {:?} gold {} lumber {}", w.shopper(p, h), w.players[p as usize].get_state(1), w.players[p as usize].get_state(2));
                    let (items, units) = w.shop_goods(h);
                    for id in items.into_iter().chain(units) {
                        let cost = w.objects.units.get(id).map(|o| (o.int("goldcost"), o.int("lumbercost"))).or_else(|| w.objects.items.get(id).map(|o| (o.int("goldcost"), o.int("lumbercost"))));
                        eprintln!("good {} stock {} cost {:?}", w3_game::objects::id_str(id), w.stock_count(h, id), cost);
                    }
                }
                i += 2;
            }
            // items: the ground items, where they are and where they show on screen.
            "items" => {
                if let Some(g) = scene.game.as_ref() {
                    for (&h, inst) in &scene.items {
                        let Some(it) = g.world.item(h) else { continue };
                        let p = inst.transform.w_axis.truncate();
                        eprintln!("item {h} {} at ({:.0}, {:.0}) visible {} screen {:?}", w3_game::objects::id_str(it.type_id), it.x, it.y, inst.visible, scene.project(p, (w, h)));
                    }
                }
                i += 1;
            }
            // menubutton <name>: presses a game menu button.
            "menubutton" => {
                scene.update_menu(&assets.frames, w as f32 / h as f32);
                scene.menu_button(&a[i + 1]);
                scene.update_menu(&assets.frames, w as f32 / h as f32);
                i += 2;
            }
            "menu" => {
                scene.open_menu((w as u32, h as u32));
                i += 1;
            }
            // type <text>: keys typed one by one; "\n" in it is Enter.
            "type" => {
                for c in a[i + 1].replace("\\n", "\n").chars() {
                    scene.key(c);
                    scene.update_menu(&assets.frames, w as f32 / h as f32);
                }
                i += 2;
            }
            // tech <upgrade> <level>: gives player 0 research.
            "tech" => {
                if let Some(g) = scene.game.as_mut() {
                    g.world.players[0].tech_researched.insert(w3_game::objects::id_from_str(&a[i + 1]), num(&a[i + 2]) as i32);
                    g.world.research_changed(0);
                }
                i += 3;
            }
            "bonus" => {
                if let Some(g) = scene.game.as_ref() {
                    if let Some(u) = g.world.unit(num(&a[i + 1]) as u32) {
                        eprintln!("{} life {}/{} bonus {:?}", w3_game::objects::id_str(u.type_id), u.life, u.max_life, g.world.upgrade_bonus(u.owner, u.type_id));
                    }
                }
                i += 2;
            }
            "selecth" => {
                scene.selection = vec![num(&a[i + 1]) as u32];
                i += 2;
            }
            // useitemon <unit> <slot> <target>: uses an item on a unit.
            "useitemon" => {
                if let Some(g) = scene.game.as_mut() {
                    let t = num(&a[i + 3]) as u32;
                    let (x, y) = g.world.widget_pos(t).unwrap_or((0.0, 0.0));
                    let ok = g.use_item_at(num(&a[i + 1]) as u32, num(&a[i + 2]) as usize, t, x, y);
                    eprintln!("used: {ok}");
                }
                i += 4;
            }
            "useitem" => {
                if let Some(g) = scene.game.as_mut() {
                    let ok = g.use_item(num(&a[i + 1]) as u32, num(&a[i + 2]) as usize);
                    eprintln!("used: {ok}");
                }
                i += 3;
            }
            // additem <unit> <item>: puts an item in a unit's inventory.
            "additem" => {
                if let Some(g) = scene.game.as_mut() {
                    let (u, it) = (num(&a[i + 1]) as u32, num(&a[i + 2]) as u32);
                    let w3_game::Game { world, vm, .. } = g;
                    world.give_item(vm, u, it, None);
                }
                i += 3;
            }
            // giveitem <unit> <slot> <target>: hands an item to a unit (or sells it to a shop).
            "giveitem" => {
                if let Some(g) = scene.game.as_mut() {
                    g.apply(w3_game::replay::Command::GiveItemTo { unit: num(&a[i + 1]) as u32, slot: num(&a[i + 2]) as usize, target: num(&a[i + 3]) as u32 });
                }
                i += 4;
            }
            "inv" => {
                if let Some(g) = scene.game.as_ref() {
                    if let Some(u) = g.world.unit(num(&a[i + 1]) as u32) {
                        let inv: Vec<String> = u.inventory.iter().map(|&it| g.world.item(it).map(|i| format!("{}x{}", w3_game::objects::id_str(i.type_id), i.charges)).unwrap_or("-".into())).collect();
                        eprintln!("inventory {:?} life {}/{} armor+{} dmg+{}", inv, u.life, u.max_life, u.item_bonus.armor, u.item_bonus.damage);
                    }
                }
                i += 2;
            }
            // killall <player>: kills every unit of a player.
            "killall" => {
                if let Some(g) = scene.game.as_mut() {
                    let p = num(&a[i + 1]) as u8;
                    let hs: Vec<u32> = g.world.unit_order.iter().copied().filter(|&h| g.world.unit(h).is_some_and(|u| u.owner == p && !u.dead)).collect();
                    for h in hs {
                        g.kill(h);
                    }
                }
                i += 2;
            }
            "aidump" => {
                if let Some(g) = &scene.game {
                    let w = &g.world;
                    for &h in &w.unit_order {
                        if let Some(u) = w.unit(h) { if u.owner == 1 && !u.is_building { eprintln!("  {} {:?} gold {} lumber {} inside {} lvl {} abil {:?}", w3_game::objects::id_str(u.type_id), u.action, u.carry_gold, u.carry_lumber, u.inside, u.level, u.abilities.iter().map(|(a, l)| (w3_game::objects::id_str(*a), *l)).collect::<Vec<_>>()); } }
                    }
                    if let Some(a) = w.ai.get(&1) { eprintln!("  harvest {:?}", a.harvest); }
                }
                i += 1;
            }
            // proj: each unit's screen position at its origin and 50 to 300 above.
            "proj" => {
                scene.update(0.02);
                if let Some(g) = &scene.game {
                    let wo = &g.world;
                    for &hd in &wo.unit_order {
                        let Some(u) = wo.unit(hd) else { continue };
                        let z = wo.ground_z(u.x, u.y) + u.fly_height;
                        let pts: Vec<_> = [0.0, 50.0, 100.0, 150.0, 200.0, 250.0, 300.0].iter().filter_map(|dz| scene.project(glam::Vec3::new(u.x, u.y, z + dz), (w, h))).map(|(x, y)| (x.round(), y.round())).collect();
                        let inst = scene.units.get(&hd).map(|v| v.inst.transform.w_axis);
                        eprintln!("proj {} {hd} ground {} terrain {} at ({}, {}) inst {:?} {:?}", w3_game::objects::id_str(u.type_id), wo.ground_z(u.x, u.y), scene.terrain.height_at(u.x, u.y), u.x, u.y, inst, pts);
                    }
                }
                i += 1;
            }
            // flyh <handle> <height>: sets a unit's flying height.
            "flyh" => {
                if let Some(g) = scene.game.as_mut() {
                    if let Some(u) = g.world.unit_mut(num(&a[i + 1]) as u32) {
                        u.fly_height = num(&a[i + 2]);
                    }
                }
                i += 3;
            }
            // heights <x> <y>: terrain heights around a point, 64 apart.
            "heights" => {
                let (x, y) = (num(&a[i + 1]), num(&a[i + 2]));
                for dy in (-3..=3).rev() {
                    let row: Vec<String> = (-3..=3).map(|dx| format!("{:.0}", scene.terrain.height_at(x + dx as f32 * 64.0, y + dy as f32 * 64.0))).collect();
                    eprintln!("heights {}", row.join(" "));
                }
                i += 3;
            }
            // rclick <x> <y>: a right click (the selection's smart order).
            "rclick" => {
                scene.command(num(&a[i + 1]), num(&a[i + 2]), (w, h));
                if let Some(g) = scene.game.as_ref() {
                    eprintln!("formations {:?}", g.world.formations);
                }
                i += 3;
            }
            // scores: every player's GetPlayerScore values.
            "scores" => {
                if let Some(g) = &scene.game {
                    for p in 0..2u8 {
                        let pl = &g.world.players[p as usize];
                        eprintln!("player {p} name {:?} race {} color {} team {}", pl.name, pl.race, pl.color, pl.team);
                        let v: Vec<i32> = (0..25).map(|k| g.world.player_score(p, k)).collect();
                        eprintln!("scores p{p} {:?}", v);
                    }
                }
                i += 1;
            }
            // mana <handle> <amount>: sets a unit's mana.
            "mana" => {
                if let Some(u) = scene.game.as_mut().and_then(|g| g.world.unit_mut(num(&a[i + 1]) as u32)) {
                    u.mana = num(&a[i + 2]);
                    u.max_mana = u.max_mana.max(u.mana);
                }
                i += 3;
            }
            // kill <handle>: kills a unit.
            "kill" => {
                if let Some(g) = scene.game.as_mut() {
                    w3_game::natives::units::kill_unit(&mut g.world, &mut g.vm, num(&a[i + 1]) as u32, 0);
                }
                i += 2;
            }
            // killby <victim> <killer>: kills a unit by another's hand.
            "killby" => {
                if let Some(g) = scene.game.as_mut() {
                    w3_game::natives::units::kill_unit(&mut g.world, &mut g.vm, num(&a[i + 1]) as u32, num(&a[i + 2]) as u32);
                }
                i += 3;
            }
            "alt" => {
                scene.alt_held = true;
                i += 1;
            }
            // waygate <handle> <dx> <dy>: activates a waygate to a point (relative to
            // the start).
            "waygate" => {
                if let Some(g) = scene.game.as_mut() {
                    let (sx, sy) = g.world.start_of(0);
                    let (x, y) = (sx + num(&a[i + 2]), sy + num(&a[i + 3]));
                    if let Some(u) = g.world.unit_mut(num(&a[i + 1]) as u32) {
                        u.extra.waygate = Some((x, y, true));
                    }
                }
                i += 4;
            }
            // ping <dx> <dy>: pings the minimap at a point (relative to the start).
            "ping" => {
                if let Some(g) = scene.game.as_mut() {
                    let (sx, sy) = g.world.start_of(0);
                    g.world.requests.push(w3_game::world::Request::Ping { x: sx + num(&a[i + 1]), y: sy + num(&a[i + 2]), duration: 3.0, color: None });
                }
                i += 3;
            }
            // lookat <handle> <bone> <target>: turns a unit's bone toward another unit.
            "lookat" => {
                if let Some(g) = scene.game.as_mut() {
                    let t = num(&a[i + 3]) as u32;
                    if let Some(u) = g.world.unit_mut(num(&a[i + 1]) as u32) {
                        u.extra.look_at = Some((a[i + 2].clone(), t, [0.0, 0.0, 60.0]));
                    }
                }
                i += 4;
            }
            // find <type> <player>: lists a player's units of a type with their handles,
            // positions, and actions.
            "find" => {
                if let Some(g) = scene.game.as_ref() {
                    let id = w3_game::objects::id_from_str(&a[i + 1]);
                    let p = num(&a[i + 2]) as u8;
                    for &h in &g.world.unit_order {
                        if let Some(u) = g.world.unit(h).filter(|u| u.type_id == id && u.owner == p) {
                            let seen: Vec<u8> = (0..12u8).filter(|&p| g.world.fog_state(u.x, u.y, p) == w3_game::fog::FOG_VISIBLE).collect();
                            eprintln!("found {h} at {:.0} {:.0} dead {} paused {} {:?} seen by {:?}", u.x, u.y, u.dead, u.paused, u.action, seen);
                        }
                    }
                }
                i += 3;
            }
            // dests <x> <y> <r>: destructables near a world point, with life and pathing.
            "dests" => {
                if let Some(g) = scene.game.as_ref() {
                    let (x, y, r) = (num(&a[i + 1]), num(&a[i + 2]), num(&a[i + 3]));
                    for (h, o) in g.world.handles.iter() {
                        if let w3_game::types::Obj::Destructable(d) = o {
                            if (d.x - x).hypot(d.y - y) <= r {
                                eprintln!("dest {h} {} at {:.0} {:.0} facing {:.0} life {} cells {}", w3_game::objects::id_str(d.type_id), d.x, d.y, d.facing, d.life, d.pathing_cells.len());
                            }
                        }
                    }
                }
                i += 4;
            }
            // abils <handle>: a unit's abilities and levels.
            "abils" => {
                if let Some(g) = scene.game.as_ref() {
                    if let Some(u) = g.world.unit(num(&a[i + 1]) as u32) {
                        eprintln!("abils {:?}", u.abilities.iter().map(|(a, l)| format!("{}:{}", w3_game::objects::id_str(*a), l)).collect::<Vec<_>>());
                    }
                }
                i += 2;
            }
            // aithreads: where each computer player's script threads wait.
            "aithreads" => {
                if let Some(g) = scene.game.as_ref() {
                    for (p, a) in &g.world.ai_vms {
                        for (at, t) in &a.sleepers {
                            eprintln!("aithread p{p} wakes {at:.1} {:?}", t.backtrace(&a.vm.module));
                        }
                        if let Some(st) = g.world.ai.get(p) {
                            for (name, c) in [("attack", &st.attack), ("defense", &st.defense)] {
                                let t = g.world.unit(c.target).map(|u| format!("{} {} at {:.0} {:.0} life {:.0}", w3_game::objects::id_str(u.type_id), c.target, u.x, u.y, u.life));
                                let units: Vec<String> = c.units.iter().filter_map(|&h| g.world.unit(h).map(|u| format!("{}@{:.0},{:.0}:{:?}", w3_game::objects::id_str(u.type_id), u.x, u.y, u.action))).collect();
                                eprintln!("captain p{p} {name} target {t:?} goal {:?} units {units:?}", c.goal);
                            }
                        }
                    }
                }
                i += 1;
            }
            // pathtime <x1> <y1> <x2> <y2> <nodes>: times a ground path search.
            "pathtime" => {
                if let Some(g) = scene.game.as_ref() {
                    if let Some(m) = &g.world.pathing {
                        let t0 = std::time::Instant::now();
                        let path = m.find_path((num(&a[i + 1]), num(&a[i + 2])), (num(&a[i + 3]), num(&a[i + 4])), 16.0, num(&a[i + 5]) as usize);
                        eprintln!("pathtime {:?} waypoints {} ends {:?}", t0.elapsed(), path.len(), path.last());
                    }
                }
                i += 6;
            }
            // pathmap <x1> <y1> <x2> <y2>: the walkable cells of a box ('.' open, '#'
            // unwalkable ground, 'o' blocked by an object), top row first.
            "pathmap" => {
                if let Some(g) = scene.game.as_ref() {
                    if let Some(m) = &g.world.pathing {
                        let (a0, b0) = m.cell_of(num(&a[i + 1]), num(&a[i + 2]));
                        let (a1, b1) = m.cell_of(num(&a[i + 3]), num(&a[i + 4]));
                        for cy in (b0..=b1).rev() {
                            let row: String = (a0..=a1).map(|cx| match m.flags_at((cx, cy)) {
                                None => ' ',
                                Some(f) if f & w3_game::pathing::NO_WALK != 0 => '#',
                                Some(_) if m.walkable((cx, cy)) => '.',
                                Some(_) => 'o',
                            }).collect();
                            eprintln!("pathmap {row}");
                        }
                    }
                }
                i += 5;
            }
            // nofog: turns the fog of war and black mask off.
            "nofog" => {
                if let Some(g) = scene.game.as_mut() {
                    g.world.fog_enabled = false;
                    g.world.fog_mask_enabled = false;
                }
                i += 1;
            }
            // camfield <field> <value>: sets a game camera field (CAMERA_FIELD_* order).
            "camfield" => {
                if let Some(g) = scene.game.as_mut() {
                    let f = num(&a[i + 1]) as usize;
                    if f < g.world.camera.fields.len() {
                        g.world.camera.fields[f] = num(&a[i + 2]);
                    }
                }
                i += 3;
            }
            // sounddur <path>: a sound file's length as GetSoundFileDuration measures it.
            "sounddur" => {
                if let Some(g) = scene.game.as_ref() {
                    let ms = g.world.chain.read(&a[i + 1]).ok().and_then(|d| w3_game::natives::presentation::sound_file_ms(&d));
                    eprintln!("sounddur {} {:?}", a[i + 1], ms);
                }
                i += 2;
            }
            // info <unit>: a unit's type, owner, life, place, and action.
            "info" => {
                if let Some(g) = scene.game.as_ref() {
                    let h = num(&a[i + 1]) as u32;
                    if let Some(u) = g.world.unit(h) {
                        eprintln!("info {h} {} p{} life {}/{} at {:.0} {:.0} {:?} swing {:?} acquire {}", w3_game::objects::id_str(u.type_id), u.owner, u.life, u.max_life, u.x, u.y, u.action, u.swing.is_some(), u.acquire_range);
                    }
                }
                i += 2;
            }
            // life <unit> <amount>: sets a unit's life.
            "life" => {
                if let Some(g) = scene.game.as_mut() {
                    if let Some(u) = g.world.unit_mut(num(&a[i + 1]) as u32) {
                        u.life = num(&a[i + 2]).min(u.max_life);
                    }
                }
                i += 3;
            }
            // missiles: every missile in flight.
            "missiles" => {
                if let Some(g) = scene.game.as_ref() {
                    for m in &g.world.missiles {
                        eprintln!("missile {} {} at {:.0} {:.0} to {:?} target {} stampede {:?}", m.id, m.model, m.x, m.y, m.impact, m.target, m.stampede.map(|s| s.2));
                    }
                }
                i += 1;
            }
            // shift <0|1>: holds Shift (queued orders).
            "shift" => {
                scene.shift_held = num(&a[i + 1]) != 0.0;
                i += 2;
            }
            // log: opens the message log (F12).
            "log" => {
                scene.open_log();
                i += 1;
            }
            // quests: opens the quest log (F9).
            "quests" => {
                scene.open_quests();
                i += 1;
            }
            // tod <hour>: sets the time of day.
            "tod" => {
                if let Some(g) = scene.game.as_mut() {
                    g.world.time_of_day = num(&a[i + 1]);
                }
                i += 2;
            }
            "state" => {
                if let Some(g) = &scene.game {
                    let p = &g.world.players[g.world.local_player as usize];
                    eprintln!("gold {} lumber {}", p.get_state(1), p.get_state(2));
                }
                i += 1;
            }
            other => panic!("unknown command {other}"),
        }
    }
    scene.update_menu(&assets.frames, w as f32 / h as f32);
    scene.update(0.02);
    scene.sync_units(&device, &queue, &mut assets);
    scene.sync_destructables(&device, &queue, &mut assets);
    scene.sync_items(&device, &queue, &mut assets);
    let size = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: w3_render::gpu::DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let cv = color.create_view(&Default::default());
    let dv = depth.create_view(&Default::default());
    // Particles emit as frames are drawn: draw a second (W3WARM seconds) of frames
    // before the shot.
    let warm: f32 = std::env::var("W3WARM").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0);
    for _ in 0..(warm * 30.0) as u32 {
        scene.update(1.0 / 30.0);
        let mut enc = device.create_command_encoder(&Default::default());
        scene.render(&device, &queue, &mut enc, &cv, &dv, (w, h), &mut assets);
        queue.submit([enc.finish()]);
    }
    // W3BENCH=<frames>: how long a frame takes (update, sync, draw, and the GPU's
    // finishing it), on average.
    if let Some(n) = std::env::var("W3BENCH").ok().and_then(|v| v.parse::<u32>().ok()) {
        let t0 = std::time::Instant::now();
        let mut cpu = 0.0;
        for _ in 0..n {
            let c0 = std::time::Instant::now();
            scene.update(1.0 / 60.0);
            scene.sync_units(&device, &queue, &mut assets);
            let mut enc = device.create_command_encoder(&Default::default());
            scene.render(&device, &queue, &mut enc, &cv, &dv, (w, h), &mut assets);
            cpu += c0.elapsed().as_secs_f64();
            queue.submit([enc.finish()]);
            device.poll(wgpu::PollType::wait_indefinitely())?;
        }
        let total = t0.elapsed().as_secs_f64();
        eprintln!("bench {n} frames at {w}x{h}: {:.2} ms a frame ({:.2} ms on the CPU)", total * 1000.0 / n as f64, cpu * 1000.0 / n as f64);
    }
    let mut enc = device.create_command_encoder(&Default::default());
    scene.render(&device, &queue, &mut enc, &cv, &dv, (w, h), &mut assets);
    let stride = (w * 4).div_ceil(256) * 256;
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (stride * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    enc.copy_texture_to_buffer(
        color.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(stride), rows_per_image: Some(h) },
        },
        size,
    );
    queue.submit([enc.finish()]);
    buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely())?;
    let data = buf.slice(..).get_mapped_range().map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for row in 0..h {
        let s = (row * stride) as usize;
        px.extend_from_slice(&data[s..s + (w * 4) as usize]);
    }
    for p in px.chunks_mut(4) {
        p[3] = 255;
    }
    image::save_buffer(&a[3], &px, w, h, image::ExtendedColorType::Rgba8)?;
    Ok(())
}
