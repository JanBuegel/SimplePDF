use std::path::PathBuf;

/// Fractions of the original page to remove; each edge is limited to 40%.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PageLayout {
    pub crop: [f32; 4], // left, right, top, bottom
    pub stretch: bool,
}

impl PageLayout {
    pub fn sanitized(mut self) -> Self {
        for edge in &mut self.crop {
            *edge = if edge.is_finite() {
                edge.clamp(0.0, 0.4)
            } else {
                0.0
            };
        }
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Single page, filling the screen.
    Vollbild,
    /// Grid of all pages.
    Uebersicht,
}

impl Mode {
    pub fn toggled(self) -> Self {
        match self {
            Mode::Vollbild => Mode::Uebersicht,
            Mode::Uebersicht => Mode::Vollbild,
        }
    }

    pub fn osc_name(self) -> &'static str {
        match self {
            Mode::Vollbild => "fullscreen",
            Mode::Uebersicht => "overview",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Vollbild => "Vollbild",
            Mode::Uebersicht => "Übersicht",
        }
    }
}

/// Everything that can change the presentation, from keyboard, UI or OSC.
#[derive(Clone, Debug)]
pub enum Command {
    SetPageLayout(PageLayout),
    NextPage,
    PrevPage,
    FirstPage,
    LastPage,
    /// 0-based.
    GotoPage(usize),
    SetMode(Mode),
    ToggleMode,
    SetBlack(bool),
    ToggleBlack,
    NextFile,
    PrevFile,
    /// 0-based index into the folder listing.
    SelectFile(usize),
    /// Absolute path, or a name relative to the folder (".pdf" may be omitted).
    OpenFile(PathBuf),
    SetFolder(PathBuf),
    Rescan,
    SetOutput(bool),
    /// Monitor name for the output window, `None` = automatic.
    SetOutputMonitor(Option<String>),
    /// Presenter keys work system-wide while the output is shown (Windows).
    SetGlobalClicker(bool),
    ToggleOutput,
}

pub enum UserEvent {
    Command(Command),
    /// The render worker has produced something.
    PdfReady,
    /// Wake the native event loop for an egui repaint, including delayed requests.
    Repaint(winit::window::WindowId, std::time::Instant, u64),
}
