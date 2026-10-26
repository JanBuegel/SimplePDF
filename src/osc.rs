//! OSC remote control (e.g. Bitfocus Companion "Generic OSC") and optional status feedback.

use std::net::{SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::thread;

use rosc::{OscMessage, OscPacket, OscType};

use crate::command::{Command, Mode};

/// Listens for OSC on `bind` and forwards parsed commands to `send`.
pub fn spawn_listener(
    bind: SocketAddr,
    send: impl Fn(Command) + Send + 'static,
) -> anyhow::Result<()> {
    let socket = UdpSocket::bind(bind)?;
    thread::Builder::new().name("osc".into()).spawn(move || {
        let mut buf = [0u8; rosc::decoder::MTU];
        loop {
            let Ok((n, from)) = socket.recv_from(&mut buf) else {
                continue;
            };
            match rosc::decoder::decode_udp(&buf[..n]) {
                Ok((_, packet)) => handle_packet(packet, &send),
                Err(e) => eprintln!("OSC: invalid packet from {from}: {e}"),
            }
        }
    })?;
    Ok(())
}

fn handle_packet(packet: OscPacket, send: &impl Fn(Command)) {
    match packet {
        OscPacket::Message(msg) => match parse(&msg) {
            Some(cmd) => send(cmd),
            None => eprintln!("OSC: ignored {} {:?}", msg.addr, msg.args),
        },
        OscPacket::Bundle(b) => b.content.into_iter().for_each(|p| handle_packet(p, send)),
    }
}

fn arg_int(a: &OscType) -> Option<i64> {
    match a {
        OscType::Int(i) => Some(*i as i64),
        OscType::Long(i) => Some(*i),
        OscType::Float(f) => Some(f.round() as i64),
        OscType::Double(f) => Some(f.round() as i64),
        OscType::Bool(b) => Some(*b as i64),
        OscType::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn arg_bool(a: &OscType) -> Option<bool> {
    match a {
        OscType::String(s) => match s.trim().to_ascii_lowercase().as_str() {
            "on" | "true" | "yes" => Some(true),
            "off" | "false" | "no" => Some(false),
            other => other.parse::<i64>().ok().map(|i| i != 0),
        },
        other => arg_int(other).map(|i| i != 0),
    }
}

fn parse_mode(a: &OscType) -> Option<Mode> {
    match a {
        OscType::String(s) => match s.trim().to_ascii_lowercase().as_str() {
            "fullscreen" | "vollbild" | "full" | "single" => Some(Mode::Vollbild),
            "overview" | "uebersicht" | "übersicht" | "grid" => Some(Mode::Uebersicht),
            _ => None,
        },
        other => arg_int(other).map(|i| {
            if i == 0 {
                Mode::Vollbild
            } else {
                Mode::Uebersicht
            }
        }),
    }
}

/// Maps an OSC message to a command. The leading `/pdf` is optional.
/// Trigger addresses ignore a first argument of 0, so button-release messages don't fire twice.
pub fn parse(msg: &OscMessage) -> Option<Command> {
    let addr = msg.addr.to_ascii_lowercase();
    let addr = addr.strip_prefix("/pdf").unwrap_or(&addr);
    let first = msg.args.first();
    let released = first.and_then(arg_int) == Some(0);
    let trigger = |c: Command| (!released).then_some(c);

    match addr {
        "/next" => trigger(Command::NextPage),
        "/prev" | "/previous" => trigger(Command::PrevPage),
        "/first" => trigger(Command::FirstPage),
        "/last" => trigger(Command::LastPage),
        "/page" | "/goto" => first
            .and_then(arg_int)
            .filter(|&p| p >= 1)
            .map(|p| Command::GotoPage(p as usize - 1)),

        "/mode" => match first {
            None => Some(Command::ToggleMode),
            Some(a) => parse_mode(a).map(Command::SetMode),
        },
        "/mode/toggle" => trigger(Command::ToggleMode),
        "/mode/fullscreen" | "/mode/vollbild" => trigger(Command::SetMode(Mode::Vollbild)),
        "/mode/overview" | "/mode/uebersicht" => trigger(Command::SetMode(Mode::Uebersicht)),

        "/black" => match first {
            None => Some(Command::ToggleBlack),
            Some(a) => arg_bool(a).map(Command::SetBlack),
        },
        "/black/toggle" => trigger(Command::ToggleBlack),

        "/output" => match first {
            None => Some(Command::ToggleOutput),
            Some(a) => arg_bool(a).map(Command::SetOutput),
        },
        "/output/toggle" => trigger(Command::ToggleOutput),
        "/output/monitor" => match first {
            Some(OscType::String(s))
                if !s.trim().is_empty() && !s.trim().eq_ignore_ascii_case("auto") =>
            {
                Some(Command::SetOutputMonitor(Some(s.trim().to_string())))
            }
            _ => Some(Command::SetOutputMonitor(None)),
        },

        "/file/next" => trigger(Command::NextFile),
        "/file/prev" | "/file/previous" => trigger(Command::PrevFile),
        "/file/select" | "/file/index" => first
            .and_then(arg_int)
            .filter(|&i| i >= 1)
            .map(|i| Command::SelectFile(i as usize - 1)),
        "/file/open" | "/file" => match first {
            Some(OscType::String(s)) if !s.trim().is_empty() => {
                Some(Command::OpenFile(PathBuf::from(s.trim())))
            }
            _ => None,
        },
        "/folder" => match first {
            Some(OscType::String(s)) if !s.trim().is_empty() => {
                Some(Command::SetFolder(PathBuf::from(s.trim())))
            }
            _ => None,
        },
        "/rescan" | "/file/rescan" => trigger(Command::Rescan),
        _ => None,
    }
}

/// Sends status messages to a fixed address (e.g. Companion's OSC listener).
pub struct Feedback {
    socket: UdpSocket,
    target: SocketAddr,
    last: Vec<OscMessage>,
}

#[derive(Default)]
pub struct Status {
    pub page: usize,
    pub pages: usize,
    pub file: String,
    pub file_index: usize,
    pub files: usize,
    pub mode: &'static str,
    pub black: bool,
    pub output: bool,
}

impl Feedback {
    pub fn new(target: SocketAddr) -> anyhow::Result<Self> {
        let bind: SocketAddr = if target.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        }
        .parse()?;
        Ok(Self {
            socket: UdpSocket::bind(bind)?,
            target,
            last: Vec::new(),
        })
    }

    /// Sends only the values that changed since the last call.
    pub fn send(&mut self, s: &Status) {
        let int = |v: usize| OscType::Int(v as i32);
        let msgs = vec![
            ("/pdf/status/page", int(s.page)),
            ("/pdf/status/pages", int(s.pages)),
            ("/pdf/status/file", OscType::String(s.file.clone())),
            ("/pdf/status/file_index", int(s.file_index)),
            ("/pdf/status/files", int(s.files)),
            ("/pdf/status/mode", OscType::String(s.mode.to_string())),
            ("/pdf/status/black", int(s.black as usize)),
            ("/pdf/status/output", int(s.output as usize)),
        ]
        .into_iter()
        .map(|(addr, arg)| OscMessage {
            addr: addr.to_string(),
            args: vec![arg],
        })
        .collect::<Vec<_>>();

        for (i, msg) in msgs.iter().enumerate() {
            if self.last.get(i) == Some(msg) {
                continue;
            }
            if let Ok(bytes) = rosc::encoder::encode(&OscPacket::Message(msg.clone())) {
                let _ = self.socket.send_to(&bytes, self.target);
            }
        }
        self.last = msgs;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(addr: &str, args: Vec<OscType>) -> Option<Command> {
        parse(&OscMessage {
            addr: addr.into(),
            args,
        })
    }

    #[test]
    fn parses_common_addresses() {
        assert!(matches!(msg("/pdf/next", vec![]), Some(Command::NextPage)));
        assert!(matches!(
            msg("/next", vec![OscType::Int(1)]),
            Some(Command::NextPage)
        ));
        assert!(msg("/pdf/next", vec![OscType::Int(0)]).is_none());
        assert!(matches!(
            msg("/pdf/page", vec![OscType::Int(3)]),
            Some(Command::GotoPage(2))
        ));
        assert!(matches!(
            msg("/pdf/page", vec![OscType::String("3".into())]),
            Some(Command::GotoPage(2))
        ));
        assert!(matches!(
            msg("/pdf/mode", vec![OscType::String("overview".into())]),
            Some(Command::SetMode(Mode::Uebersicht))
        ));
        assert!(matches!(
            msg("/pdf/mode/toggle", vec![]),
            Some(Command::ToggleMode)
        ));
        assert!(matches!(
            msg("/pdf/black", vec![OscType::Float(1.0)]),
            Some(Command::SetBlack(true))
        ));
        assert!(matches!(
            msg("/pdf/file/select", vec![OscType::Int(1)]),
            Some(Command::SelectFile(0))
        ));
        assert!(matches!(
            msg("/PDF/File/Open", vec![OscType::String("talk".into())]),
            Some(Command::OpenFile(_))
        ));
        assert!(msg("/pdf/unknown", vec![]).is_none());
    }
}
