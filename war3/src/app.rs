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
    pending: Option<(std::path::PathBuf, Vec<w3_game::LobbySlot>)>,
    /// Sound output, handed to each game.
    audio: Option<w3_audio::Audio>,
    assets: Assets,
    last: Instant,
    keys: HashSet<KeyCode>,
    cursor: PhysicalPosition<f64>,
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
            Some(path) => Some(Scene::load(&gpu.device, &gpu.queue, gpu.config.format, &self.data, &mut assets, path, inner.width as f32 / inner.height.max(1) as f32, None)?),
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
            audio,
            assets,
            last: Instant::now(),
            keys: HashSet::new(),
            cursor: PhysicalPosition::new(0.0, 0.0),
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
                if event.state == ElementState::Pressed {
                    if let (Some(scene), Some(text)) = (r.scene.as_mut(), event.text.as_ref()) {
                        for c in text.chars() {
                            scene.key(c);
                        }
                    }
                    if event.physical_key == PhysicalKey::Code(KeyCode::Escape) {
                        if let Some(scene) = r.scene.as_mut() {
                            scene.key('\u{1b}');
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
                    if let Some(d) = scene.drag.as_mut() {
                        d.1 = (position.x as f32, position.y as f32);
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
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
                    scene.camera.distance = (scene.camera.distance - lines * 100.0).clamp(500.0, 5000.0);
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - r.last).as_secs_f32().min(0.25);
                r.last = now;
                if let Some(scene) = r.scene.as_mut() {
                    // Keyboard and screen-edge scrolling (UI\MiscData.txt [CameraRates]).
                    let speed = 3000.0 * dt;
                    let (w, h) = (r.gpu.config.width as f64, r.gpu.config.height as f64);
                    let edge = 2.0;
                    let mut d = glam::Vec2::ZERO;
                    if r.keys.contains(&KeyCode::ArrowLeft) || r.cursor.x <= edge {
                        d.x -= 1.0;
                    }
                    if r.keys.contains(&KeyCode::ArrowRight) || r.cursor.x >= w - 1.0 - edge {
                        d.x += 1.0;
                    }
                    if r.keys.contains(&KeyCode::ArrowUp) || r.cursor.y <= edge {
                        d.y += 1.0;
                    }
                    if r.keys.contains(&KeyCode::ArrowDown) || r.cursor.y >= h - 1.0 - edge {
                        d.y -= 1.0;
                    }
                    scene.camera.target.x += d.x * speed;
                    scene.camera.target.y += d.y * speed;
                    scene.update(dt);
                    scene.update_audio(&r.assets);
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
                        return;
                    }
                    scene.sync_units(&r.gpu.device, &r.gpu.queue, &mut r.assets);
                    scene.sync_destructables(&r.gpu.device, &r.gpu.queue, &mut r.assets);
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
                    glue.update(dt, &mut r.assets);
                    if let Some(a) = r.audio.as_mut() {
                        a.update([0.0; 3]);
                    }
                    match glue.action.take() {
                        Some(crate::glue::GlueAction::Quit) => {
                            el.exit();
                            return;
                        }
                        Some(crate::glue::GlueAction::Play(path, slots)) => {
                            // Draw the loading screen first; the map loads next frame.
                            glue.show_loading(&mut r.assets);
                            r.pending = Some((path, slots));
                        }
                        None if r.pending.is_some() => {
                            let (path, slots) = r.pending.take().unwrap();
                            let aspect = size.0 as f32 / size.1.max(1) as f32;
                            match Scene::load(&r.gpu.device, &r.gpu.queue, r.gpu.config.format, &self.data, &mut r.assets, &path, aspect, Some(&slots)) {
                                Ok(mut s) => {
                                    if let Some(mut a) = r.audio.take() {
                                        a.stop_music();
                                        s.audio = Some(a);
                                    }
                                    r.scene = Some(s);
                                    r.glue_saved = r.glue.take();
                                    if let Some(g) = r.glue_saved.as_mut() {
                                        g.show(crate::glue::Screen::Skirmish, &mut r.assets);
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
