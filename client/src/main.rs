mod nifti;
mod render;

use glam::{Quat, Vec3};
use render::Renderer;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

const GIT_HASH: &str = env!("GIT_HASH");

// Renderer is created asynchronously on wasm (wgpu adapter request is async), so
// we share a slot between the App and the spawn_local future. Native target can
// fill it synchronously; same plumbing works either way.
type RendererSlot = Rc<RefCell<Option<Renderer>>>;

struct App {
    window: Option<Arc<Window>>,
    renderer: RendererSlot,
    volume: Option<nifti::Volume>,
    mouse_down: bool,
    last_mouse: Option<(f64, f64)>,
    auto_spin: bool,
}

impl App {
    fn new(volume: nifti::Volume) -> Self {
        Self {
            window: None,
            renderer: Rc::new(RefCell::new(None)),
            volume: Some(volume),
            mouse_down: false,
            last_mouse: None,
            auto_spin: true,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes().with_title("cerveau"))
                .unwrap(),
        );

        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowExtWebSys;
            let canvas = window.canvas().expect("no canvas");
            let web_window = web_sys::window().unwrap();
            let dpr = web_window.device_pixel_ratio();
            let w = (web_window.inner_width().unwrap().as_f64().unwrap() * dpr) as u32;
            let h = (web_window.inner_height().unwrap().as_f64().unwrap() * dpr) as u32;
            canvas.set_width(w);
            canvas.set_height(h);
            let _ = canvas.style().set_property("width", "100vw");
            let _ = canvas.style().set_property("height", "100vh");
            let _ = canvas.style().set_property("display", "block");
            web_window
                .document()
                .unwrap()
                .get_element_by_id("wasm-container")
                .unwrap()
                .append_child(&canvas)
                .unwrap();
        }

        let volume = self.volume.take().expect("volume");

        #[cfg(target_arch = "wasm32")]
        {
            let slot = self.renderer.clone();
            let window_for_renderer = window.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let renderer = Renderer::new(window_for_renderer.clone(), volume).await;
                *slot.borrow_mut() = Some(renderer);
                window_for_renderer.request_redraw();
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let renderer = pollster::block_on(Renderer::new(window.clone(), volume));
            *self.renderer.borrow_mut() = Some(renderer);
        }

        self.window = Some(window);
        log::info!("brain viewer up — git {}", GIT_HASH);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _wid: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(r) = self.renderer.borrow_mut().as_mut() {
                    r.resize(size.width, size.height);
                }
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                self.mouse_down = state == ElementState::Pressed;
                if !self.mouse_down {
                    self.last_mouse = None;
                }
                if self.mouse_down {
                    self.auto_spin = false;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = (position.x, position.y);
                if self.mouse_down {
                    if let (Some((lx, ly)), Some(r)) =
                        (self.last_mouse, self.renderer.borrow_mut().as_mut())
                    {
                        let dx = (x - lx) as f32;
                        let dy = (y - ly) as f32;
                        let mag2 = dx * dx + dy * dy;
                        if mag2 > 0.0 {
                            let mag = mag2.sqrt();
                            // Axis perpendicular to drag in the screen plane.
                            // Drag right (+dx) → rotate around screen-up (+Y);
                            // drag down (+dy in pixels) → rotate around screen-right (+X).
                            let axis_cam = Vec3::new(-dy, -dx, 0.0) / mag;
                            let axis_world = (r.camera.orientation * axis_cam).normalize();
                            let angle = mag * 0.006;
                            r.camera.orientation = (Quat::from_axis_angle(axis_world, angle)
                                * r.camera.orientation)
                                .normalize();
                        }
                    }
                }
                self.last_mouse = Some((x, y));
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(r) = self.renderer.borrow_mut().as_mut() {
                    let ticks = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y,
                        MouseScrollDelta::PixelDelta(p) => (p.y as f32) / 60.0,
                    };
                    r.camera.distance = (r.camera.distance * (1.0 - ticks * 0.1)).clamp(1.2, 6.0);
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(r) = self.renderer.borrow_mut().as_mut() {
                    if self.auto_spin {
                        // Slow spin around world-up.
                        r.camera.orientation =
                            (Quat::from_rotation_y(0.004) * r.camera.orientation).normalize();
                    }
                    match r.render() {
                        Ok(()) => {}
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                            let size = self.window.as_ref().unwrap().inner_size();
                            r.resize(size.width, size.height);
                        }
                        Err(e) => log::error!("render error: {:?}", e),
                    }
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }
}

#[cfg(target_arch = "wasm32")]
async fn fetch_brain() -> Result<Vec<u8>, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let window = web_sys::window().ok_or("no window")?;
    let resp_val = JsFuture::from(window.fetch_with_str("/brain.nii"))
        .await
        .map_err(|e| format!("fetch failed: {:?}", e))?;
    let resp: web_sys::Response = resp_val.dyn_into().unwrap();
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let buf = JsFuture::from(resp.array_buffer().map_err(|e| format!("{:?}", e))?)
        .await
        .map_err(|e| format!("{:?}", e))?;
    let arr = js_sys::Uint8Array::new(&buf);
    Ok(arr.to_vec())
}

#[cfg(target_arch = "wasm32")]
fn main() {} // wasm-bindgen takes over via `start` below; cargo still wants a main.

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    console_log::init_with_level(log::Level::Info).ok();
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&format!("{}", info).into());
    }));

    wasm_bindgen_futures::spawn_local(async move {
        log::info!("fetching /brain.nii ...");
        let bytes = match fetch_brain().await {
            Ok(b) => b,
            Err(e) => {
                log::error!("{}", e);
                return;
            }
        };
        log::info!("got {} bytes; parsing ...", bytes.len());
        let volume = match nifti::parse(&bytes) {
            Ok(v) => v,
            Err(e) => {
                log::error!("parse error: {}", e);
                return;
            }
        };
        log::info!(
            "volume {}x{}x{} @ {:?}mm",
            volume.dims[0], volume.dims[1], volume.dims[2], volume.spacing
        );

        let event_loop = EventLoop::new().unwrap();
        let app = App::new(volume);
        use winit::platform::web::EventLoopExtWebSys;
        event_loop.spawn_app(app);
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    env_logger::init();
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "client/brain.nii".to_string());
    let bytes = std::fs::read(&path).expect("read .nii file");
    let volume = nifti::parse(&bytes).expect("parse nifti");
    log::info!(
        "volume {}x{}x{} @ {:?}mm",
        volume.dims[0], volume.dims[1], volume.dims[2], volume.spacing
    );

    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new(volume);
    event_loop.run_app(&mut app).unwrap();
}
