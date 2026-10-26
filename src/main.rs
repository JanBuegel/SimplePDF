// No console window for release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod command;
mod hotkeys;
mod library;
mod logfile;
mod osc;
mod pdf;
mod state;
mod ui;

use std::net::{SocketAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::mpsc;

use clap::Parser;
use winit::event_loop::EventLoop;

use crate::command::{Command, Mode, UserEvent};
use crate::library::Library;
use crate::state::Presentation;

/// Simple PDF presenter: overview / fullscreen, remote controllable via OSC (Bitfocus Companion).
#[derive(Parser)]
#[command(version)]
struct Args {
    /// PDF file to open or folder to list (default: current directory).
    path: Option<PathBuf>,

    /// UDP port for incoming OSC commands.
    #[arg(long, default_value_t = 9000)]
    osc_port: u16,

    /// Address to bind the OSC listener to.
    #[arg(long, default_value = "0.0.0.0")]
    osc_bind: String,

    /// Send status feedback as OSC to HOST:PORT (e.g. Companion).
    #[arg(long)]
    feedback: Option<String>,

    /// Name of the monitor for the output window (default: automatic, the non-primary display).
    #[arg(long)]
    monitor: Option<String>,

    /// Start the fullscreen output immediately.
    #[arg(long)]
    present: bool,

    /// Start in overview mode.
    #[arg(long)]
    overview: bool,
}

fn main() -> anyhow::Result<()> {
    // Handles install/update/uninstall hooks when started by the Velopack installer.
    velopack::VelopackApp::build().run();
    let args = Args::parse();
    logfile::init();
    log::info!(
        "======== SimplePDF {} started: {:?}",
        env!("CARGO_PKG_VERSION"),
        std::env::args().collect::<Vec<_>>()
    );
    for (k, v) in
        std::env::vars().filter(|(k, _)| k.starts_with("SIMPLEPDF_") || k.starts_with("WGPU_"))
    {
        log::info!("{k}={v}");
    }

    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();

    let (req_tx, req_rx) = mpsc::channel();
    let (resp_tx, resp_rx) = mpsc::channel();
    let wake = proxy.clone();
    let mut errors = Vec::new();
    let requests = match pdf::spawn(req_rx, resp_tx, move || {
        let _ = wake.send_event(UserEvent::PdfReady);
    }) {
        Ok(()) => Some(req_tx),
        Err(e) => {
            errors.push(e.to_string());
            None
        }
    };

    let mut initial = Vec::new();
    let dir = match &args.path {
        Some(p) if p.is_dir() => p.clone(),
        Some(p) => {
            initial.push(Command::OpenFile(p.clone()));
            p.parent()
                .filter(|d| !d.as_os_str().is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| ".".into())
        }
        None => ".".into(),
    };
    if args.overview {
        initial.push(Command::SetMode(Mode::Uebersicht));
    }
    if args.present {
        initial.push(Command::SetOutput(true));
    }

    let osc_addr = format!("{}:{}", args.osc_bind, args.osc_port);
    let osc_proxy = proxy.clone();
    let mut osc_info = match osc_addr
        .parse::<SocketAddr>()
        .map_err(anyhow::Error::from)
        .and_then(|addr| {
            osc::spawn_listener(addr, move |cmd| {
                let _ = osc_proxy.send_event(UserEvent::Command(cmd));
            })
        }) {
        Ok(()) => format!("OSC: udp {osc_addr}"),
        Err(e) => {
            errors.push(format!("OSC {osc_addr}: {e}"));
            "OSC: aus".into()
        }
    };

    let feedback = args.feedback.as_deref().and_then(|f| {
        match f
            .to_socket_addrs()
            .map_err(anyhow::Error::from)
            .and_then(|mut a| {
                let addr = a.next().ok_or_else(|| anyhow::anyhow!("no address"))?;
                osc::Feedback::new(addr)
            }) {
            Ok(fb) => {
                osc_info.push_str(&format!(" · Feedback an {f}"));
                Some(fb)
            }
            Err(e) => {
                errors.push(format!("Feedback {f}: {e}"));
                None
            }
        }
    });

    let mut pres = Presentation::new(Library::new(dir), requests, resp_rx);
    pres.output_monitor = args.monitor;
    if !errors.is_empty() {
        for e in &errors {
            eprintln!("{e}");
        }
        pres.error = Some(errors.join(" · "));
    }

    let keys_proxy = proxy.clone();
    let keys = hotkeys::GlobalKeys::spawn(move |cmd| {
        let _ = keys_proxy.send_event(UserEvent::Command(cmd));
    });

    let mut app = app::App::new(pres, feedback, osc_info, initial, keys, proxy);
    event_loop.run_app(&mut app)?;
    Ok(())
}
