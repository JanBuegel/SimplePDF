//! Drawing for the control window and the output window.

use std::collections::HashMap;
use std::path::PathBuf;

use egui::{
    Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, TextureHandle, Ui, pos2, vec2,
};

use crate::command::{Command, Mode};
use crate::library;
use crate::pdf::Kind;
use crate::state::{ImageKey, Presentation};

pub type Textures = HashMap<ImageKey, TextureHandle>;

const ACCENT: Color32 = Color32::from_rgb(255, 170, 0);

/// Uploads new page images into this window's egui context and frees stale ones.
pub fn sync_textures(ctx: &egui::Context, pres: &Presentation, textures: &mut Textures) {
    textures.retain(|k, _| pres.images.contains_key(k));
    for (key, image) in &pres.images {
        textures.entry(*key).or_insert_with(|| {
            let name = format!("page-{}-{}-{:?}", key.0, key.1, key.2);
            ctx.load_texture(name, (**image).clone(), egui::TextureOptions::LINEAR)
        });
    }
}

/// Largest rect with `aspect` (w/h) centered in `outer`.
fn fit(outer: Rect, aspect: f32) -> Rect {
    let size = if outer.width() / outer.height() > aspect {
        vec2(outer.height() * aspect, outer.height())
    } else {
        vec2(outer.width(), outer.width() / aspect)
    };
    Rect::from_center_size(outer.center(), size)
}

/// Number of columns that makes `n` pages of `aspect` as large as possible inside `rect`.
fn grid_columns(n: usize, aspect: f32, rect: Rect, gap: f32) -> usize {
    (1..=n.max(1))
        .max_by(|&a, &b| {
            let size = |cols: usize| {
                let rows = n.div_ceil(cols);
                let w = (rect.width() - gap * (cols + 1) as f32) / cols as f32;
                let h = (rect.height() - gap * (rows + 1) as f32) / rows as f32;
                w.min(h * aspect)
            };
            size(a).total_cmp(&size(b))
        })
        .unwrap_or(1)
}

fn page_texture<'a>(
    pres: &Presentation,
    tex: &'a Textures,
    page: usize,
    prefer_full: bool,
) -> Option<&'a TextureHandle> {
    let id = pres.doc.as_ref()?.id;
    let full = tex.get(&(id, page, Kind::Full));
    let thumb = tex.get(&(id, page, Kind::Thumb));
    if prefer_full {
        full.or(thumb)
    } else {
        thumb.or(full)
    }
}

fn draw_page(
    ui: &Ui,
    rect: Rect,
    pres: &Presentation,
    tex: &Textures,
    page: usize,
    prefer_full: bool,
) -> Rect {
    let aspect = pres.doc.as_ref().map_or(16.0 / 9.0, |d| d.aspect(page));
    let [left, right, top, bottom] = pres.page_layout.crop;
    let uv = Rect::from_min_max(pos2(left, top), pos2(1.0 - right, 1.0 - bottom));
    let aspect = aspect * uv.width() / uv.height();
    let r = if prefer_full && pres.page_layout.stretch {
        rect
    } else {
        fit(rect, aspect)
    };
    match page_texture(pres, tex, page, prefer_full) {
        Some(t) => {
            ui.painter().image(t.id(), r, uv, Color32::WHITE);
        }
        None => {
            ui.painter().rect_filled(r, 0.0, Color32::from_gray(40));
        }
    }
    r
}

/// What the audience sees. Returns a page that was clicked (only in overview mode with `clickable`).
pub fn output_view(
    ui: &mut Ui,
    rect: Rect,
    pres: &Presentation,
    tex: &Textures,
    clickable: bool,
) -> Option<usize> {
    ui.painter().rect_filled(rect, 0.0, Color32::BLACK);
    let doc = pres.doc.as_ref()?;
    if pres.black || doc.page_count() == 0 {
        return None;
    }
    match pres.mode {
        Mode::Vollbild => {
            draw_page(ui, rect, pres, tex, pres.page, true);
            None
        }
        Mode::Uebersicht => {
            let n = doc.page_count();
            let gap = (rect.width().min(rect.height()) * 0.015).max(2.0);
            let aspect = doc.aspect(0);
            let cols = grid_columns(n, aspect, rect, gap);
            let rows = n.div_ceil(cols);
            let cell = vec2(
                (rect.width() - gap * (cols + 1) as f32) / cols as f32,
                (rect.height() - gap * (rows + 1) as f32) / rows as f32,
            );
            // Center the grid as a whole.
            let used = fit(Rect::from_min_size(rect.min, cell), aspect).size();
            let grid = vec2(
                used.x * cols as f32 + gap * (cols - 1) as f32,
                used.y * rows as f32 + gap * (rows - 1) as f32,
            );
            let origin = rect.center() - grid / 2.0;
            let mut clicked = None;
            for page in 0..n {
                let (c, r) = (page % cols, page / cols);
                let cell_rect = Rect::from_min_size(
                    origin + vec2(c as f32 * (used.x + gap), r as f32 * (used.y + gap)),
                    used,
                );
                let drawn = draw_page(ui, cell_rect, pres, tex, page, false);
                if page == pres.page {
                    let w = (gap * 0.5).max(2.0);
                    ui.painter().rect_stroke(
                        drawn.expand(w * 0.5),
                        0.0,
                        Stroke::new(w, ACCENT),
                        StrokeKind::Outside,
                    );
                }
                if clickable
                    && ui
                        .interact(drawn, ui.scope_id().with(("ov", page)), Sense::click())
                        .clicked()
                {
                    clicked = Some(page);
                }
            }
            clicked
        }
    }
}

/// Persistent widget state of the control window.
#[derive(Default)]
pub struct ControlUi {
    pub folder_edit: Option<String>,
    /// Last (doc, page) the thumbnail strip was scrolled to.
    scrolled_to: Option<(u64, usize)>,
}

pub struct ControlInfo<'a> {
    pub monitors: &'a [String],
    pub output_on: Option<&'a str>,
    pub osc: &'a str,
    /// System-wide clicker keys: (active, registered key count); `None` if unsupported.
    pub clicker: Option<(bool, usize)>,
    /// Width / height of the output display, for the preview box.
    pub output_aspect: f32,
}

pub fn control(
    ui: &mut Ui,
    pres: &Presentation,
    tex: &Textures,
    cui: &mut ControlUi,
    info: &ControlInfo,
    cmds: &mut Vec<Command>,
) {
    egui::Panel::top("toolbar").show(ui, |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Modus:");
            for m in [Mode::Vollbild, Mode::Uebersicht] {
                if ui.selectable_label(pres.mode == m, m.label()).clicked() {
                    cmds.push(Command::SetMode(m));
                }
            }
            ui.separator();
            let black = egui::Button::new("Schwarz (B)").selected(pres.black);
            if ui.add(black).clicked() {
                cmds.push(Command::ToggleBlack);
            }
            ui.separator();
            ui.label("Ausgabe auf:");
            let current = pres
                .output_monitor
                .clone()
                .unwrap_or_else(|| "Automatisch".into());
            egui::ComboBox::from_id_salt("monitor")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    let mut choice = pres.output_monitor.clone();
                    ui.selectable_value(&mut choice, None, "Automatisch");
                    for m in info.monitors {
                        ui.selectable_value(&mut choice, Some(m.clone()), m);
                    }
                    if choice != pres.output_monitor {
                        cmds.push(Command::SetOutputMonitor(choice));
                    }
                });
            let label = if pres.output_wanted {
                "Ausgabe beenden (Esc)"
            } else {
                "Ausgabe starten (F5)"
            };
            if ui
                .add(egui::Button::new(label).selected(pres.output_wanted))
                .clicked()
            {
                cmds.push(Command::ToggleOutput);
            }
            if let Some(m) = info.output_on {
                ui.weak(format!("aktiv auf {m}"));
            }
            if let Some((active, registered)) = info.clicker {
                ui.separator();
                let mut global = pres.global_clicker;
                let hint = "Bild↑/Bild↓ und ←/→ steuern die Seiten auch ohne Fokus, solange die \
                            Ausgabe läuft. Diese Tasten fehlen dann in anderen Programmen.";
                if ui
                    .checkbox(&mut global, "Clicker global")
                    .on_hover_text(hint)
                    .changed()
                {
                    cmds.push(Command::SetGlobalClicker(global));
                }
                if active && registered < 4 {
                    ui.colored_label(
                        Color32::LIGHT_RED,
                        format!("nur {registered}/4 Tasten frei"),
                    );
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.weak(info.osc)
            });
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui(pres.current_path().is_some(), |ui| {
                ui.label("Bildanpassung (diese Datei):");
                let mut layout = pres.page_layout;
                ui.checkbox(&mut layout.stretch, "Auf Ausgabe strecken")
                    .on_hover_text(
                        "Füllt die Ausgabe vollständig. Das Seitenverhältnis kann sich ändern.",
                    );
                ui.menu_button("Ränder beschneiden …", |ui| {
                    for (edge, label) in layout
                        .crop
                        .iter_mut()
                        .zip(["Links", "Rechts", "Oben", "Unten"])
                    {
                        let mut percent = *edge * 100.0;
                        ui.add(
                            egui::Slider::new(&mut percent, 0.0..=40.0)
                                .suffix(" %")
                                .text(label),
                        );
                        *edge = percent / 100.0;
                    }
                    if ui.button("A4 quer → 16:9").clicked() {
                        layout.crop = [0.0, 0.0, 0.1025, 0.1025];
                    }
                });
                if ui.button("Zurücksetzen").clicked() {
                    layout = Default::default();
                }
                if layout != pres.page_layout {
                    cmds.push(Command::SetPageLayout(layout));
                }
                ui.weak("Gilt für alle Seiten · für diese Sitzung gemerkt");
            });
        });
        ui.add_space(4.0);
    });

    egui::Panel::bottom("status").show(ui, |ui| {
        ui.horizontal(|ui| {
            if pres.doc.is_some() {
                ui.strong(format!("Seite {} / {}", pres.page + 1, pres.page_count()));
            } else if pres.is_loading() {
                ui.spinner();
                ui.label("Lade …");
            }
            if let Some(p) = pres.current_path() {
                ui.label(library::file_name(p));
            }
            if let Some(e) = &pres.error {
                ui.colored_label(Color32::LIGHT_RED, e);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.weak("Pfeiltasten Seite · O Modus · B Schwarz · F5/Esc Ausgabe · [ ] Datei");
            });
        });
    });

    egui::Panel::left("files")
        .resizable(true)
        .default_size(260.0)
        .show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.strong("Ordner");
                if ui.small_button("Neu einlesen").clicked() {
                    cmds.push(Command::Rescan);
                }
            });
            let text = cui
                .folder_edit
                .get_or_insert_with(|| pres.library.dir.display().to_string());
            let resp = ui.add(egui::TextEdit::singleline(text).desired_width(f32::INFINITY));
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                cmds.push(Command::SetFolder(PathBuf::from(text.trim())));
            }
            if !resp.has_focus() {
                cui.folder_edit = None;
            }
            ui.weak("PDF oder Ordner auch per Drag & Drop");
            ui.separator();
            let current = pres.current_file_index();
            egui::ScrollArea::vertical().show(ui, |ui| {
                if pres.library.files.is_empty() {
                    ui.weak("Keine PDFs im Ordner");
                }
                for (i, f) in pres.library.files.iter().enumerate() {
                    let label = format!("{:>2}. {}", i + 1, library::file_name(f));
                    if ui.selectable_label(current == Some(i), label).clicked() {
                        cmds.push(Command::SelectFile(i));
                    }
                }
            });
        });

    egui::CentralPanel::default().show(ui, |ui| {
        let full = ui.available_rect_before_wrap();
        let top_h = (full.height() * 0.58).max(200.0);
        let top = Rect::from_min_size(full.min, vec2(full.width(), top_h));
        let main_w = top.width() * 0.66;
        let label_h = 20.0;
        // The preview has the shape of the output display, whatever the window size.
        let preview_size = fit(
            Rect::from_min_size(top.min, vec2(main_w - 8.0, top_h - label_h)),
            info.output_aspect,
        )
        .size();
        let font = FontId::proportional(14.0);
        let text_col = ui.visuals().strong_text_color();

        // Live preview of the output.
        let main = Rect::from_min_size(top.min, preview_size + vec2(0.0, label_h));
        let mode = if pres.black {
            "SCHWARZ".to_string()
        } else {
            format!("Ausgabe · {}", pres.mode.label())
        };
        ui.painter().text(
            main.left_top(),
            Align2::LEFT_TOP,
            mode,
            font.clone(),
            if pres.black { ACCENT } else { text_col },
        );
        let preview = Rect::from_min_max(main.min + vec2(0.0, label_h), main.max);
        if let Some(p) = output_view(ui, preview, pres, tex, true) {
            cmds.push(Command::GotoPage(p));
        }
        if pres.black {
            ui.painter()
                .rect_stroke(preview, 0.0, Stroke::new(2.0, ACCENT), StrokeKind::Inside);
        }

        // Next page.
        let side = Rect::from_min_max(pos2(main.max.x + 16.0, top.min.y), top.max);
        if pres.doc.is_some() {
            let next = pres.page + 1;
            let txt = if next < pres.page_count() {
                format!("Nächste: Seite {}", next + 1)
            } else {
                "Ende".into()
            };
            ui.painter()
                .text(side.left_top(), Align2::LEFT_TOP, txt, font, text_col);
            if next < pres.page_count() {
                let r = Rect::from_min_size(
                    side.min + vec2(0.0, label_h),
                    vec2(side.width(), preview_size.y * 0.6),
                );
                draw_page(ui, fit(r, info.output_aspect), pres, tex, next, true);
            }
        }

        // All pages.
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(Rect::from_min_max(
                pos2(full.min.x, top.max.y + 10.0),
                full.max,
            )),
            |ui| {
                ui.separator();
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        thumbnails(ui, pres, tex, &mut cui.scrolled_to, cmds);
                    });
            },
        );
    });
}

fn thumbnails(
    ui: &mut Ui,
    pres: &Presentation,
    tex: &Textures,
    scrolled_to: &mut Option<(u64, usize)>,
    cmds: &mut Vec<Command>,
) {
    let Some(doc) = &pres.doc else { return };
    let scroll = *scrolled_to != Some((doc.id, pres.page));
    *scrolled_to = Some((doc.id, pres.page));
    let thumb_w = 170.0;
    let thumb = vec2(thumb_w, thumb_w / doc.aspect(0) + 18.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
        for page in 0..doc.page_count() {
            let (rect, resp) = ui.allocate_exact_size(thumb, Sense::click());
            let img = Rect::from_min_max(rect.min, rect.max - vec2(0.0, 18.0));
            let drawn = draw_page(ui, img, pres, tex, page, false);
            let current = page == pres.page;
            if current {
                ui.painter()
                    .rect_stroke(drawn, 0.0, Stroke::new(3.0, ACCENT), StrokeKind::Outside);
                if scroll {
                    resp.scroll_to_me(None);
                }
            } else if resp.hovered() {
                ui.painter().rect_stroke(
                    drawn,
                    0.0,
                    Stroke::new(1.0, Color32::GRAY),
                    StrokeKind::Outside,
                );
            }
            ui.painter().text(
                pos2(rect.center().x, rect.max.y),
                Align2::CENTER_BOTTOM,
                (page + 1).to_string(),
                FontId::proportional(13.0),
                if current {
                    ACCENT
                } else {
                    ui.visuals().text_color()
                },
            );
            if resp.clicked() {
                cmds.push(Command::GotoPage(page));
            }
        }
    });
}
