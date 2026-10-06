//! Tiksu design system for egui. Values are copied from `Tiksu brändi/DESIGN.md` and
//! `design-system/tiksu.css` (dark theme only, by design). Change the brand there first.

use eframe::egui::{
    self, epaint, text::LayoutJob, vec2, Align, Color32, CornerRadius, FontData,
    FontDefinitions, FontFamily, FontId, Frame, Margin, Painter, Pos2, Rect, Response, RichText,
    Sense, Shadow, Stroke, TextFormat, TextStyle, Ui, WidgetInfo, WidgetText, WidgetType,
};

// --- Colors (translucent tokens are premultiplied: rgb * alpha) ---
pub const MINT: Color32 = Color32::from_rgb(0x3a, 0xd9, 0xb8);
pub const BLUE: Color32 = Color32::from_rgb(0x5b, 0x7d, 0xff);
pub const BG: Color32 = Color32::from_rgb(0x07, 0x0f, 0x14);
pub const SURFACE: Color32 = Color32::from_rgb(0x10, 0x1c, 0x25);
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(0x16, 0x24, 0x2e);
pub const SURFACE_SUNK: Color32 = Color32::from_rgb(0x05, 0x0c, 0x11);
pub const OVERLAY: Color32 = Color32::from_rgb(0x0d, 0x1a, 0x22);
/// Sidebar: near-black, darker than every surface so white nav text reads at a glance.
pub const SIDEBAR_BG: Color32 = Color32::from_rgb(0x03, 0x07, 0x0a);
pub const LINE: Color32 = Color32::from_rgba_premultiplied(18, 22, 24, 31); // rgba(146,184,200,.12)
pub const LINE_STRONG: Color32 = Color32::from_rgba_premultiplied(32, 40, 44, 56); // .22
pub const LINE_BRAND: Color32 = Color32::from_rgba_premultiplied(20, 76, 64, 89); // mint .35
pub const TEXT: Color32 = Color32::from_rgb(0xea, 0xf2, 0xf7);
pub const TEXT_2: Color32 = Color32::from_rgb(0xb9, 0xcb, 0xd6);
pub const MUTED: Color32 = Color32::from_rgb(0x82, 0x98, 0xa5);
pub const ON_BRAND: Color32 = Color32::from_rgb(0x04, 0x12, 0x1a);
pub const WARNING: Color32 = Color32::from_rgb(0xf0, 0xb3, 0x54);
pub const DANGER: Color32 = Color32::from_rgb(0xff, 0x6b, 0x7d);

// --- Radii, sizes ---
pub const R_SM: u8 = 8;
pub const R_MD: u8 = 12;
pub const R_LG: u8 = 16;
pub const SIDEBAR_W: f32 = 264.0;
pub const TOPBAR_H: f32 = 64.0;
pub const PAGE_PAD: i8 = 32;

// --- Fonts: Outfit (display), Instrument Sans (UI), JetBrains Mono (data) ---
const DISPLAY: &str = "tk-display";
const SEMIBOLD: &str = "tk-semibold";

pub fn display_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(DISPLAY.into()))
}
pub fn semibold_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}
pub fn mono_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

/// Installs fonts, text styles and widget visuals. Call once at startup.
pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let faces: [(&str, &'static [u8]); 4] = [
        ("outfit-bold", include_bytes!("../assets/fonts/Outfit-Bold.ttf")),
        ("instrument-regular", include_bytes!("../assets/fonts/InstrumentSans-Regular.ttf")),
        ("instrument-semibold", include_bytes!("../assets/fonts/InstrumentSans-SemiBold.ttf")),
        ("jetbrains-medium", include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf")),
    ];
    for (name, bytes) in faces {
        fonts.font_data.insert(name.into(), FontData::from_static(bytes).into());
    }
    // egui's bundled fonts stay as fallbacks (symbols, emoji).
    let fallback = fonts.families[&FontFamily::Proportional].clone();
    let family = |first: &str| [vec![first.to_string()], fallback.clone()].concat();
    fonts.families.insert(FontFamily::Proportional, family("instrument-regular"));
    fonts.families.insert(FontFamily::Name(DISPLAY.into()), family("outfit-bold"));
    fonts.families.insert(FontFamily::Name(SEMIBOLD.into()), family("instrument-semibold"));
    fonts.families.get_mut(&FontFamily::Monospace).unwrap().insert(0, "jetbrains-medium".into());
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    for name in [DISPLAY, SEMIBOLD] {
        fonts.families.get_mut(&FontFamily::Name(name.into())).unwrap().push("phosphor".into());
    }
    ctx.set_fonts(fonts);

    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, display_font(28.0)),
            (TextStyle::Body, FontId::proportional(15.0)),
            (TextStyle::Button, semibold_font(14.0)),
            (TextStyle::Small, FontId::proportional(13.0)),
            (TextStyle::Monospace, mono_font(13.0)),
        ]
        .into();
        style.spacing.item_spacing = vec2(8.0, 8.0);
        style.spacing.button_padding = vec2(12.0, 9.0);
        style.spacing.interact_size.y = 40.0;
        style.spacing.combo_height = 320.0;

        let v = &mut style.visuals;
        *v = egui::Visuals::dark();
        v.panel_fill = BG;
        v.window_fill = OVERLAY;
        v.extreme_bg_color = SURFACE_SUNK;
        v.faint_bg_color = SURFACE;
        v.code_bg_color = SURFACE_SUNK;
        v.hyperlink_color = BLUE;
        v.warn_fg_color = WARNING;
        v.error_fg_color = DANGER;
        v.window_stroke = Stroke::new(1.0, LINE_STRONG);
        v.window_corner_radius = CornerRadius::same(R_MD);
        v.menu_corner_radius = CornerRadius::same(R_MD);
        v.window_shadow = Shadow { offset: [0, 16], blur: 48, spread: 0, color: Color32::from_black_alpha(150) };
        v.popup_shadow = v.window_shadow;
        v.selection.bg_fill = Color32::from_rgba_premultiplied(9, 35, 29, 41); // mint .16
        v.selection.stroke = Stroke::new(1.0, MINT);
        v.slider_trailing_fill = true;

        let w = &mut v.widgets;
        w.noninteractive.bg_fill = SURFACE;
        w.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
        w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        // Inputs and selects sit sunk; hover lifts them to surface (tiksu.css .tk-input).
        for (state, fill, stroke, fg) in [
            (&mut w.inactive, SURFACE_SUNK, LINE, TEXT),
            (&mut w.hovered, SURFACE_HOVER, LINE_STRONG, TEXT),
            (&mut w.active, SURFACE_HOVER, MINT, TEXT),
            (&mut w.open, SURFACE_SUNK, MINT, TEXT),
        ] {
            state.bg_fill = fill;
            state.weak_bg_fill = fill;
            state.bg_stroke = Stroke::new(1.0, stroke);
            state.fg_stroke = Stroke::new(1.0, fg);
            state.corner_radius = CornerRadius::same(R_MD);
            state.expansion = 0.0;
        }
        w.inactive.bg_fill = LINE_STRONG; // slider rail
    });
}

// --- Text ---

pub fn display(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(display_font(size)).color(TEXT)
}

pub fn mono(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(mono_font(size))
}

/// Mono, uppercase, 0.12em tracking. Used for nav groups, stat labels and product name.
pub fn eyebrow(ui: &mut Ui, text: &str) {
    ui.label(eyebrow_text(text, MUTED));
}

pub fn eyebrow_text(text: &str, color: Color32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.append(
        &text.to_uppercase(),
        0.0,
        TextFormat { font_id: mono_font(11.0), color, extra_letter_spacing: 1.3, ..Default::default() },
    );
    job
}

// --- Brand gradient: always along the long axis, mint first ---

fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

pub fn gradient(painter: &Painter, rect: Rect, radius: u8, vertical: bool, opacity: f32) {
    let mut mesh = epaint::Mesh::default();
    let mut tess = epaint::Tessellator::new(
        painter.ctx().pixels_per_point(),
        Default::default(),
        [1, 1],
        vec![],
    );
    tess.tessellate_rect(&epaint::RectShape::filled(rect, radius, Color32::WHITE), &mut mesh);
    for v in &mut mesh.vertices {
        let t = if vertical {
            (v.pos.y - rect.top()) / rect.height()
        } else {
            (v.pos.x - rect.left()) / rect.width()
        };
        v.color = lerp(MINT, BLUE, t.clamp(0.0, 1.0)).gamma_multiply(v.color.a() as f32 / 255.0 * opacity);
    }
    painter.add(mesh);
}

/// The single ambient mint glow in the page background (top-left).
pub fn ambient_glow(painter: &Painter, center: Pos2, radius: f32) {
    let mut mesh = epaint::Mesh::default();
    let core = Color32::from_rgba_premultiplied(3, 12, 10, 14); // mint ~.055
    mesh.colored_vertex(center, core);
    let n = 48;
    for i in 0..=n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        mesh.colored_vertex(center + radius * vec2(a.cos(), a.sin()), Color32::TRANSPARENT);
        if i > 0 {
            mesh.add_triangle(0, i, i + 1);
        }
    }
    painter.add(mesh);
}

fn focus_ring(ui: &Ui, response: &Response, radius: u8) {
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.expand(2.0),
            CornerRadius::same(radius.saturating_add(2)),
            Stroke::new(2.0, MINT),
            egui::StrokeKind::Outside,
        );
    }
}

// --- Buttons ---

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Primary,
    Secondary,
    Ghost,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Size {
    Sm,
    Md,
}

/// `tk-btn`. Hover changes color only: no lift, no scale, no glow.
pub fn button(ui: &mut Ui, kind: Kind, size: Size, text: &str, enabled: bool) -> Response {
    let (height, pad, font, radius) = match size {
        Size::Sm => (32.0, 12.0, semibold_font(13.0), R_SM),
        Size::Md => (40.0, 16.0, semibold_font(14.0), R_MD),
    };
    let text_color = match kind {
        Kind::Primary => ON_BRAND,
        Kind::Secondary => TEXT,
        Kind::Ghost => TEXT_2,
    };
    let galley = text_galley(ui, text, font);
    let size = vec2(galley.size().x + 2.0 * pad, height);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, text));

    if ui.is_rect_visible(rect) {
        let hovered = enabled && response.hovered();
        let alpha = if enabled { 1.0 } else { 0.45 };
        let p = ui.painter();
        let r = CornerRadius::same(radius);
        match kind {
            Kind::Primary => {
                gradient(p, rect, radius, false, alpha);
                if hovered {
                    p.rect_filled(rect, r, Color32::from_white_alpha(18)); // brightness(1.08)
                }
            }
            Kind::Secondary => {
                let fill = if hovered { SURFACE_HOVER } else { SURFACE };
                let stroke = if hovered { LINE_BRAND } else { LINE_STRONG };
                p.rect(rect, r, fill.gamma_multiply(alpha), Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
            }
            Kind::Ghost => {
                if hovered {
                    p.rect_filled(rect, r, SURFACE);
                }
            }
        }
        let color = if kind == Kind::Ghost && hovered { TEXT } else { text_color };
        p.galley(rect.center() - galley.size() / 2.0, galley, color.gamma_multiply(alpha));
        focus_ring(ui, &response, radius);
    }
    if enabled { response.on_hover_cursor(egui::CursorIcon::PointingHand) } else { response }
}

/// Segmented choice (sm buttons); the selected one gets the brand border.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        for (option, label) in options {
            let selected = *value == *option;
            let galley = text_galley(ui, label, semibold_font(13.0));
            let (rect, response) =
                ui.allocate_exact_size(vec2(galley.size().x + 24.0, 32.0), Sense::click());
            response.widget_info(|| WidgetInfo::selected(WidgetType::RadioButton, true, selected, *label));
            let (fill, stroke, color) = if selected {
                (MINT.gamma_multiply(0.10), LINE_BRAND, MINT)
            } else if response.hovered() {
                (SURFACE, LINE_STRONG, TEXT)
            } else {
                (SURFACE_SUNK, LINE, TEXT_2)
            };
            let p = ui.painter();
            p.rect(rect, CornerRadius::same(R_SM), fill, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
            p.galley(rect.center() - galley.size() / 2.0, galley, color);
            focus_ring(ui, &response, R_SM);
            if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !selected {
                *value = *option;
                changed = true;
            }
        }
    });
    changed
}

/// `tk-switch`: 42x24, line-strong off, mint on, white knob.
pub fn switch(ui: &mut Ui, on: &mut bool) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(vec2(42.0, 24.0), Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    response.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, true, *on, ""));
    let t = ui.ctx().animate_bool_with_time(response.id, *on, 0.12);
    let p = ui.painter();
    let track = if *on { MINT } else { LINE_STRONG };
    p.rect_filled(rect, CornerRadius::same(12), track);
    let knob = egui::pos2(rect.left() + 12.0 + t * 18.0, rect.center().y);
    p.circle_filled(knob + vec2(0.0, 1.0), 9.0, Color32::from_black_alpha(100));
    p.circle_filled(knob, 9.0, Color32::WHITE);
    focus_ring(ui, &response, 12);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Slider in the switch's language: 4px rail, mint fill up to the value, white knob.
pub fn slider(ui: &mut Ui, value: &mut u32, range: std::ops::RangeInclusive<u32>, step: u32) -> Response {
    let (lo, hi) = (*range.start(), *range.end());
    let width = (ui.available_width() - 90.0).clamp(140.0, 420.0); // leaves room for the value label
    let (rect, mut response) = ui.allocate_exact_size(vec2(width, 24.0), Sense::click_and_drag());
    let rail = Rect::from_center_size(rect.center(), vec2(rect.width() - 18.0, 4.0));

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
    p.rect_filled(Rect::from_min_max(rail.min, egui::pos2(x, rail.max.y)), CornerRadius::same(2), MINT);
    let knob = egui::pos2(x, rail.center().y);
    p.circle_filled(knob + vec2(0.0, 1.0), 9.0, Color32::from_black_alpha(100));
    p.circle_filled(knob, 9.0, Color32::WHITE);
    if response.has_focus() {
        p.circle_stroke(knob, 12.0, Stroke::new(2.0, MINT));
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// `tk-toggle` row: label + hint on the left, switch on the right.
pub fn toggle_row(ui: &mut Ui, label: &str, hint: &str, on: &mut bool) -> bool {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(RichText::new(label).font(semibold_font(14.0)).color(TEXT));
            ui.label(RichText::new(hint).size(12.0).color(MUTED));
        });
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| switch(ui, on).changed())
            .inner
    })
    .inner
}

// --- Containers ---

/// Temp-data keys `grid` uses to give the cards of one row the same height.
const CARD_MIN_H: &str = "tk_card_min_h";
const CARD_NATURAL_H: &str = "tk_card_natural_h";

/// `tk-card`: surface, 1px line, 16px radius, 20px padding. Separated by border, not shadow.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let min_h: f32 = ui.data(|d| d.get_temp(egui::Id::new(CARD_MIN_H))).unwrap_or(0.0);
    Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(CornerRadius::same(R_LG))
        .inner_margin(Margin::same(20))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // Before the content: set_min_height counts from the cursor, not from the top.
            ui.set_min_height(min_h);
            let top = ui.cursor().top();
            let r = add(ui);
            let natural = ui.cursor().top() - top;
            ui.data_mut(|d| {
                let prev: f32 = d.get_temp(egui::Id::new(CARD_NATURAL_H)).unwrap_or(0.0);
                d.insert_temp(egui::Id::new(CARD_NATURAL_H), prev.max(natural));
            });
            r
        })
        .inner
}

/// Responsive card grid (`tk-field-row` / card grid): as many columns of at least `min_col`
/// as fit, 16px gaps, cards in a row equally tall (heights settle one frame later).
pub fn grid<S>(ui: &mut Ui, state: &mut S, min_col: f32, sections: &[fn(&mut S, &mut Ui)]) {
    let n = (((ui.available_width() + 16.0) / (min_col + 16.0)).floor() as usize).clamp(1, sections.len().max(1));
    let (min_key, nat_key) = (egui::Id::new(CARD_MIN_H), egui::Id::new(CARD_NATURAL_H));
    for (ri, row) in sections.chunks(n).enumerate() {
        let row_key = ui.id().with(("tk_grid_row", ri, n));
        let target: f32 = if n > 1 { ui.data(|d| d.get_temp(row_key)).unwrap_or(0.0) } else { 0.0 };
        let mut tallest = 0.0f32;
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            ui.columns(n, |cols| {
                for (col, section) in cols.iter_mut().zip(row) {
                    col.data_mut(|d| {
                        d.insert_temp(min_key, target);
                        d.insert_temp(nat_key, 0.0f32);
                    });
                    // Columns are justified by default, which letter-spaces wrapped text.
                    col.with_layout(egui::Layout::top_down(Align::Min), |ui| section(state, ui));
                    tallest = tallest.max(col.data(|d| d.get_temp(nat_key)).unwrap_or(0.0));
                }
            });
        });
        ui.data_mut(|d| {
            d.insert_temp(min_key, 0.0f32);
            d.insert_temp(row_key, tallest);
        });
        if n > 1 && (tallest - target).abs() > 0.5 {
            ui.ctx().request_repaint();
        }
        ui.add_space(16.0);
    }
}

/// Selectable tile for presets: title, mono meta line, one-line description.
/// Selected = brand border and mint title, the same signal as a selected segment.
pub fn choice_tile(ui: &mut Ui, selected: bool, title: &str, meta: &str, desc: &str) -> Response {
    let mut prepared = Frame::new()
        .inner_margin(Margin::same(14))
        .corner_radius(CornerRadius::same(R_MD))
        .begin(ui);
    {
        let ui = &mut prepared.content_ui;
        ui.set_width(ui.available_width());
        ui.with_layout(egui::Layout::top_down(Align::Min), |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.label(display(title, 15.0).color(if selected { MINT } else { TEXT }));
            ui.add(egui::Label::new(mono(meta, 12.0).color(TEXT_2)).wrap());
            ui.add(egui::Label::new(RichText::new(desc).size(12.0).color(MUTED)).wrap());
        });
    }
    let rect = prepared.content_ui.min_rect().expand(14.0);
    let response = ui.interact(rect, egui::Id::new(("tk_tile", title)), Sense::click());
    response.widget_info(|| WidgetInfo::selected(WidgetType::RadioButton, true, selected, title));
    let (fill, stroke) = if selected {
        (MINT.gamma_multiply(0.06), LINE_BRAND)
    } else if response.hovered() {
        (SURFACE_HOVER, LINE_STRONG)
    } else {
        (SURFACE_SUNK, LINE)
    };
    prepared.frame = prepared.frame.fill(fill).stroke(Stroke::new(1.0, stroke));
    prepared.end(ui);
    focus_ring(ui, &response, R_MD);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn card_header(ui: &mut Ui, title: &str, sub: &str) {
    card_header_badge(ui, title, sub, None);
}

/// Card title with an optional badge after it (e.g. "Mukautettu", "2 kpl").
pub fn card_header_badge(ui: &mut Ui, title: &str, sub: &str, badge_: Option<(Tone, &str)>) {
    ui.spacing_mut().item_spacing.y = 2.0;
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = 0.0; // a horizontal row is otherwise 40px tall
        ui.horizontal(|ui| {
            ui.label(display(title, 17.0));
            if let Some((tone, text)) = badge_ {
                badge(ui, tone, text);
            }
        });
    });
    if !sub.is_empty() {
        ui.add(egui::Label::new(RichText::new(sub).size(13.0).color(MUTED)).wrap());
    }
    ui.spacing_mut().item_spacing.y = 8.0;
    ui.add_space(12.0);
}

/// `tk-field`: label 13/600, control, hint 12 muted.
pub fn field<R>(ui: &mut Ui, label: &str, hint: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.spacing_mut().item_spacing.y = 6.0;
    ui.label(RichText::new(label).font(semibold_font(13.0)).color(TEXT_2));
    let r = add(ui);
    if !hint.is_empty() {
        ui.label(RichText::new(hint).size(12.0).color(MUTED));
    }
    ui.spacing_mut().item_spacing.y = 8.0;
    r
}

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
            Tone::Success => MINT,
            Tone::Warning => WARNING,
            Tone::Danger => DANGER,
            Tone::Info => BLUE,
            Tone::Neutral => TEXT_2,
        }
    }
}

/// `tk-badge`: pill with tinted background. State is also in the words, never color alone.
pub fn badge(ui: &mut Ui, tone: Tone, text: &str) -> Response {
    let c = tone.color();
    let (fill, stroke) = match tone {
        Tone::Neutral => (SURFACE, LINE_STRONG),
        _ => (c.gamma_multiply(0.10), c.gamma_multiply(0.22)),
    };
    // Painted, not a Frame: a Frame would stretch to the row height inside horizontal layouts.
    let galley = text_galley(ui, text, semibold_font(12.0));
    let (rect, response) = ui.allocate_exact_size(galley.size() + vec2(20.0, 8.0), Sense::hover());
    let p = ui.painter();
    p.rect(rect, CornerRadius::same(255), fill, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
    p.galley(rect.center() - galley.size() / 2.0, galley, c);
    response
}

/// `tk-stat`: eyebrow, mono value 28px, small line below.
pub fn stat(ui: &mut Ui, width: f32, label: &str, value: &str, sub: &str, sub_color: Color32) {
    Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(CornerRadius::same(R_LG))
        .inner_margin(Margin::symmetric(20, 16))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.set_width(width - 42.0);
                ui.spacing_mut().item_spacing.y = 4.0;
                eyebrow(ui, label);
                ui.add(egui::Label::new(mono(value, 28.0).color(TEXT)).truncate());
                ui.add(egui::Label::new(RichText::new(sub).size(12.0).color(sub_color)).truncate());
            });
        });
}

fn text_galley(ui: &Ui, text: &str, font: FontId) -> std::sync::Arc<egui::Galley> {
    WidgetText::from(RichText::new(text).font(font)).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        TextStyle::Button,
    )
}
