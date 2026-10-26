//! PDF rendering on a dedicated worker thread (Pdfium is not cheap to share).

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::thread;

use egui::ColorImage;
use pdfium_render::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Thumb,
    Full,
}

#[derive(Clone, Debug)]
pub struct DocInfo {
    pub id: u64,
    pub path: PathBuf,
    /// Page sizes in PDF points (width, height).
    pub page_sizes: Vec<(f32, f32)>,
}

impl DocInfo {
    pub fn page_count(&self) -> usize {
        self.page_sizes.len()
    }

    pub fn aspect(&self, page: usize) -> f32 {
        let (w, h) = self.page_sizes.get(page).copied().unwrap_or((16.0, 9.0));
        if h > 0.0 { w / h } else { 16.0 / 9.0 }
    }
}

pub enum Request {
    Open {
        id: u64,
        path: PathBuf,
    },
    Render {
        doc: u64,
        page: usize,
        kind: Kind,
        max_w: u32,
        max_h: u32,
    },
}

pub enum Response {
    Opened {
        id: u64,
        result: Result<DocInfo, String>,
    },
    Page {
        doc: u64,
        page: usize,
        kind: Kind,
        image: ColorImage,
    },
}

/// Spawns the render thread. `wake` is called after every response so the UI can repaint.
pub fn spawn(
    requests: Receiver<Request>,
    responses: Sender<Response>,
    wake: impl Fn() + Send + 'static,
) -> anyhow::Result<()> {
    let bindings = bind_pdfium()?;
    thread::Builder::new()
        .name("pdf-render".into())
        .spawn(move || {
            let pdfium = Pdfium::new(bindings);
            worker(&pdfium, requests, responses, wake);
        })?;
    Ok(())
}

/// Looks for the Pdfium library next to the executable, in `./pdfium/lib`, `./pdfium/bin`,
/// the working directory and finally the system search path.
fn bind_pdfium() -> anyhow::Result<Box<dyn PdfiumLibraryBindings>> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        dirs.push(exe_dir);
    }
    for d in ["pdfium/bin", "pdfium/lib", "."] {
        dirs.push(PathBuf::from(d));
    }
    for dir in &dirs {
        let lib = Pdfium::pdfium_platform_library_name_at_path(dir);
        if lib.exists()
            && let Ok(b) = Pdfium::bind_to_library(&lib)
        {
            return Ok(b);
        }
    }
    Pdfium::bind_to_system_library().map_err(|e| {
        anyhow::anyhow!(
            "Pdfium library not found ({e}). Put {} next to the executable (see scripts/fetch-pdfium.*)",
            Pdfium::pdfium_platform_library_name().to_string_lossy()
        )
    })
}

struct Job {
    page: usize,
    kind: Kind,
    max_w: u32,
    max_h: u32,
}

fn worker(
    pdfium: &Pdfium,
    requests: Receiver<Request>,
    responses: Sender<Response>,
    wake: impl Fn(),
) {
    let mut doc: Option<(u64, PdfDocument)> = None;
    let mut pending: Vec<Job> = Vec::new();

    loop {
        // Block only when there is nothing to do, otherwise just drain the queue.
        let first = if pending.is_empty() {
            match requests.recv() {
                Ok(r) => Some(r),
                Err(_) => return,
            }
        } else {
            None
        };
        let mut incoming: Vec<Request> = first.into_iter().collect();
        loop {
            match requests.try_recv() {
                Ok(r) => incoming.push(r),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }

        for req in incoming {
            match req {
                Request::Open { id, path } => {
                    pending.clear();
                    doc = None;
                    let result = pdfium
                        .load_pdf_from_file(&path, None)
                        .map_err(|e| format!("{}: {e}", path.display()))
                        .map(|d| {
                            let sizes = d
                                .pages()
                                .iter()
                                .map(|p| (p.width().value, p.height().value))
                                .collect::<Vec<_>>();
                            doc = Some((id, d));
                            DocInfo {
                                id,
                                path: path.clone(),
                                page_sizes: sizes,
                            }
                        });
                    let _ = responses.send(Response::Opened { id, result });
                    wake();
                }
                Request::Render {
                    doc: id,
                    page,
                    kind,
                    max_w,
                    max_h,
                } => {
                    if doc.as_ref().is_some_and(|(d, _)| *d == id)
                        && !pending.iter().any(|j| j.page == page && j.kind == kind)
                    {
                        pending.push(Job {
                            page,
                            kind,
                            max_w,
                            max_h,
                        });
                    }
                }
            }
        }

        // Full renders win over thumbnails; the most recently requested full render goes first
        // (that's the page the presenter just jumped to). Thumbnails render in order.
        let next = match pending.iter().rposition(|j| j.kind == Kind::Full) {
            Some(i) => Some(pending.remove(i)),
            None if !pending.is_empty() => Some(pending.remove(0)),
            None => None,
        };
        let (Some(job), Some((id, d))) = (next, doc.as_ref()) else {
            continue;
        };
        if let Some(image) = render(d, &job) {
            let _ = responses.send(Response::Page {
                doc: *id,
                page: job.page,
                kind: job.kind,
                image,
            });
            wake();
        }
    }
}

fn render(doc: &PdfDocument, job: &Job) -> Option<ColorImage> {
    let page = doc.pages().get(job.page as PdfPageIndex).ok()?;
    let (w, h) = (page.width().value.max(1.0), page.height().value.max(1.0));
    let scale = (job.max_w as f32 / w).min(job.max_h as f32 / h);
    let config = PdfRenderConfig::new()
        .set_target_width(((w * scale).round() as Pixels).max(1))
        .render_form_data(true);
    let bitmap = page.render_with_config(&config).ok()?;
    let size = [bitmap.width() as usize, bitmap.height() as usize];
    Some(ColorImage::from_rgba_unmultiplied(
        size,
        &bitmap.as_rgba_bytes(),
    ))
}
