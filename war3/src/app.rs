//! Window, input, and the frame loop.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

use crate::cli::Args;
use crate::data::{self, GameData};
use crate::scene::{Assets, Scene};

struct Running {
    window: Arc<Window>,
    gpu: w3_render::Gpu,
    scene: Option<Scene>,
    /// The menus, shown when no game is running.
    glue: Option<crate::glue::Glue>,
    /// The menus kept while a game runs, to return to afterwards.
    glue_saved: Option<crate::glue::Glue>,
    /// A map to load once the loading screen has been drawn.
    /// The map to load next, with the setup screen's choices or a saved game's state.
    pending: Option<(std::path::PathBuf, Option<Vec<w3_game::LobbySlot>>, Option<w3_game::save::SaveGame>)>,
    /// A replay to watch on the pending map.
    pending_replay: Option<w3_game::replay::Replay>,
    /// The network game the pending map is for.
    pending_net: Option<crate::net::Role>,
    /// A loaded campaign mission waiting for a key on its loading screen.
    ready: Option<Scene>,
    /// Sound output, handed to each game.
    audio: Option<w3_audio::Audio>,
    assets: Assets,
    last: Instant,
    keys: HashSet<KeyCode>,
    cursor: PhysicalPosition<f64>,
    /// Whether the system cursor is hidden for the game's own.
    cursor_hidden: bool,
}

impl Running {
    /// Starts the loaded game: the music stops, the menus are kept for afterwards.
    fn start_ready(r: &mut Running) {
        let Some(mut s) = r.ready.take() else { return };
        if let Some(mut a) = r.audio.take() {
            a.stop_music();
            s.audio = Some(a);
        }
        r.scene = Some(s);
        r.glue_saved = r.glue.take();
        if let Some(g) = r.glue_saved.as_mut() {
            g.press_a_key = false;
            g.show(crate::glue::Screen::Skirmish, &mut r.assets);
        }
    }
}

struct App {
    args: Args,
    data: GameData,
    running: Option<Running>,
    error: Option<anyhow::Error>,
}

pub fn run(args: Args) -> anyhow::Result<()> {
    let dir = data::find_data_dir(args.datadir.as_deref())?;
    let data = GameData::open(&dir, args.classic)?;
    log::info!("game data: {} ({:?})", dir.display(), data.version);
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App { args, data, running: None, error: None };
    event_loop.run_app(&mut app)?;
    match app.error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

impl App {
    fn start(&mut self, el: &ActiveEventLoop) -> anyhow::Result<()> {
        let monitor = el.primary_monitor().or_else(|| el.available_monitors().next());
        let native = monitor.as_ref().map(|m| m.size()).unwrap_or(PhysicalSize::new(1920, 1080));
        let size = PhysicalSize::new(
            self.args.width.unwrap_or(native.width),
            self.args.height.unwrap_or(native.height),
        );
        let mut attrs = Window::default_attributes().with_title(match self.data.version {
            data::Version::FrozenThrone => "Warcraft III: The Frozen Throne",
            data::Version::ReignOfChaos => "Warcraft III",
        });
        if self.args.window {
            attrs = attrs.with_inner_size(size);
        } else {
            attrs = attrs.with_fullscreen(Some(Fullscreen::Borderless(monitor)));
        }
        let window = Arc::new(el.create_window(attrs)?);
        let inner = window.inner_size();
        let backends = if self.args.opengl {
            wgpu::Backends::GL
        } else if self.args.d3d {
            wgpu::Backends::DX12
        } else {
            wgpu::Backends::PRIMARY
        };
        let gpu = w3_render::Gpu::new(window.clone(), inner.width, inner.height, backends, !self.args.no_vsync)?;
        let mut assets = Assets::new(&gpu.device, &gpu.queue, &self.data);
        let mut audio = Some(w3_audio::Audio::new(self.data.chain.clone()));
        let mut scene = match &self.args.loadfile {
            // A saved game starts its map with the saved state.
            Some(path) if path.extension().is_some_and(|e| e.eq_ignore_ascii_case(crate::saveload::EXTENSION)) => {
                let save = w3_game::save::SaveGame::parse(&std::fs::read(path)?)?;
                let map = std::path::PathBuf::from(&save.map);
                Some(Scene::load(&gpu.device, &gpu.queue, gpu.config.format, &self.data, &mut assets, &map, inner.width as f32 / inner.height.max(1) as f32, None, Some(save), None)?)
            }
            Some(path) => Some(Scene::load(&gpu.device, &gpu.queue, gpu.config.format, &self.data, &mut assets, path, inner.width as f32 / inner.height.max(1) as f32, None, None, None)?),
            None => None,
        };
        let glue = match scene.as_mut() {
            Some(s) => {
                s.audio = audio.take();
                None
            }
            None => {
                // The glue plays war3skins.txt GlueMusic_V<n>.
                if let (Some(a), Some(music)) = (audio.as_mut(), assets.frames.skin_value("GlueMusic")) {
                    a.play_music(&music, false, 0);
                }
                Some(crate::glue::Glue::new(&gpu.device, &gpu.queue, gpu.config.format, &self.data, &mut assets))
            }
        };
        self.running = Some(Running {
            window,
            gpu,
            scene,
            glue,
            glue_saved: None,
            pending: None,
            pending_replay: None,
            pending_net: None,
            ready: None,
            audio,
            assets,
            last: Instant::now(),
            keys: HashSet::new(),
            cursor: PhysicalPosition::new(0.0, 0.0),
            cursor_hidden: false,
        });
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.running.is_none() {
            if let Err(e) = self.start(el) {
                self.error = Some(e);
                el.exit();
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(r) = self.running.as_mut() else { return };
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(s) => r.gpu.resize(s.width, s.height),
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed && r.ready.is_some() {
                    Running::start_ready(r);
                    return;
                }
                if event.state == ElementState::Pressed {
                    if let (Some(scene), Some(text)) = (r.scene.as_mut(), event.text.as_ref()) {
                        // Escape comes through its key code below.
                        for c in text.chars().filter(|&c| c != '\u{1b}') {
                            scene.key(c);
                        }
                    }
                    if event.physical_key == PhysicalKey::Code(KeyCode::Escape) {
                        if let Some(scene) = r.scene.as_mut() {
                            scene.key('\u{1b}');
                        }
                    }
                    for (k, key) in [KeyCode::F1, KeyCode::F2, KeyCode::F3].into_iter().enumerate() {
                        if event.physical_key == PhysicalKey::Code(key) {
                            if let Some(scene) = r.scene.as_mut() {
                                scene.hero_key(k);
                            }
                        }
                    }
                    if event.physical_key == PhysicalKey::Code(KeyCode::F12) {
                        if let Some(scene) = r.scene.as_mut() {
                            scene.open_log();
                        }
                    }
                    if event.physical_key == PhysicalKey::Code(KeyCode::F9) {
                        if let Some(scene) = r.scene.as_mut() {
                            scene.open_quests();
                        }
                    }
                    // Digits: control groups.
                    let digits = [KeyCode::Digit0, KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9];
                    if let Some(n) = digits.iter().position(|&d| event.physical_key == PhysicalKey::Code(d)) {
                        let ctrl = r.keys.contains(&KeyCode::ControlLeft) || r.keys.contains(&KeyCode::ControlRight);
                        let shift = r.keys.contains(&KeyCode::ShiftLeft) || r.keys.contains(&KeyCode::ShiftRight);
                        if let Some(scene) = r.scene.as_mut() {
                            scene.control_group(n, ctrl, shift);
                        }
                    }
                    if event.physical_key == PhysicalKey::Code(KeyCode::F8) {
                        if let Some(scene) = r.scene.as_mut() {
                            scene.idle_worker_key();
                        }
                    }
                    if event.physical_key == PhysicalKey::Code(KeyCode::F10) {
                        if let Some(scene) = r.scene.as_mut() {
                            scene.open_menu((r.gpu.config.width, r.gpu.config.height));
                        }
                    }
                    if let Some(glue) = r.glue.as_mut().filter(|g| g.interactive()) {
                        if event.physical_key == PhysicalKey::Code(KeyCode::Escape) {
                            glue.key('\u{1b}', &mut r.assets);
                        } else if let Some(text) = event.text.as_ref() {
                            for c in text.chars() {
                                glue.key(c, &mut r.assets);
                            }
                        }
                    }
                }
                if let PhysicalKey::Code(code) = event.physical_key {
                    match event.state {
                        ElementState::Pressed => {
                            r.keys.insert(code);
                        }
                        ElementState::Released => {
                            r.keys.remove(&code);
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                r.cursor = position;
                if let Some(glue) = r.glue.as_mut() {
                    glue.pointer_move(position.x as f32, position.y as f32, (r.gpu.config.width, r.gpu.config.height));
                }
                if let Some(scene) = r.scene.as_mut() {
                    scene.hover(position.x as f32, position.y as f32, (r.gpu.config.width, r.gpu.config.height));
                    if let Some(d) = scene.drag.as_mut() {
                        d.1 = (position.x as f32, position.y as f32);
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if state == ElementState::Pressed && r.ready.is_some() {
                    Running::start_ready(r);
                    return;
                }
                let size = (r.gpu.config.width, r.gpu.config.height);
                let p = (r.cursor.x as f32, r.cursor.y as f32);
                let shift = r.keys.contains(&KeyCode::ShiftLeft) || r.keys.contains(&KeyCode::ShiftRight);
                if let Some(glue) = r.glue.as_mut().filter(|g| g.interactive()) {
                    match (button, state) {
                        (MouseButton::Left, ElementState::Pressed) => {
                            glue.pointer_down(&mut r.assets);
                        }
                        (MouseButton::Left, ElementState::Released) => {
                            if glue.pointer_up(&mut r.assets).is_some() {
                                if let Some(a) = r.audio.as_mut() {
                                    // UISounds GlueScreenClick on glue buttons.
                                    a.play_label("GlueScreenClick", None);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(scene) = r.scene.as_mut() {
                    match (button, state) {
                        (MouseButton::Left, ElementState::Pressed) => {
                            if !scene.left_click(p.0, p.1, size) {
                                scene.drag = Some((p, p));
                            }
                        }
                        (MouseButton::Left, ElementState::Released) => {
                            scene.left_release(p.0, p.1, size);
                            if let Some((a, b)) = scene.drag.take() {
                                scene.select(a, b, size, shift);
                            }
                        }
                        (MouseButton::Right, ElementState::Pressed) => scene.command(p.0, p.1, size),
                        _ => {}
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(scene) = r.scene.as_mut() {
                    let lines = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y,
                        MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                    };
                    scene.zoom(lines * 100.0);
                }
            }
            WindowEvent::RedrawRequested => {
                // The game and the menus draw their own cursor.
                let hide = r.scene.is_some() || r.glue.is_some();
                if hide != r.cursor_hidden {
                    r.window.set_cursor_visible(!hide);
                    r.cursor_hidden = hide;
                }
                let now = Instant::now();
                let dt = (now - r.last).as_secs_f32().min(0.25);
                r.last = now;
                if let Some(scene) = r.scene.as_mut() {
                    // Keyboard and screen-edge scrolling (UI\MiscData.txt [CameraRates]), at
                    // the rates the options set; the options can turn edge scrolling off.
                    scene.alt_held = r.keys.contains(&KeyCode::AltLeft) || r.keys.contains(&KeyCode::AltRight);
                    scene.shift_held = r.keys.contains(&KeyCode::ShiftLeft) || r.keys.contains(&KeyCode::ShiftRight);
                    scene.ctrl_held = r.keys.contains(&KeyCode::ControlLeft) || r.keys.contains(&KeyCode::ControlRight);
                    let (keys, mouse) = scene.options.scroll_factors();
                    let edges = !scene.options.mousescrolldisable;
                    let (w, h) = (r.gpu.config.width as f64, r.gpu.config.height as f64);
                    let edge = 2.0;
                    let (mut k, mut m) = (glam::Vec2::ZERO, glam::Vec2::ZERO);
                    let key = |c: KeyCode| r.keys.contains(&c);
                    k.x += key(KeyCode::ArrowRight) as i32 as f32 - key(KeyCode::ArrowLeft) as i32 as f32;
                    k.y += key(KeyCode::ArrowUp) as i32 as f32 - key(KeyCode::ArrowDown) as i32 as f32;
                    if edges {
                        m.x += (r.cursor.x >= w - 1.0 - edge) as i32 as f32 - (r.cursor.x <= edge) as i32 as f32;
                        m.y += (r.cursor.y <= edge) as i32 as f32 - (r.cursor.y >= h - 1.0 - edge) as i32 as f32;
                    }
                    scene.scroll_dir = (m.x as i32, m.y as i32);
                    let d = (k * keys + m * mouse) * 3000.0 * dt;
                    if d != glam::Vec2::ZERO {
                        scene.scroll(d.x, d.y);
                    }
                    scene.update(dt);
                    scene.update_audio(&r.assets);
                    if scene.exit {
                        el.exit();
                        return;
                    }
                    // A saved game chosen in the game menu, or a restart: the map's loading
                    // screen, then the game.
                    // A replay restarts as itself.
                    let restart = std::mem::take(&mut scene.restart_requested).then(|| match scene.playback.as_ref() {
                        Some(p) => crate::glue::GlueAction::PlayReplay(Box::new(p.replay.clone())),
                        None => crate::glue::GlueAction::Restart(std::path::PathBuf::from(&scene.map_path), scene.lobby.clone()),
                    });
                    if let Some(action) = scene.load_requested.take().map(crate::glue::GlueAction::LoadSave).or(restart) {
                        let mut audio = scene.audio.take();
                        if let Some(a) = audio.as_mut() {
                            a.stop_music();
                        }
                        r.audio = audio;
                        r.scene = None;
                        r.glue = r.glue_saved.take();
                        if r.glue.is_none() {
                            r.glue = Some(crate::glue::Glue::new(&r.gpu.device, &r.gpu.queue, r.gpu.config.format, &self.data, &mut r.assets));
                        }
                        if let Some(glue) = r.glue.as_mut() {
                            glue.action = Some(action);
                        }
                        return;
                    }
                    // The game's profile changes (campaign progress, saved caches) are kept.
                    let change = scene.change_level.take();
                    let mut scene_score = scene.final_score.take();
                    if scene.ended || change.is_some() {
                        if let Some(g) = scene.game.as_ref() {
                            *self.data.persist.borrow_mut() = g.world.persist.clone();
                            self.data.save_profile();
                        }
                        if let Some(replay) = scene.take_replay() {
                            crate::replays::keep(&replay);
                        }
                    }
                    if let Some(map) = change {
                        // ChangeLevel: the next mission's loading screen, then the mission.
                        let mut audio = scene.audio.take();
                        if let Some(a) = audio.as_mut() {
                            a.stop_music();
                        }
                        r.audio = audio;
                        r.scene = None;
                        r.glue = r.glue_saved.take();
                        if r.glue.is_none() {
                            r.glue = Some(crate::glue::Glue::new(&r.gpu.device, &r.gpu.queue, r.gpu.config.format, &self.data, &mut r.assets));
                        }
                        if let Some(glue) = r.glue.as_mut() {
                            // The score screen first when ChangeLevel asks for it.
                            match scene_score.take() {
                                Some(d) => glue.show_score(d, Some(map), &mut r.assets),
                                None => glue.action = Some(crate::glue::GlueAction::Campaign(map)),
                            }
                        }
                        return;
                    }
                    if scene.ended {
                        // Back to the screen the game started from, with the glue music.
                        let mut audio = scene.audio.take();
                        if let (Some(a), Some(music)) = (audio.as_mut(), r.assets.frames.skin_value("GlueMusic")) {
                            a.play_music(&music, false, 0);
                        }
                        r.audio = audio;
                        r.scene = None;
                        r.glue = r.glue_saved.take();
                        if r.glue.is_none() {
                            r.glue = Some(crate::glue::Glue::new(&r.gpu.device, &r.gpu.queue, r.gpu.config.format, &self.data, &mut r.assets));
                        }
                        if let (Some(glue), Some(d)) = (r.glue.as_mut(), scene_score.take()) {
                            glue.show_score(d, None, &mut r.assets);
                        }
                        return;
                    }
                    scene.sync_units(&r.gpu.device, &r.gpu.queue, &mut r.assets);
                    scene.sync_destructables(&r.gpu.device, &r.gpu.queue, &mut r.assets);
                    scene.sync_items(&r.gpu.device, &r.gpu.queue, &mut r.assets);
                }
                let frame = match r.gpu.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
                    _ => {
                        r.gpu.resize(r.gpu.config.width, r.gpu.config.height);
                        return;
                    }
                };
                let view = frame.texture.create_view(&Default::default());
                let mut enc = r.gpu.device.create_command_encoder(&Default::default());
                let size = (r.gpu.config.width, r.gpu.config.height);
                // The glue's requests: quitting or starting a game.
                if let Some(glue) = r.glue.as_mut() {
                    glue.display = size;
                    glue.update(dt, &mut r.assets);
                    if let Some(a) = r.audio.as_mut() {
                        let (sound, music) = glue.options.volumes();
                        a.set_volumes(sound, music);
                        a.update([0.0; 3]);
                    }
                    match glue.action.take() {
                        Some(crate::glue::GlueAction::Quit) => {
                            el.exit();
                            return;
                        }
                        Some(crate::glue::GlueAction::Play(path, slots)) => {
                            // Draw the loading screen first; the map loads next frame.
                            self.data.campaign_game.set(false);
                            glue.show_loading(&mut r.assets);
                            r.pending = Some((path, Some(slots), None));
                        }
                        Some(crate::glue::GlueAction::Campaign(map)) => {
                            self.data.campaign_game.set(true);
                            glue.skirmish.summary = crate::skirmish::summarize_from(&self.data.chain, &map);
                            glue.show_loading(&mut r.assets);
                            r.pending = Some((std::path::PathBuf::from(map), None, None));
                        }
                        Some(crate::glue::GlueAction::Restart(path, slots)) => {
                            let map = path.to_string_lossy().to_string();
                            glue.skirmish.summary = if path.is_file() { crate::skirmish::summarize_path(&path) } else { crate::skirmish::summarize_from(&self.data.chain, &map) };
                            glue.show_loading(&mut r.assets);
                            r.pending = Some((path, slots, None));
                        }
                        Some(crate::glue::GlueAction::LoadSave(file)) => {
                            match std::fs::read(&file).map_err(anyhow::Error::from).and_then(|b| w3_game::save::SaveGame::parse(&b)) {
                                Ok(save) => {
                                    self.data.campaign_game.set(false);
                                    let map = save.map.clone();
                                    let path = std::path::PathBuf::from(&map);
                                    glue.skirmish.summary = if path.is_file() { crate::skirmish::summarize_path(&path) } else { crate::skirmish::summarize_from(&self.data.chain, &map) };
                                    glue.show_loading(&mut r.assets);
                                    r.pending = Some((path, None, Some(save)));
                                }
                                Err(e) => log::error!("loading {}: {e:#}", file.display()),
                            }
                        }
                        Some(crate::glue::GlueAction::WatchReplay(file)) => match std::fs::read(&file).map_err(anyhow::Error::from).and_then(|b| w3_game::replay::Replay::parse(&b)) {
                            Ok(replay) => glue.action = Some(crate::glue::GlueAction::PlayReplay(Box::new(replay))),
                            Err(e) => log::error!("replay {}: {e:#}", file.display()),
                        },
                        Some(crate::glue::GlueAction::PlayNet(path, slots, role)) => {
                            self.data.campaign_game.set(false);
                            glue.skirmish.summary = crate::skirmish::summarize_path(&path);
                            glue.show_loading(&mut r.assets);
                            r.pending = Some((path, Some(slots), None));
                            r.pending_net = role.borrow_mut().take();
                        }
                        Some(crate::glue::GlueAction::PlayReplay(replay)) => {
                            match Ok::<_, anyhow::Error>(*replay) {
                                Ok(replay) => {
                                    self.data.campaign_game.set(replay.campaign);
                                    let path = std::path::PathBuf::from(&replay.map);
                                    glue.skirmish.summary = if path.is_file() { crate::skirmish::summarize_path(&path) } else { crate::skirmish::summarize_from(&self.data.chain, &replay.map) };
                                    glue.show_loading(&mut r.assets);
                                    r.pending = Some((path, None, None));
                                    r.pending_replay = Some(replay);
                                }
                                Err(e) => log::error!("replay: {e:#}"),
                            }
                        }
                        None if r.pending.is_some() => {
                            let (path, slots, restore) = r.pending.take().unwrap();
                            let aspect = size.0 as f32 / size.1.max(1) as f32;
                            let replay = r.pending_replay.take();
                            match Scene::load(&r.gpu.device, &r.gpu.queue, r.gpu.config.format, &self.data, &mut r.assets, &path, aspect, slots.as_deref(), restore, replay) {
                                Ok(mut s) => {
                                    if let Some(role) = r.pending_net.take() {
                                        s.start_net(role);
                                    }
                                    if self.data.campaign_game.get() {
                                        // Campaign missions wait on their loading screen for a
                                        // key (LOADING_PRESS_A_KEY).
                                        glue.loading_done(&mut r.assets);
                                        r.ready = Some(s);
                                    } else {
                                        r.ready = Some(s);
                                        Running::start_ready(r);
                                        return;
                                    }
                                }
                                Err(e) => log::error!("loading {}: {e:#}", path.display()),
                            }
                        }
                        None => {}
                    }
                }
                if let Some(glue) = r.glue.as_mut() {
                    glue.render(&r.gpu.device, &r.gpu.queue, &mut enc, &view, &r.gpu.depth, size, &mut r.assets);
                } else if let Some(scene) = r.scene.as_mut() {
                    scene.render(&r.gpu.device, &r.gpu.queue, &mut enc, &view, &r.gpu.depth, size, &mut r.assets);
                } else {
                    enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("clear"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        ..Default::default()
                    });
                }
                r.gpu.queue.submit([enc.finish()]);
                r.window.pre_present_notify();
                r.gpu.queue.present(frame);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(r) = &self.running {
            r.window.request_redraw();
        }
    }
}
