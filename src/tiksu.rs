//! Design system for egui: the IT-Veljekset look from ProjectHub (`IT-Veljekset Group/DESIGN.md`).
//! Neutral near-black surfaces, 1px borders instead of shadows, one blue accent, colour only as meaning.
//! Scales are fixed: radius 6/10/14, font 11/12/13/15/20, control heights 28/32/40.
//! Dark theme only; the tokens are the single place to add a light one.

use eframe::egui::{
    self, vec2, Align, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Frame, Layout, Margin,
    Rect, Response, RichText, Sense, Shadow, Stroke, TextStyle, Ui, WidgetInfo, WidgetText, WidgetType,
};

// --- Tokens (DESIGN.md 2.1) ---
pub const BG: Color32 = Color32::from_rgb(13, 15, 18);
pub const SURFACE: Color32 = Color32::from_rgb(20, 23, 28);
pub const SURFACE_2: Color32 = Color32::from_rgb(26, 30, 36);
pub const LINE: Color32 = Color32::from_rgb(36, 41, 47);
pub const LINE_STRONG: Color32 = Color32::from_rgb(51, 58, 67);
pub const TEXT: Color32 = Color32::from_rgb(230, 232, 235);
pub const MUTED: Color32 = Color32::from_rgb(154, 163, 173);
pub const SUBTLE: Color32 = Color32::from_rgb(107, 116, 126);
pub const ACCENT: Color32 = Color32::from_rgb(59, 130, 246);
pub const ACCENT_FG: Color32 = Color32::WHITE;
pub const SUCCESS: Color32 = Color32::from_rgb(34, 197, 94);
pub const WARN: Color32 = Color32::from_rgb(245, 158, 11);
pub const DANGER: Color32 = Color32::from_rgb(239, 68, 68);

// --- Scales (DESIGN.md 2.2) ---
pub const R_SM: u8 = 6;
pub const R_MD: u8 = 10;
pub const R_LG: u8 = 14;
pub const SIDEBAR_W: f32 = 240.0;
pub const TOPBAR_H: f32 = 40.0;
pub const PAGE_PAD: i8 = 24;
/// Settings pages read best in one column; list pages use the full width.
pub const READ_W: f32 = 880.0;

// --- Fonts: Segoe UI from Windows (as ProjectHub), Instrument Sans bundled as fallback ---
const SEMIBOLD: &str = "tk-semibold";

pub fn font(size: f32) -> FontId {
    FontId::proportional(size)
}
pub fn semibold_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}
pub fn mono_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

fn system_font(file: &str) -> Option<Vec<u8>> {
    let windir = std::env::var_os("WINDIR").map(std::path::PathBuf::from)?;
    std::fs::read(windir.join("Fonts").join(file)).ok()
}

/// Installs fonts, text styles and widget visuals. Call once at startup.
pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let mut add = |name: &str, bytes: Vec<u8>| {
        fonts.font_data.insert(name.into(), FontData::from_owned(bytes).into());
    };
    add("instrument-regular", include_bytes!("../assets/fonts/InstrumentSans-Regular.ttf").to_vec());
    add("instrument-semibold", include_bytes!("../assets/fonts/InstrumentSans-SemiBold.ttf").to_vec());
    add("jetbrains-medium", include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf").to_vec());
    let segoe = system_font("segoeui.ttf").zip(system_font("seguisb.ttf"));
    let (regular, semibold) = match segoe {
        Some((regular, semibold)) => {
            add("segoe", regular);
            add("segoe-semibold", semibold);
            ("segoe", "segoe-semibold")
        }
        None => ("instrument-regular", "instrument-semibold"),
    };
    // egui's bundled fonts stay as fallbacks (symbols, emoji).
    let fallback = fonts.families[&FontFamily::Proportional].clone();
    let family = |first: &str| [vec![first.to_string()], fallback.clone()].concat();
    fonts.families.insert(FontFamily::Proportional, family(regular));
    fonts.families.insert(FontFamily::Name(SEMIBOLD.into()), family(semibold));
    fonts.families.get_mut(&FontFamily::Monospace).unwrap().insert(0, "jetbrains-medium".into());
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    fonts.families.get_mut(&FontFamily::Name(SEMIBOLD.into())).unwrap().push("phosphor".into());
    ctx.set_fonts(fonts);

    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, semibold_font(20.0)),
            (TextStyle::Body, font(13.0)),
            (TextStyle::Button, semibold_font(13.0)),
            (TextStyle::Small, font(12.0)),
            (TextStyle::Monospace, mono_font(12.0)),
        ]
        .into();
        style.spacing.item_spacing = vec2(8.0, 8.0);
        style.spacing.button_padding = vec2(10.0, 5.0);
        style.spacing.interact_size.y = 32.0;
        style.spacing.combo_height = 320.0;

        let v = &mut style.visuals;
        *v = egui::Visuals::dark();
        v.panel_fill = BG;
        v.window_fill = SURFACE;
        v.extreme_bg_color = SURFACE_2;
        v.faint_bg_color = SURFACE;
        v.code_bg_color = SURFACE_2;
        v.hyperlink_color = ACCENT;
        v.warn_fg_color = WARN;
        v.error_fg_color = DANGER;
        v.window_stroke = Stroke::new(1.0, LINE_STRONG);
        v.window_corner_radius = CornerRadius::same(R_LG);
        v.menu_corner_radius = CornerRadius::same(R_LG);
        // The one shadow: floating things only (DESIGN.md: 0 12px 32px rgb(0 0 0 / .28)).
        v.window_shadow = Shadow { offset: [0, 12], blur: 32, spread: 0, color: Color32::from_black_alpha(71) };
        v.popup_shadow = v.window_shadow;
        v.selection.bg_fill = ACCENT.gamma_multiply(0.20);
        v.selection.stroke = Stroke::new(1.0, ACCENT);
        v.slider_trailing_fill = true;

        let w = &mut v.widgets;
        w.noninteractive.bg_fill = SURFACE;
        w.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
        w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        // Fields and selects: surface-2, hover lifts the border, focus is the accent (`.field`).
        for (state, fill, stroke) in [
            (&mut w.inactive, SURFACE_2, LINE),
            (&mut w.hovered, SURFACE_2, LINE_STRONG),
            (&mut w.active, SURFACE_2, ACCENT),
            (&mut w.open, SURFACE_2, ACCENT),
        ] {
            state.bg_fill = fill;
            state.weak_bg_fill = fill;
            state.bg_stroke = Stroke::new(1.0, stroke);
            state.fg_stroke = Stroke::new(1.0, TEXT);
            state.corner_radius = CornerRadius::same(R_MD);
            state.expansion = 0.0;
        }
    });
}

// --- Text ---

/// 13 / text.
pub fn body(text: impl Into<String>) -> RichText {
    RichText::new(text).size(13.0).color(TEXT)
}
/// 12 / muted: explanations under a label.
pub fn hint(text: impl Into<String>) -> RichText {
    RichText::new(text).size(12.0).color(MUTED)
}
/// 11 / subtle: metadata.
pub fn meta(text: impl Into<String>) -> RichText {
    RichText::new(text).size(11.0).color(SUBTLE)
}
/// Semibold, for titles and numbers only (DESIGN.md: 600 is for headings and figures).
pub fn strong(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(semibold_font(size)).color(TEXT)
}
pub fn mono(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(mono_font(size))
}

/// A muted paragraph that wraps.
pub fn note(ui: &mut Ui, text: impl Into<String>) {
    ui.add(egui::Label::new(hint(text)).wrap());
}

fn focus_ring(ui: &Ui, response: &Response, radius: u8) {
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.expand(1.0),
            CornerRadius::same(radius.saturating_add(1)),
            Stroke::new(2.0, ACCENT),
            egui::StrokeKind::Outside,
        );
    }
}

fn text_galley(ui: &Ui, text: &str, font: FontId) -> std::sync::Arc<egui::Galley> {
    WidgetText::from(RichText::new(text).font(font)).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        TextStyle::Button,
    )
}

// --- Buttons (`.btn`) ---

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Primary,
    Secondary,
    Ghost,
    Danger,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Size {
    /// 28 px (`.btn-sm`)
    Sm,
    /// 32 px
    Md,
}

/// Hover changes colour only: no lift, no scale, no glow.
pub fn button(ui: &mut Ui, kind: Kind, size: Size, text: &str, enabled: bool) -> Response {
    let (height, pad, font) = match size {
        Size::Sm => (28.0, 10.0, semibold_font(12.0)),
        Size::Md => (32.0, 12.0, semibold_font(13.0)),
    };
    let galley = text_galley(ui, text, font);
    let (rect, response) = ui.allocate_exact_size(vec2(galley.size().x + 2.0 * pad, height), if enabled { Sense::click() } else { Sense::hover() });
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, text));

    if ui.is_rect_visible(rect) {
        let hovered = enabled && response.hovered();
        let alpha = if enabled { 1.0 } else { 0.4 };
        let (fill, stroke, color) = match (kind, hovered) {
            (Kind::Primary, false) => (ACCENT, Color32::TRANSPARENT, ACCENT_FG),
            (Kind::Primary, true) => (ACCENT.lerp_to_gamma(Color32::WHITE, 0.12), Color32::TRANSPARENT, ACCENT_FG),
            (Kind::Secondary, false) => (SURFACE_2, LINE, TEXT),
            (Kind::Secondary, true) => (LINE, LINE_STRONG, TEXT),
            (Kind::Ghost, false) => (Color32::TRANSPARENT, Color32::TRANSPARENT, MUTED),
            (Kind::Ghost, true) => (SURFACE_2, LINE, TEXT),
            (Kind::Danger, false) => (DANGER.gamma_multiply(0.10), DANGER.gamma_multiply(0.30), DANGER),
            (Kind::Danger, true) => (DANGER.gamma_multiply(0.20), DANGER.gamma_multiply(0.30), DANGER),
        };
        let p = ui.painter();
        p.rect(rect, CornerRadius::same(R_MD), fill.gamma_multiply(alpha), Stroke::new(1.0, stroke.gamma_multiply(alpha)), egui::StrokeKind::Inside);
        p.galley(rect.center() - galley.size() / 2.0, galley, color.gamma_multiply(alpha));
        focus_ring(ui, &response, R_MD);
    }
    if enabled { response.on_hover_cursor(egui::CursorIcon::PointingHand) } else { response }
}

/// Choice among a few options as chips (`.chip`); the selected one is tinted with the accent.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        for (option, label) in options {
            let selected = *value == *option;
            let galley = text_galley(ui, label, semibold_font(12.0));
            let (rect, response) = ui.allocate_exact_size(vec2(galley.size().x + 22.0, 28.0), Sense::click());
            response.widget_info(|| WidgetInfo::selected(WidgetType::RadioButton, true, selected, *label));
            let (fill, stroke, color) = if selected {
                (ACCENT.gamma_multiply(0.10), ACCENT.gamma_multiply(0.40), ACCENT)
            } else if response.hovered() {
                (SURFACE_2, LINE_STRONG, TEXT)
            } else {
                (SURFACE_2, LINE, MUTED)
            };
            let p = ui.painter();
            p.rect(rect, CornerRadius::same(255), fill, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
            p.galley(rect.center() - galley.size() / 2.0, galley, color);
            focus_ring(ui, &response, 14);
            if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !selected {
                *value = *option;
                changed = true;
            }
        }
    });
    changed
}

/// 36x20 switch.
pub fn switch(ui: &mut Ui, on: &mut bool) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(vec2(36.0, 20.0), Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    response.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, true, *on, ""));
    let t = ui.ctx().animate_bool_with_time(response.id, *on, 0.12);
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(10), if *on { ACCENT } else { LINE_STRONG });
    p.circle_filled(egui::pos2(rect.left() + 10.0 + t * 16.0, rect.center().y), 7.0, Color32::WHITE);
    focus_ring(ui, &response, 10);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// 4px rail, accent fill up to the value, white knob.
pub fn slider(ui: &mut Ui, value: &mut u32, range: std::ops::RangeInclusive<u32>, step: u32) -> Response {
    let (lo, hi) = (*range.start(), *range.end());
    let width = (ui.available_width() - 90.0).clamp(140.0, 360.0); // leaves room for the value label
    let (rect, mut response) = ui.allocate_exact_size(vec2(width, 24.0), Sense::click_and_drag());
    let rail = Rect::from_center_size(rect.center(), vec2(rect.width() - 16.0, 4.0));

    let mut new = *value;
    if let Some(pos) = response.interact_pointer_pos() {
        let t = ((pos.x - rail.left()) / rail.width()).clamp(0.0, 1.0);
        new = lo + ((t * (hi - lo) as f32 / step as f32).round() as u32 * step);
    }
    if response.has_focus() {
        ui.input(|i| {
            if i.key_pressed(egui::Key::ArrowLeft) {
                new = new.saturating_sub(step).max(lo);
            }
            if i.key_pressed(egui::Key::ArrowRight) {
                new = (new + step).min(hi);
            }
        });
    }
    if new != *value {
        *value = new.clamp(lo, hi);
        response.mark_changed();
    }
    response.widget_info(|| WidgetInfo::slider(true, *value as f64, ""));

    let t = (*value - lo) as f32 / (hi - lo) as f32;
    let x = rail.left() + t * rail.width();
    let p = ui.painter();
    p.rect_filled(rail, CornerRadius::same(2), LINE_STRONG);
    p.rect_filled(Rect::from_min_max(rail.min, egui::pos2(x, rail.max.y)), CornerRadius::same(2), ACCENT);
    let knob = egui::pos2(x, rail.center().y);
    p.circle_filled(knob, 8.0, Color32::WHITE);
    if response.has_focus() {
        p.circle_stroke(knob, 11.0, Stroke::new(2.0, ACCENT));
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

// --- Page frame ---

/// `.page-head`: title and one line of explanation on the left, the primary action on the right.
pub fn page_head(ui: &mut Ui, title: &str, desc: &str, actions: impl FnOnce(&mut Ui)) {
    let r = Frame::new().inner_margin(Margin::symmetric(PAGE_PAD, 16)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_max_width((ui.available_width() - 280.0).max(240.0));
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(strong(title, 20.0));
                if !desc.is_empty() {
                    ui.add(egui::Label::new(RichText::new(desc).size(13.0).color(MUTED)).wrap());
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), actions);
        });
    });
    let rect = r.response.rect;
    ui.painter().hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, LINE));
}

/// `.page-body`: the page's only scroll area. `read` limits the width for settings pages.
pub fn content(ui: &mut Ui, read: bool, add: impl FnOnce(&mut Ui)) {
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        Frame::new().inner_margin(Margin { left: PAGE_PAD, right: PAGE_PAD, top: PAGE_PAD, bottom: 40 }).show(ui, |ui| {
            ui.set_max_width(ui.available_width().min(if read { READ_W } else { 1400.0 }));
            ui.spacing_mut().item_spacing.y = 16.0;
            add(ui);
        });
    });
}

// --- Panels and rows ---

/// `.panel`: surface, 1px line, header row with icon and title. Rows go inside (see [`row`]).
pub fn panel<R>(ui: &mut Ui, icon: &str, title: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    panel_with(ui, icon, title, |_| {}, add)
}

/// [`panel`] with something on the right of the header (a badge, a link button).
pub fn panel_with<R>(ui: &mut Ui, icon: &str, title: &str, right: impl FnOnce(&mut Ui), add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(CornerRadius::same(R_LG))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.allocate_ui_with_layout(vec2(ui.available_width(), 40.0), Layout::left_to_right(Align::Center), |ui| {
                ui.add_space(16.0);
                if !icon.is_empty() {
                    ui.label(RichText::new(icon).size(15.0).color(SUBTLE));
                }
                ui.label(RichText::new(title).font(semibold_font(13.0)).color(MUTED));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(10.0);
                    right(ui);
                });
            });
            add(ui)
        })
        .inner
}

/// A row with free content, separated from the previous one by a line.
pub fn row_custom<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let r = Frame::new().inner_margin(Margin::symmetric(16, 12)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
        add(ui)
    });
    let rect = r.response.rect;
    ui.painter().hline(rect.x_range(), rect.top(), Stroke::new(1.0, LINE));
    r.inner
}

fn label_and_hint(ui: &mut Ui, label: &str, hint_text: &str) {
    ui.spacing_mut().item_spacing.y = 2.0;
    ui.add(egui::Label::new(body(label)).wrap());
    if !hint_text.is_empty() {
        note(ui, hint_text);
    }
}

/// Setting row: label and hint on the left, a small control (switch, chips, button) on the right.
/// On a narrow window it stacks like [`row_wide`].
pub fn row<R>(ui: &mut Ui, label: &str, hint_text: &str, control: impl FnOnce(&mut Ui) -> R) -> R {
    row_custom(ui, |ui| {
        let width = ui.available_width();
        if width < 520.0 {
            return stacked(ui, label, hint_text, control);
        }
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width((width * 0.55).min(440.0));
                label_and_hint(ui, label, hint_text);
            });
            ui.with_layout(Layout::right_to_left(Align::Center), control).inner
        })
        .inner
    })
}

/// Setting row whose control needs width (a select, a slider, a list): control goes under the label.
pub fn row_wide<R>(ui: &mut Ui, label: &str, hint_text: &str, control: impl FnOnce(&mut Ui) -> R) -> R {
    row_custom(ui, |ui| stacked(ui, label, hint_text, control))
}

fn stacked<R>(ui: &mut Ui, label: &str, hint_text: &str, control: impl FnOnce(&mut Ui) -> R) -> R {
    ui.vertical(|ui| {
        label_and_hint(ui, label, hint_text);
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.add_space(2.0);
        control(ui)
    })
    .inner
}

/// Row with a switch.
pub fn toggle(ui: &mut Ui, label: &str, hint_text: &str, on: &mut bool) -> bool {
    row(ui, label, hint_text, |ui| switch(ui, on).changed())
}

/// A clickable-looking list row (40 px+, hover tint, line above). Used for clips and attention items.
pub fn list_item<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let mut prepared = Frame::new().inner_margin(Margin::symmetric(16, 8)).begin(ui);
    let inner = {
        let ui = &mut prepared.content_ui;
        ui.set_width(ui.available_width());
        ui.set_min_height(24.0);
        ui.spacing_mut().item_spacing = vec2(8.0, 4.0);
        add(ui)
    };
    let outer = prepared.content_ui.min_rect().expand2(vec2(16.0, 8.0));
    if ui.rect_contains_pointer(outer) {
        prepared.frame = prepared.frame.fill(SURFACE_2);
    }
    prepared.end(ui);
    ui.painter().hline(outer.x_range(), outer.top(), Stroke::new(1.0, LINE));
    inner
}

/// Empty state: what is missing in one sentence, then whatever fixes it.
pub fn empty<R>(ui: &mut Ui, text: &str, fix: impl FnOnce(&mut Ui) -> R) -> R {
    row_custom(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(12.0);
            ui.add(egui::Label::new(RichText::new(text).size(13.0).color(MUTED)).wrap());
            ui.add_space(4.0);
            let r = fix(ui);
            ui.add_space(12.0);
            r
        })
        .inner
    })
}

// --- Stat strip (ProjectHub overview) ---

pub struct Stat {
    pub value: String,
    pub label: &'static str,
    pub sub: String,
    pub color: Color32,
}

/// One panel, cells separated by lines: figure, label, a short status line.
pub fn stat_strip(ui: &mut Ui, stats: &[Stat]) {
    let n = if ui.available_width() >= 700.0 { stats.len() } else { 2 };
    let r = Frame::new().fill(SURFACE).stroke(Stroke::new(1.0, LINE)).corner_radius(CornerRadius::same(R_LG)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let mut line_ys = Vec::new();
        for (ri, row) in stats.chunks(n).enumerate() {
            if ri > 0 {
                line_ys.push(ui.cursor().top());
            }
            ui.columns(n, |cols| {
                for (col, stat) in cols.iter_mut().zip(row) {
                    Frame::new().inner_margin(Margin::symmetric(16, 14)).show(col, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.add(egui::Label::new(strong(&stat.value, 20.0)).truncate());
                        ui.label(hint(stat.label));
                        ui.add(egui::Label::new(RichText::new(&stat.sub).size(11.0).color(stat.color)).truncate());
                    });
                }
            });
        }
        line_ys
    });
    let rect = r.response.rect;
    let p = ui.painter();
    for k in 1..n {
        let x = rect.left() + rect.width() * k as f32 / n as f32;
        p.vline(x, rect.y_range(), Stroke::new(1.0, LINE));
    }
    for y in r.inner {
        p.hline(rect.x_range(), y, Stroke::new(1.0, LINE));
    }
}

// --- Small things ---

#[derive(Clone, Copy)]
pub enum Tone {
    Success,
    Warning,
    Danger,
    Info,
    Neutral,
}

impl Tone {
    pub fn color(self) -> Color32 {
        match self {
            Tone::Success => SUCCESS,
            Tone::Warning => WARN,
            Tone::Danger => DANGER,
            Tone::Info => ACCENT,
            Tone::Neutral => MUTED,
        }
    }
}

/// Pill with a tinted background. State is also in the words, never colour alone.
pub fn badge(ui: &mut Ui, tone: Tone, text: &str) -> Response {
    let c = tone.color();
    let (fill, stroke) = match tone {
        Tone::Neutral => (SURFACE_2, LINE),
        _ => (c.gamma_multiply(0.10), c.gamma_multiply(0.25)),
    };
    // Painted, not a Frame: a Frame would stretch to the row height inside horizontal layouts.
    let galley = text_galley(ui, text, semibold_font(12.0));
    let (rect, response) = ui.allocate_exact_size(vec2(galley.size().x + 18.0, 22.0), Sense::hover());
    let p = ui.painter();
    p.rect(rect, CornerRadius::same(255), fill, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
    p.galley(rect.center() - galley.size() / 2.0, galley, c);
    response
}

/// A key as a small cap, e.g. Ctrl+F10.
pub fn keycap(ui: &mut Ui, text: &str) -> Response {
    let galley = text_galley(ui, text, mono_font(12.0));
    let (rect, response) = ui.allocate_exact_size(vec2(galley.size().x + 14.0, 22.0), Sense::hover());
    let p = ui.painter();
    p.rect(rect, CornerRadius::same(R_SM), SURFACE_2, Stroke::new(1.0, LINE_STRONG), egui::StrokeKind::Inside);
    p.galley(rect.center() - galley.size() / 2.0, galley, TEXT);
    response
}

/// Small coloured dot (status).
pub fn dot(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
}

/// Sidebar entry: 32 px, icon + label, optional count on the right. Active = surface-2.
pub fn nav_item(ui: &mut Ui, icon: &str, label: &str, active: bool, count: Option<(Tone, &str)>) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
    response.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, active, label));
    let p = ui.painter();
    let radius = CornerRadius::same(R_MD);
    if active {
        p.rect_filled(rect, radius, SURFACE_2);
    } else if response.hovered() {
        p.rect_filled(rect, radius, SURFACE_2.gamma_multiply(0.5));
    }
    let color = if active || response.hovered() { TEXT } else { MUTED };
    p.text(rect.left_center() + vec2(10.0, 0.0), egui::Align2::LEFT_CENTER, icon, FontId::proportional(16.0), color);
    let font = if active { semibold_font(13.0) } else { font(13.0) };
    p.text(rect.left_center() + vec2(34.0, 0.0), egui::Align2::LEFT_CENTER, label, font, color);
    if let Some((tone, text)) = count {
        p.text(rect.right_center() - vec2(10.0, 0.0), egui::Align2::RIGHT_CENTER, text, font_meta(), tone.color());
    }
    if response.has_focus() {
        p.rect_stroke(rect.expand(1.0), CornerRadius::same(R_MD + 1), Stroke::new(2.0, ACCENT), egui::StrokeKind::Outside);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn font_meta() -> FontId {
    font(11.0)
}

/// `.section-label`: small heading above a group in the sidebar or a page.
pub fn section_label(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).font(semibold_font(12.0)).color(SUBTLE));
}

/// Selectable tile (quality presets): title, mono meta line, one-line description.
pub fn choice_tile(ui: &mut Ui, selected: bool, title: &str, meta_line: &str, desc: &str) -> Response {
    let mut prepared = Frame::new().inner_margin(Margin::same(12)).corner_radius(CornerRadius::same(R_MD)).begin(ui);
    {
        let ui = &mut prepared.content_ui;
        ui.set_width(ui.available_width());
        ui.with_layout(Layout::top_down(Align::Min), |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.label(strong(title, 13.0).color(if selected { ACCENT } else { TEXT }));
            ui.add(egui::Label::new(mono(meta_line, 11.0).color(MUTED)).wrap());
            ui.add(egui::Label::new(meta(desc)).wrap());
        });
    }
    let rect = prepared.content_ui.min_rect().expand(12.0);
    let response = ui.interact(rect, egui::Id::new(("tk_tile", title)), Sense::click());
    response.widget_info(|| WidgetInfo::selected(WidgetType::RadioButton, true, selected, title));
    let (fill, stroke) = if selected {
        (ACCENT.gamma_multiply(0.08), ACCENT.gamma_multiply(0.45))
    } else if response.hovered() {
        (SURFACE_2, LINE_STRONG)
    } else {
        (SURFACE, LINE)
    };
    prepared.frame = prepared.frame.fill(fill).stroke(Stroke::new(1.0, stroke));
    prepared.end(ui);
    focus_ring(ui, &response, R_MD);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
