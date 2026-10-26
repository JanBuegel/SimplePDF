//! winit application: one control window (internal display) and an optional fullscreen output window.

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

use egui::ViewportId;
use egui_wgpu::wgpu::PowerPreference;
use egui_wgpu::winit::Painter;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::monitor::MonitorHandle;
use winit::window::{Fullscreen, Window, WindowAttributes, WindowId, WindowLevel};

use crate::command::{Command, Mode, UserEvent};
use crate::hotkeys::GlobalKeys;
use crate::library::is_pdf;
use crate::osc::Feedback;
use crate::state::Presentation;
use crate::ui::{self, ControlInfo, ControlUi, Textures};

/// GPU + egui state of a window. Kept alive while the output window is closed: tearing down a
/// second Vulkan device while another window keeps rendering crashes some drivers (NVIDIA).
struct Gfx {
    ctx: egui::Context,
    painter: Painter,
    textures: Textures,
    /// Set when the last frame could not be presented (surface outdated, occluded, ...).
    frame_dropped: Arc<Mutex<Option<String>>>,
}

/// How soon to try again after a dropped frame, even if egui's repaint request was already
/// satisfied by the frame in progress when the surface failed.
const RETRY_DROPPED_FRAME: Duration = Duration::from_millis(100);

/// Which GPU and graphics API a window renders with.
///
/// On laptops with two GPUs (e.g. Intel + NVIDIA) each display is wired to one of them. A window
/// rendered on the other GPU has to be copied across, and that copy stalls on some drivers while
/// the other window is fullscreen. So the control window (usually on the internal display, wired
/// to the integrated GPU) and the output (usually on the external display, wired to the discrete
/// GPU) each pick their own. Override with `SIMPLEPDF_{CONTROL,OUTPUT}_GPU=low|high|none` and
/// `SIMPLEPDF_{CONTROL,OUTPUT}_BACKEND=dx12|vulkan|gl`.
#[derive(Clone, Copy, Debug)]
struct GpuChoice {
    power: egui_wgpu::wgpu::PowerPreference,
    backends: egui_wgpu::wgpu::Backends,
}

impl GpuChoice {
    fn for_window(role: &str, power: egui_wgpu::wgpu::PowerPreference) -> Self {
        use egui_wgpu::wgpu::{Backends, PowerPreference};
        let env = |what: &str| std::env::var(format!("SIMPLEPDF_{role}_{what}")).ok();
        let power = match env("GPU").as_deref() {
            Some("low") => PowerPreference::LowPower,
            Some("high") => PowerPreference::HighPerformance,
            Some("none") => PowerPreference::None,
            _ => power,
        };
        // Intel's Vulkan driver crashes on some laptops; DirectX 12 is the native API there.
        let default = if cfg!(windows) {
            Backends::DX12
        } else {
            Backends::PRIMARY
        };
        let backends = env("BACKEND").map_or(default, |b| Backends::from_comma_list(&b));
        Self { power, backends }
    }
}

/// Swap chain presentation mode, `SIMPLEPDF_PRESENT=fifo|mailbox|immediate|auto` (default: vsync).
fn present_mode() -> egui_wgpu::wgpu::PresentMode {
    use egui_wgpu::wgpu::PresentMode;
    match std::env::var("SIMPLEPDF_PRESENT").as_deref() {
        Ok("fifo") => PresentMode::Fifo,
        Ok("mailbox") => PresentMode::Mailbox,
        Ok("immediate") => PresentMode::Immediate,
        Ok("auto") => PresentMode::AutoNoVsync,
        _ => PresentMode::AutoVsync,
    }
}

struct Win {
    window: Arc<Window>,
    egui: egui_winit::State,
    gfx: Gfx,
    next_repaint: Option<Instant>,
    /// Frames in a row that could not be presented.
    dropped_frames: u32,
    role: &'static str,
    stats: Stats,
}

/// Per-window counters, logged every few seconds while something happens.
#[derive(Default)]
struct Stats {
    frames: u32,
    dropped: u32,
    clicks: u32,
    slowest: Duration,
}

const STATS_INTERVAL: Duration = Duration::from_secs(2);

impl Win {
    fn new(
        event_loop: &ActiveEventLoop,
        attrs: WindowAttributes,
        gfx: Option<Gfx>,
        role: &'static str,
        gpu: GpuChoice,
        proxy: EventLoopProxy<UserEvent>,
    ) -> anyhow::Result<Self> {
        let window = Arc::new(event_loop.create_window(attrs)?);
        let fresh = gfx.is_none();
        let mut gfx = gfx.unwrap_or_else(|| {
            let ctx = egui::Context::default();
            let frame_dropped = Arc::new(Mutex::new(None));
            let dropped = frame_dropped.clone();
            let mut setup = egui_wgpu::WgpuSetupCreateNew::from_display_handle(
                event_loop.owned_display_handle(),
            );
            setup.instance_descriptor.backends = gpu.backends;
            setup.power_preference = gpu.power;
            let config = egui_wgpu::WgpuConfiguration {
                wgpu_setup: setup.into(),
                surface: egui_wgpu::SurfaceConfig {
                    present_mode: present_mode(),
                    ..egui_wgpu::SurfaceConfig::HIGH_THROUGHPUT
                },
                on_surface_status: Arc::new(move |status| {
                    use egui_wgpu::SurfaceErrorAction;
                    use egui_wgpu::wgpu::CurrentSurfaceTexture;
                    if let Ok(mut d) = dropped.lock() {
                        *d = Some(format!("{status:?}"));
                    }
                    match status {
                        CurrentSurfaceTexture::Lost => SurfaceErrorAction::RecreateSurface,
                        // A fresh swap chain also recovers from timeouts and occlusion that
                        // otherwise persist, e.g. while another window is fullscreen.
                        _ => SurfaceErrorAction::Reconfigure,
                    }
                }),
            };
            let options = egui_wgpu::RendererOptions::default();
            let painter = pollster::block_on(Painter::new(ctx.clone(), config, false, options));
            Gfx {
                ctx,
                painter,
                frame_dropped,
                textures: Textures::new(),
            }
        });
        pollster::block_on(
            gfx.painter
                .set_window(ViewportId::ROOT, Some(window.clone())),
        )?;
        if fresh && let Some(rs) = gfx.painter.render_state() {
            let info = rs.adapter.get_info();
            log::info!(
                "{role}: {gpu:?}, {:?} on {} ({:?}, driver {} {}), present {:?}",
                info.backend,
                info.name,
                info.device_type,
                info.driver,
                info.driver_info,
                present_mode(),
            );
        }
        let egui = egui_winit::State::new(
            gfx.ctx.clone(),
            ViewportId::ROOT,
            event_loop,
            Some(window.scale_factor() as f32),
            event_loop.system_theme(),
            gfx.painter.max_texture_side(),
        );
        let id = window.id();
        gfx.ctx.set_request_repaint_callback(move |info| {
            if let Some(when) = Instant::now().checked_add(info.delay) {
                let _ = proxy.send_event(UserEvent::Repaint(
                    id,
                    when,
                    info.current_cumulative_pass_nr,
                ));
            }
        });
        Ok(Self {
            window,
            egui,
            gfx,
            next_repaint: None,
            dropped_frames: 0,
            role,
            stats: Stats::default(),
        })
    }

    /// Builds a fresh swap chain, as a resize would.
    fn reconfigure(&mut self) {
        let size = self.window.inner_size();
        if let (Some(w), Some(h)) = (
            std::num::NonZeroU32::new(size.width),
            std::num::NonZeroU32::new(size.height),
        ) {
            self.gfx.painter.on_window_resized(ViewportId::ROOT, w, h);
        }
        self.window.request_redraw();
    }

    /// Closes the window but keeps the GPU device for reuse.
    fn close(self) -> Gfx {
        let mut gfx = self.gfx;
        let _ = pollster::block_on(gfx.painter.set_window(ViewportId::ROOT, None));
        gfx
    }

    /// Runs one egui frame and paints it.
    fn frame(&mut self, pres: &Presentation, ui_fn: impl FnMut(&mut egui::Ui, &Textures)) {
        let start = Instant::now();
        let mut ui_fn = ui_fn;
        let input = self.egui.take_egui_input(&self.window);
        let Gfx {
            ctx,
            painter,
            textures,
            ..
        } = &mut self.gfx;
        let mut out = ctx.run_ui(input, |ui| {
            // Inside the frame, so egui already knows the real maximum texture size.
            ui::sync_textures(ui.ctx(), pres, textures);
            ui_fn(ui, textures)
        });
        self.egui
            .handle_platform_output(&self.window, out.platform_output);
        let prims = ctx.tessellate(out.shapes, out.pixels_per_point);
        painter.paint_and_update_textures(
            ViewportId::ROOT,
            out.pixels_per_point,
            [0.0, 0.0, 0.0, 1.0],
            &prims,
            &mut out.textures_delta,
            Vec::new(),
            &self.window,
        );
        self.next_repaint = out
            .viewport_output
            .get(&ViewportId::ROOT)
            .and_then(|v| Instant::now().checked_add(v.repaint_delay));
        let dropped = self
            .gfx
            .frame_dropped
            .lock()
            .ok()
            .and_then(|mut d| d.take());
        if let Some(status) = dropped {
            // Only the first of a series; the retry loop would flood the log.
            if self.dropped_frames == 0 {
                log::info!("{}: frame dropped: {status}", self.role);
            }
            self.dropped_frames += 1;
            self.stats.dropped += 1;
            let retry = Instant::now() + RETRY_DROPPED_FRAME;
            self.next_repaint = Some(self.next_repaint.map_or(retry, |t| t.min(retry)));
        } else if self.dropped_frames > 0 {
            log::info!(
                "{}: presenting again after {} dropped frames",
                self.role,
                self.dropped_frames
            );
            self.dropped_frames = 0;
        }
        self.stats.frames += 1;
        self.stats.slowest = self.stats.slowest.max(start.elapsed());
    }

    fn log_stats(&mut self) {
        let s = std::mem::take(&mut self.stats);
        if s.frames > 0 || s.clicks > 0 {
            log::info!(
                "{}: {} frames ({} dropped), slowest {} ms, {} clicks",
                self.role,
                s.frames,
                s.dropped,
                s.slowest.as_millis(),
                s.clicks,
            );
        }
    }
}

pub struct App {
    pres: Presentation,
    control: Option<Win>,
    output: Option<Win>,
    /// GPU state of the closed output window, reused when it opens again.
    parked_output: Option<Gfx>,
    /// Name of the monitor the output window currently covers.
    output_on: Option<String>,
    /// Width / height of the (prospective) output monitor.
    output_aspect: f32,
    control_ui: ControlUi,
    feedback: Option<Feedback>,
    osc_info: String,
    pending: Vec<Command>,
    /// System-wide clicker keys (Windows), enabled while the output is shown.
    keys: Option<GlobalKeys>,
    last_stats: Instant,
    proxy: EventLoopProxy<UserEvent>,
}

impl App {
    pub fn new(
        pres: Presentation,
        feedback: Option<Feedback>,
        osc_info: String,
        initial: Vec<Command>,
        keys: Option<GlobalKeys>,
        proxy: EventLoopProxy<UserEvent>,
    ) -> Self {
        Self {
            pres,
            control: None,
            output: None,
            parked_output: None,
            output_on: None,
            output_aspect: 16.0 / 9.0,
            control_ui: ControlUi::default(),
            feedback,
            osc_info,
            pending: initial,
            keys,
            last_stats: Instant::now(),
            proxy,
        }
    }

    fn request_redraw(&self) {
        for w in [&self.control, &self.output].into_iter().flatten() {
            w.window.request_redraw();
        }
    }

    fn monitors(event_loop: &ActiveEventLoop) -> Vec<MonitorHandle> {
        event_loop.available_monitors().collect()
    }

    fn monitor_name(m: &MonitorHandle) -> String {
        m.name()
            .unwrap_or_else(|| format!("{}x{}", m.size().width, m.size().height))
    }

    /// The configured monitor if connected; otherwise one that isn't showing the control window.
    fn pick_output_monitor(&self, event_loop: &ActiveEventLoop) -> Option<MonitorHandle> {
        let monitors = Self::monitors(event_loop);
        if let Some(name) = &self.pres.output_monitor
            && let Some(m) = monitors.iter().find(|m| &Self::monitor_name(m) == name)
        {
            return Some(m.clone());
        }
        let control_on = self
            .control
            .as_ref()
            .and_then(|c| c.window.current_monitor())
            .or_else(|| event_loop.primary_monitor());
        monitors
            .iter()
            .find(|m| Some(*m) != control_on.as_ref())
            .cloned()
            .or(control_on)
            .or_else(|| monitors.first().cloned())
    }

    /// Opens, moves or closes the output window to match the presentation state.
    fn sync_output(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(m) = self.pick_output_monitor(event_loop) {
            let size = m.size();
            if size.width > 0 && size.height > 0 {
                self.output_aspect = size.width as f32 / size.height as f32;
            }
        }
        if !self.pres.output_wanted {
            if let Some(out) = self.output.take() {
                log::info!("output: closing");
                self.parked_output = Some(out.close());
                self.output_on = None;
                if let Some(c) = &mut self.control {
                    c.reconfigure();
                }
            }
            return;
        }
        let monitor = self.pick_output_monitor(event_loop);
        let name = monitor.as_ref().map(Self::monitor_name);
        // On its own display the output stays on top, even when another window gets focus.
        // Sharing the display with the control window, it must not cover it permanently.
        let control_on = self
            .control
            .as_ref()
            .and_then(|c| c.window.current_monitor())
            .map(|m| Self::monitor_name(&m));
        let level = if name.is_some() && name != control_on {
            WindowLevel::AlwaysOnTop
        } else {
            WindowLevel::Normal
        };
        match &self.output {
            Some(out) => {
                if name != self.output_on {
                    out.window
                        .set_fullscreen(Some(Fullscreen::Borderless(monitor)));
                    out.window.set_window_level(level);
                    self.output_on = name;
                }
            }
            None => {
                // Fullscreen only once the swap chain exists: DirectX 12 refuses to create one
                // for a window that is already fullscreen (0x887A0001).
                let mut attrs = Window::default_attributes()
                    .with_title("SimplePDF – Ausgabe")
                    .with_decorations(false)
                    .with_window_level(level);
                if let Some(m) = &monitor {
                    attrs = attrs.with_position(m.position()).with_inner_size(m.size());
                }
                log::info!("output: opening on {name:?} ({level:?}), control on {control_on:?}");
                let gpu = GpuChoice::for_window("OUTPUT", PowerPreference::HighPerformance);
                match Win::new(
                    event_loop,
                    attrs,
                    self.parked_output.take(),
                    "output",
                    gpu,
                    self.proxy.clone(),
                ) {
                    Ok(w) => {
                        w.window
                            .set_fullscreen(Some(Fullscreen::Borderless(monitor)));
                        w.window
                            .set_cursor_visible(self.pres.mode == Mode::Uebersicht);
                        self.output = Some(w);
                        self.output_on = name;
                        if let Some(c) = &mut self.control {
                            c.reconfigure();
                        }
                    }
                    Err(e) => {
                        log::error!("output window: {e}");
                        self.pres.output_wanted = false;
                        self.pres.error = Some(format!("Ausgabefenster: {e}"));
                    }
                }
            }
        }
    }

    fn apply_pending(&mut self, event_loop: &ActiveEventLoop) {
        let changed = !self.pending.is_empty();
        for cmd in std::mem::take(&mut self.pending) {
            log::debug!("command {cmd:?}");
            self.pres.apply(cmd);
        }
        if self.pres.poll_renderer() || changed {
            self.pres.schedule_renders();
            self.sync_output(event_loop);
            if let Some(keys) = &mut self.keys {
                keys.set_enabled(self.output.is_some() && self.pres.global_clicker);
            }
            if let Some(out) = &self.output {
                out.window
                    .set_cursor_visible(self.pres.mode == Mode::Uebersicht);
            }
            if let Some(fb) = &mut self.feedback {
                fb.send(&self.pres.status());
            }
            self.request_redraw();
        }
    }

    fn key_command(key: &Key) -> Option<Command> {
        Some(match key {
            Key::Named(
                NamedKey::ArrowRight
                | NamedKey::ArrowDown
                | NamedKey::PageDown
                | NamedKey::Space
                | NamedKey::Enter,
            ) => Command::NextPage,
            Key::Named(
                NamedKey::ArrowLeft | NamedKey::ArrowUp | NamedKey::PageUp | NamedKey::Backspace,
            ) => Command::PrevPage,
            Key::Named(NamedKey::Home) => Command::FirstPage,
            Key::Named(NamedKey::End) => Command::LastPage,
            Key::Named(NamedKey::F5) => Command::SetOutput(true),
            Key::Named(NamedKey::Escape) => Command::SetOutput(false),
            Key::Character(c) => match c.as_str() {
                "o" | "O" | "u" | "U" => Command::ToggleMode,
                // Presenter remotes send "b" or "." for black screen.
                "b" | "B" | "." => Command::ToggleBlack,
                "f" | "F" => Command::ToggleOutput,
                "]" => Command::NextFile,
                "[" => Command::PrevFile,
                _ => return None,
            },
            _ => return None,
        })
    }

    fn redraw_control(&mut self) {
        let monitors: Vec<String> = self
            .control
            .as_ref()
            .map(|c| {
                c.window
                    .available_monitors()
                    .map(|m| Self::monitor_name(&m))
                    .collect()
            })
            .unwrap_or_default();
        let Some(win) = &mut self.control else { return };
        let info = ControlInfo {
            monitors: &monitors,
            output_on: self.output_on.as_deref(),
            osc: &self.osc_info,
            clicker: self.keys.as_ref().map(|k| (k.is_enabled(), k.registered())),
            output_aspect: self.output_aspect,
        };
        let (pres, cui, pending) = (&self.pres, &mut self.control_ui, &mut self.pending);
        win.frame(pres, |ui, tex| {
            ui::control(ui, pres, tex, cui, &info, pending)
        });
    }

    fn redraw_output(&mut self) {
        let Some(win) = &mut self.output else { return };
        let (pres, pending) = (&self.pres, &mut self.pending);
        win.frame(pres, |ui, tex| {
            let rect = ui.max_rect();
            if let Some(p) = ui::output_view(ui, rect, pres, tex, true) {
                pending.push(Command::GotoPage(p));
                pending.push(Command::SetMode(Mode::Vollbild));
            }
        });
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.control.is_some() {
            return;
        }
        // Full renders are sized for the largest connected display.
        let (w, h) = Self::monitors(event_loop)
            .iter()
            .map(|m| (m.size().width, m.size().height))
            .fold((1920, 1080), |(w, h), (mw, mh)| (w.max(mw), h.max(mh)));
        self.pres.full_box = (w, h);

        let mut attrs = Window::default_attributes()
            .with_title(format!(
                "SimplePDF {} – Steuerung",
                env!("CARGO_PKG_VERSION")
            ))
            .with_inner_size(winit::dpi::LogicalSize::new(1400.0, 900.0));
        if let Some(primary) = event_loop.primary_monitor() {
            attrs = attrs.with_position(primary.position());
        }
        for m in Self::monitors(event_loop) {
            log::info!(
                "monitor {:?}: {:?} at {:?}, {} Hz, scale {}",
                Self::monitor_name(&m),
                m.size(),
                m.position(),
                m.refresh_rate_millihertz().unwrap_or(0) / 1000,
                m.scale_factor(),
            );
        }
        let gpu = GpuChoice::for_window("CONTROL", PowerPreference::LowPower);
        match Win::new(event_loop, attrs, None, "control", gpu, self.proxy.clone()) {
            Ok(w) => self.control = Some(w),
            Err(e) => {
                log::error!("control window: {e}");
                eprintln!("Cannot create window: {e}");
                event_loop.exit();
                return;
            }
        }
        self.apply_pending(event_loop);
        self.sync_output(event_loop);
        self.request_redraw();
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Command(cmd) => self.pending.push(cmd),
            UserEvent::PdfReady => {}
            UserEvent::Repaint(id, when, pass) => {
                for w in [&mut self.control, &mut self.output].into_iter().flatten() {
                    // Ignore requests already satisfied by a later egui pass.
                    if w.window.id() == id && w.gfx.ctx.cumulative_pass_nr() <= pass + 1 {
                        w.next_repaint = Some(w.next_repaint.map_or(when, |t| t.min(when)));
                    }
                }
            }
        }
        self.apply_pending(event_loop);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let is_control = self.control.as_ref().is_some_and(|w| w.window.id() == id);
        let Some(win) = (if is_control {
            self.control.as_mut()
        } else {
            self.output.as_mut()
        }) else {
            return;
        };
        if win.window.id() != id {
            return;
        }

        let response = win.egui.on_window_event(&win.window, &event);
        // egui marks RedrawRequested as needing paint, which we perform below. Requesting
        // another native redraw here creates an endless WM_PAINT loop on Windows and can
        // starve the other window's paint messages, especially with fullscreen output.
        if response.repaint && !matches!(event, WindowEvent::RedrawRequested) {
            win.window.request_redraw();
        }

        match event {
            WindowEvent::CloseRequested if is_control => event_loop.exit(),
            WindowEvent::CloseRequested => self.pending.push(Command::SetOutput(false)),
            WindowEvent::Resized(size) => {
                if let (Some(w), Some(h)) = (
                    std::num::NonZeroU32::new(size.width),
                    std::num::NonZeroU32::new(size.height),
                ) {
                    win.gfx.painter.on_window_resized(ViewportId::ROOT, w, h);
                }
                win.window.request_redraw();
            }
            // Windows may show a cached old frame when a window becomes visible again.
            WindowEvent::Occluded(false) | WindowEvent::Focused(true) => {
                log::debug!("{}: {event:?}", win.role);
                win.window.request_redraw()
            }
            WindowEvent::Occluded(true) | WindowEvent::Focused(false) | WindowEvent::Moved(_) => {
                log::debug!("{}: {event:?}", win.role);
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                ..
            } => win.stats.clicks += 1,
            WindowEvent::RedrawRequested => {
                if is_control {
                    self.redraw_control()
                } else {
                    self.redraw_output()
                }
            }
            WindowEvent::DroppedFile(path) => {
                if path.is_dir() {
                    self.pending.push(Command::SetFolder(path));
                } else if is_pdf(&path) {
                    if let Some(dir) = path.parent().filter(|d| *d != self.pres.library.dir) {
                        self.pending.push(Command::SetFolder(dir.to_path_buf()));
                    }
                    self.pending.push(Command::OpenFile(path));
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key,
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => {
                if !win.gfx.ctx.egui_wants_keyboard_input()
                    && let Some(cmd) = Self::key_command(&logical_key)
                {
                    self.pending.push(cmd);
                }
            }
            _ => {}
        }
        self.apply_pending(event_loop);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.apply_pending(event_loop);
        let now = Instant::now();
        if now - self.last_stats >= STATS_INTERVAL {
            self.last_stats = now;
            for w in [&mut self.control, &mut self.output].into_iter().flatten() {
                w.log_stats();
            }
        }
        let mut wake: Option<Instant> = None;
        for w in [&mut self.control, &mut self.output].into_iter().flatten() {
            match w.next_repaint {
                Some(t) if t <= now => {
                    w.next_repaint = None;
                    w.window.request_redraw();
                }
                Some(t) => wake = Some(wake.map_or(t, |x| x.min(t))),
                None => {}
            }
        }
        event_loop.set_control_flow(wake.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presenter_keys_change_pages_not_files() {
        let page_keys = [
            (NamedKey::ArrowRight, true),
            (NamedKey::ArrowDown, true),
            (NamedKey::PageDown, true),
            (NamedKey::Space, true),
            (NamedKey::ArrowLeft, false),
            (NamedKey::ArrowUp, false),
            (NamedKey::PageUp, false),
            (NamedKey::Backspace, false),
        ];
        for (key, forward) in page_keys {
            let cmd = App::key_command(&Key::Named(key));
            if forward {
                assert!(matches!(cmd, Some(Command::NextPage)), "{key:?}");
            } else {
                assert!(matches!(cmd, Some(Command::PrevPage)), "{key:?}");
            }
        }
    }
}
