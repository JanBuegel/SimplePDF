//! Presentation state: which file, which page, which mode. Independent of windows.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};

use egui::ColorImage;

use crate::command::{Command, Mode, PageLayout};
use crate::library::{self, Library};
use crate::osc::Status;
use crate::pdf::{DocInfo, Kind, Request, Response};

pub type ImageKey = (u64, usize, Kind);

/// Thumbnails are rendered to fit this box (pixels).
const THUMB_BOX: u32 = 640;
/// Full renders are kept for pages within this distance of the current page.
const KEEP_FULL: usize = 2;

pub struct Presentation {
    pub page_layout: PageLayout,
    layouts: HashMap<PathBuf, PageLayout>,
    pub library: Library,
    pub doc: Option<DocInfo>,
    pub page: usize,
    pub mode: Mode,
    pub black: bool,
    pub output_wanted: bool,
    /// Monitor name chosen for the output, `None` = automatic.
    pub output_monitor: Option<String>,
    pub global_clicker: bool,
    /// Shown in the control window footer.
    pub error: Option<String>,
    pub images: HashMap<ImageKey, Arc<ColorImage>>,

    /// Size full renders fit into (largest connected monitor).
    pub full_box: (u32, u32),
    requested: HashSet<ImageKey>,
    loading: Option<(u64, PathBuf)>,
    /// Page navigation that arrived while a document was still loading.
    deferred: Vec<Command>,
    next_doc_id: u64,
    requests: Option<Sender<Request>>,
    responses: Receiver<Response>,
}

impl Presentation {
    pub fn new(
        library: Library,
        requests: Option<Sender<Request>>,
        responses: Receiver<Response>,
    ) -> Self {
        Self {
            page_layout: PageLayout::default(),
            layouts: HashMap::new(),
            library,
            doc: None,
            page: 0,
            mode: Mode::Vollbild,
            black: false,
            output_wanted: false,
            output_monitor: None,
            global_clicker: true,
            error: None,
            images: HashMap::new(),
            full_box: (1920, 1080),
            requested: HashSet::new(),
            loading: None,
            deferred: Vec::new(),
            next_doc_id: 1,
            requests,
            responses,
        }
    }

    pub fn page_count(&self) -> usize {
        self.doc.as_ref().map_or(0, DocInfo::page_count)
    }

    pub fn current_path(&self) -> Option<&Path> {
        self.loading
            .as_ref()
            .map(|(_, p)| p.as_path())
            .or(self.doc.as_ref().map(|d| d.path.as_path()))
    }

    pub fn current_file_index(&self) -> Option<usize> {
        self.current_path().and_then(|p| self.library.index_of(p))
    }

    pub fn is_loading(&self) -> bool {
        self.loading.is_some()
    }

    pub fn open(&mut self, path: PathBuf) {
        let Some(tx) = &self.requests else { return };
        self.page_layout = self.layouts.get(&path).copied().unwrap_or_default();
        let id = self.next_doc_id;
        self.next_doc_id += 1;
        self.doc = None;
        self.page = 0;
        self.images.clear();
        self.requested.clear();
        self.error = None;
        self.deferred.clear();
        let _ = tx.send(Request::Open {
            id,
            path: path.clone(),
        });
        self.loading = Some((id, path));
    }

    pub fn apply(&mut self, cmd: Command) {
        let navigation = matches!(
            cmd,
            Command::NextPage
                | Command::PrevPage
                | Command::FirstPage
                | Command::LastPage
                | Command::GotoPage(_)
        );
        if navigation && self.loading.is_some() {
            self.deferred.push(cmd);
            return;
        }
        let count = self.page_count();
        let last = count.saturating_sub(1);
        match cmd {
            Command::SetPageLayout(layout) => {
                if let Some(path) = self.current_path().map(Path::to_path_buf) {
                    self.page_layout = layout.sanitized();
                    self.layouts.insert(path, self.page_layout);
                }
            }
            Command::NextPage => self.page = (self.page + 1).min(last),
            Command::PrevPage => self.page = self.page.saturating_sub(1),
            Command::FirstPage => self.page = 0,
            Command::LastPage => self.page = last,
            Command::GotoPage(p) => self.page = p.min(last),
            Command::SetMode(m) => self.mode = m,
            Command::ToggleMode => self.mode = self.mode.toggled(),
            Command::SetBlack(b) => self.black = b,
            Command::ToggleBlack => self.black = !self.black,
            Command::NextFile | Command::PrevFile => {
                self.library.rescan();
                let n = self.library.files.len();
                if n > 0 {
                    let forward = matches!(cmd, Command::NextFile);
                    let i = match self.current_file_index() {
                        Some(i) if forward => (i + 1) % n,
                        Some(i) => (i + n - 1) % n,
                        None if forward => 0,
                        None => n - 1,
                    };
                    self.open(self.library.files[i].clone());
                }
            }
            Command::SelectFile(i) => {
                self.library.rescan();
                match self.library.files.get(i) {
                    Some(p) => self.open(p.clone()),
                    None => self.error = Some(format!("Keine Datei Nr. {} im Ordner", i + 1)),
                }
            }
            Command::OpenFile(name) => {
                self.library.rescan();
                match self.library.resolve(&name) {
                    Some(p) => self.open(p),
                    None => self.error = Some(format!("Datei nicht gefunden: {}", name.display())),
                }
            }
            Command::SetFolder(dir) => {
                if dir.is_dir() {
                    self.library = Library::new(dir);
                } else {
                    self.error = Some(format!("Kein Ordner: {}", dir.display()));
                }
            }
            Command::Rescan => self.library.rescan(),
            Command::SetOutput(on) => self.output_wanted = on,
            Command::ToggleOutput => self.output_wanted = !self.output_wanted,
            Command::SetOutputMonitor(m) => self.output_monitor = m,
            Command::SetGlobalClicker(on) => self.global_clicker = on,
        }
    }

    /// Collects finished work from the renderer. Returns true if anything changed.
    pub fn poll_renderer(&mut self) -> bool {
        let mut changed = false;
        while let Ok(resp) = self.responses.try_recv() {
            match resp {
                Response::Opened { id, result } => {
                    if self.loading.as_ref().is_some_and(|(l, _)| *l == id) {
                        self.loading = None;
                        match result {
                            Ok(info) => {
                                self.doc = Some(info);
                                for cmd in std::mem::take(&mut self.deferred) {
                                    self.apply(cmd);
                                }
                            }
                            Err(e) => self.error = Some(e),
                        }
                        changed = true;
                    }
                }
                Response::Page {
                    doc,
                    page,
                    kind,
                    image,
                } => {
                    if self.doc.as_ref().is_some_and(|d| d.id == doc) {
                        self.images.insert((doc, page, kind), Arc::new(image));
                        changed = true;
                    }
                }
            }
        }
        changed
    }

    /// Requests renders that are needed now and drops full renders that are no longer near.
    pub fn schedule_renders(&mut self) {
        let (Some(doc), Some(tx)) = (&self.doc, &self.requests) else {
            return;
        };
        let id = doc.id;
        let count = doc.page_count();
        let keep = self.page.saturating_sub(KEEP_FULL)
            ..=(self.page + KEEP_FULL).min(count.saturating_sub(1));

        self.images
            .retain(|&(_, p, k), _| k == Kind::Thumb || keep.contains(&p));
        self.requested
            .retain(|&(_, p, k)| k == Kind::Thumb || keep.contains(&p));

        let mut send = |page: usize, kind: Kind, (max_w, max_h): (u32, u32)| {
            if page < count && self.requested.insert((id, page, kind)) {
                let _ = tx.send(Request::Render {
                    doc: id,
                    page,
                    kind,
                    max_w,
                    max_h,
                });
            }
        };
        for p in 0..count {
            send(p, Kind::Thumb, (THUMB_BOX, THUMB_BOX));
        }
        // The worker renders the most recently requested full page first, so the current page goes last.
        let p = self.page;
        for page in [p + 2, p.wrapping_sub(1), p + 1, p] {
            send(page, Kind::Full, self.full_box);
        }
    }

    pub fn status(&self) -> Status {
        Status {
            page: if self.doc.is_some() { self.page + 1 } else { 0 },
            pages: self.page_count(),
            file: self
                .current_path()
                .map(library::file_name)
                .unwrap_or_default(),
            file_index: self.current_file_index().map_or(0, |i| i + 1),
            files: self.library.files.len(),
            mode: self.mode.osc_name(),
            black: self.black,
            output: self.output_wanted,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn presentation(pages: usize) -> Presentation {
        let (_tx, rx) = mpsc::channel();
        let mut p = Presentation::new(
            Library {
                dir: ".".into(),
                files: Vec::new(),
            },
            None,
            rx,
        );
        p.doc = Some(DocInfo {
            id: 1,
            path: "talk.pdf".into(),
            page_sizes: vec![(16.0, 9.0); pages],
        });
        p
    }

    #[test]
    fn page_layout_is_remembered_per_file_and_crop_is_bounded() {
        let mut p = presentation(1);
        let (tx, _rx) = mpsc::channel();
        p.requests = Some(tx);
        p.open("first.pdf".into());
        p.apply(Command::SetPageLayout(PageLayout {
            crop: [0.1, -0.1, 0.9, f32::NAN],
            stretch: true,
        }));
        let expected = PageLayout {
            crop: [0.1, 0.0, 0.4, 0.0],
            stretch: true,
        };
        assert_eq!(p.page_layout, expected);
        p.open("second.pdf".into());
        assert_eq!(p.page_layout, PageLayout::default());
        p.open("first.pdf".into());
        assert_eq!(p.page_layout, expected);
        p.apply(Command::SetPageLayout(PageLayout::default()));
        p.open("second.pdf".into());
        p.open("first.pdf".into());
        assert_eq!(p.page_layout, PageLayout::default());
    }

    #[test]
    fn paging_stays_within_the_open_file() {
        let mut p = presentation(3);
        p.apply(Command::LastPage);
        p.apply(Command::NextPage);
        p.apply(Command::NextPage);
        assert_eq!(p.page, 2);
        assert_eq!(p.doc.as_ref().map(|d| d.id), Some(1));
        p.apply(Command::FirstPage);
        p.apply(Command::PrevPage);
        assert_eq!(p.page, 0);
        assert_eq!(p.doc.as_ref().map(|d| d.id), Some(1));
    }
}
