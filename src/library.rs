//! The folder of PDFs that can be selected by index (Companion buttons map to positions).

use std::path::{Path, PathBuf};

pub struct Library {
    pub dir: PathBuf,
    pub files: Vec<PathBuf>,
}

impl Library {
    pub fn new(dir: PathBuf) -> Self {
        // Not `canonicalize`: on Windows that yields `\\?\C:\...` paths.
        let dir = std::path::absolute(&dir).unwrap_or(dir);
        let mut lib = Self {
            dir,
            files: Vec::new(),
        };
        lib.rescan();
        lib
    }

    /// Re-reads the folder; files are sorted by name (case-insensitive).
    pub fn rescan(&mut self) {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&self.dir)
            .map(|rd| {
                rd.filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| p.is_file() && is_pdf(p))
                    .collect()
            })
            .unwrap_or_default();
        files.sort_by_key(|p| file_name(p).to_lowercase());
        self.files = files;
    }

    pub fn index_of(&self, path: &Path) -> Option<usize> {
        self.files.iter().position(|f| same_file(f, path))
    }

    /// Resolves a user/OSC supplied name: absolute path, path relative to the folder,
    /// or a case-insensitive match on the file name: exact (".pdf" optional), then prefix,
    /// then substring. The first file in sort order wins.
    pub fn resolve(&self, name: &Path) -> Option<PathBuf> {
        let candidates = [
            name.to_path_buf(),
            self.dir.join(name),
            self.dir.join(name).with_extension("pdf"),
        ];
        if let Some(p) = candidates.into_iter().find(|p| p.is_file()) {
            return Some(p);
        }
        let wanted = name.to_string_lossy().to_lowercase();
        let names: Vec<String> = self
            .files
            .iter()
            .map(|f| file_name(f).to_lowercase())
            .collect();
        let matchers: [&dyn Fn(&str) -> bool; 3] = [
            &|n| n == wanted || n.strip_suffix(".pdf") == Some(&wanted),
            &|n| n.starts_with(&wanted),
            &|n| n.contains(&wanted),
        ];
        matchers
            .iter()
            .find_map(|m| names.iter().position(|n| m(n)))
            .map(|i| self.files[i].clone())
    }
}

pub fn is_pdf(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

pub fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn same_file(a: &Path, b: &Path) -> bool {
    a == b || matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y)
}
