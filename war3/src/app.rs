//! Window, input, and the frame loop.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
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
    _assets: Assets,
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
        let scene = match &self.args.loadfile {
            Some(path) => Some(Scene::load(&gpu.device, &gpu.queue, gpu.config.format, &self.data, &mut assets, path)?),
            None => {
                log::warn!("the glue screens are not implemented yet; pass --loadfile <map>");
                None
            }
        };
        self.running = Some(Running {
            window,
            gpu,
            scene,
            _assets: assets,
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
            WindowEvent::CursorMoved { position, .. } => r.cursor = position,
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
                let aspect = r.gpu.aspect();
                if let Some(scene) = r.scene.as_mut() {
                    scene.render(&r.gpu.device, &r.gpu.queue, &mut enc, &view, &r.gpu.depth, aspect);
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
