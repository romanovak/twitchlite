// Interfaz con el look de Twitch (tema oscuro). Los íconos se dibujan con el painter: cero fuentes
// de íconos o imágenes extra en memoria.
use crate::{App, Msg, Page, Part, img, sort_str, twitch, twitch::Live};
use eframe::egui::{
    self, Align, Align2, Color32, FontFamily, FontId, Key, Layout, Margin, Modifiers, Painter, Pos2, Rect, Response, RichText, Sense, Shape, Stroke, Ui, UiBuilder, Vec2, load::SizedTexture,
    pos2, text::LayoutJob, vec2,
};
use std::time::{Duration, Instant};

pub const BG: Color32 = Color32::from_rgb(14, 14, 16);
pub const PANEL: Color32 = Color32::from_rgb(24, 24, 27);
pub const SIDE: Color32 = Color32::from_rgb(31, 31, 35);
pub const HOVER: Color32 = Color32::from_rgb(44, 44, 51);
pub const LINE: Color32 = Color32::from_rgb(47, 47, 53);
pub const TEXT: Color32 = Color32::from_rgb(239, 239, 241);
pub const DIM: Color32 = Color32::from_rgb(173, 173, 184);
pub const PURPLE: Color32 = Color32::from_rgb(145, 71, 255);
pub const PURPLE_L: Color32 = Color32::from_rgb(191, 148, 255);
pub const RED: Color32 = Color32::from_rgb(235, 4, 0);

pub const EMOTE_H: f32 = 28.0;
pub const EMOJI_H: f32 = 20.0;
pub const BADGE_H: f32 = 18.0;
const SIDE_W: f32 = 240.0;
const SIDE_MIN: f32 = 52.0;
/// Debajo de este ancho la info del canal pasa a dos renglones.
const COMPACT_W: f32 = 760.0;

fn semi(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semi".into()))
}

pub fn style(ctx: &egui::Context) {
    let mut f = egui::FontDefinitions::default();
    let base = f.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let fonts = std::path::Path::new(&std::env::var("WINDIR").unwrap_or_default()).join("Fonts");
    let mut semi_fam = base.clone();
    // Segoe UI Variable (la de Windows 11) si existe; si no, Segoe UI clásica
    let body = if fonts.join("SegUIVar.ttf").exists() { "SegUIVar.ttf" } else { "segoeui.ttf" };
    for (name, file) in [("ui", body), ("ui-semi", "seguisb.ttf")] {
        if let Ok(bytes) = std::fs::read(fonts.join(file)) {
            f.font_data.insert(name.into(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
            if name == "ui" {
                f.families.entry(FontFamily::Proportional).or_default().insert(0, name.into());
                semi_fam.push(name.into());
            } else {
                semi_fam.insert(0, name.into());
            }
        }
    }
    for file in ["YuGothR.ttc", "msyh.ttc", "malgun.ttf"] {
        if let Ok(bytes) = std::fs::read(fonts.join(file)) {
            let name = format!("cjk-{file}");
            f.font_data.insert(name.clone(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
            f.families.entry(FontFamily::Proportional).or_default().push(name.clone());
            semi_fam.push(name);
            break;
        }
    }
    f.families.insert(FontFamily::Name("semi".into()), semi_fam);
    ctx.set_fonts(f);

    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = Color32::from_rgb(40, 40, 46);
    v.faint_bg_color = SIDE;
    v.selection.bg_fill = PURPLE;
    v.selection.stroke = Stroke::new(1.5, PURPLE_L);
    v.hyperlink_color = PURPLE_L;
    v.window_stroke = Stroke::new(1.0, LINE);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(47, 47, 53);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(60, 60, 68);
    v.widgets.active.weak_bg_fill = PURPLE;
    // Botones planos y redondeados como twitch.tv; popups con sombra suave
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = 4.into();
    }
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.hovered.bg_stroke = Stroke::NONE;
    v.widgets.active.bg_stroke = Stroke::NONE;
    v.text_edit_bg_color = Some(Color32::from_rgb(24, 24, 27));
    v.window_corner_radius = 8.into();
    v.menu_corner_radius = 6.into();
    v.popup_shadow = egui::Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(150) };
    v.window_shadow = egui::Shadow { offset: [0, 10], blur: 30, spread: 0, color: Color32::from_black_alpha(170) };
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.scroll = egui::style::ScrollStyle::floating(); // barras delgadas que aparecen al pasar el mouse
        s.spacing.button_padding = vec2(10.0, 5.0);
        s.text_styles.insert(egui::TextStyle::Body, FontId::proportional(13.5));
        s.text_styles.insert(egui::TextStyle::Button, FontId::proportional(13.5));
        s.text_styles.insert(egui::TextStyle::Small, FontId::proportional(11.5));
    });
}

// ---------- Íconos vectoriales ----------

#[derive(Clone, Copy)]
pub enum Icon {
    Play,
    Pause,
    Vol,
    Mute,
    Gear,
    Theater,
    Full,
    Pin,
    Video,
    Left,
    Right,
    Smile,
    Search,
    Person,
    Reply,
    Close,
    Points,
    Chat,
    Heart,
    Share,
    Gauge,
    Users,
    Refresh,
    Info,
    Verified,
    Bits,
    WinMin,
    WinMax,
    WinRestore,
    Bell,
    Keyboard,
}

fn arc(c: Pos2, r: f32, a0: f32, a1: f32) -> Vec<Pos2> {
    (0..=16).map(|i| a0 + (a1 - a0) * i as f32 / 16.0).map(|a| c + vec2(a.cos(), a.sin()) * r).collect()
}

fn paint_icon(p: &Painter, r: Rect, icon: Icon, c: Color32) {
    let s = Stroke::new(1.7, c);
    let (w, h, m) = (r.width(), r.height(), r.center());
    match icon {
        Icon::Play => {
            p.add(Shape::convex_polygon(vec![r.left_top() + vec2(w * 0.15, 0.0), r.left_bottom() + vec2(w * 0.15, 0.0), pos2(r.right(), m.y)], c, Stroke::NONE));
        }
        Icon::Pause => {
            p.rect_filled(Rect::from_min_size(r.min + vec2(w * 0.12, 0.0), vec2(w * 0.28, h)), 1.0, c);
            p.rect_filled(Rect::from_min_size(r.min + vec2(w * 0.6, 0.0), vec2(w * 0.28, h)), 1.0, c);
        }
        Icon::Vol | Icon::Mute => {
            let x0 = r.left();
            p.add(Shape::convex_polygon(
                vec![pos2(x0, m.y - h * 0.18), pos2(x0 + w * 0.22, m.y - h * 0.18), pos2(x0 + w * 0.5, r.top() + h * 0.08), pos2(x0 + w * 0.5, r.bottom() - h * 0.08), pos2(x0 + w * 0.22, m.y + h * 0.18), pos2(x0, m.y + h * 0.18)],
                c,
                Stroke::NONE,
            ));
            if matches!(icon, Icon::Vol) {
                let o = pos2(x0 + w * 0.5, m.y);
                p.add(Shape::line(arc(o, w * 0.22, -0.8, 0.8), s));
                p.add(Shape::line(arc(o, w * 0.42, -0.9, 0.9), s));
            } else {
                let a = pos2(x0 + w * 0.65, m.y - h * 0.18);
                p.line_segment([a, a + vec2(w * 0.32, h * 0.36)], s);
                p.line_segment([a + vec2(0.0, h * 0.36), a + vec2(w * 0.32, 0.0)], s);
            }
        }
        Icon::Gear => {
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                let d = vec2(a.cos(), a.sin());
                p.line_segment([m + d * w * 0.3, m + d * w * 0.5], Stroke::new(2.4, c));
            }
            p.circle_stroke(m, w * 0.3, s);
            p.circle_stroke(m, w * 0.11, s);
        }
        Icon::Theater => {
            p.rect_stroke(r.shrink2(vec2(0.0, h * 0.12)), 2.0, s, egui::StrokeKind::Middle);
            p.line_segment([pos2(r.left() + w * 0.68, r.top() + h * 0.12), pos2(r.left() + w * 0.68, r.bottom() - h * 0.12)], s);
        }
        Icon::Full => {
            let k = w * 0.32;
            for (o, dx, dy) in [(r.left_top(), 1.0, 1.0), (r.right_top(), -1.0, 1.0), (r.left_bottom(), 1.0, -1.0), (r.right_bottom(), -1.0, -1.0)] {
                p.add(Shape::line(vec![o + vec2(0.0, dy * k), o, o + vec2(dx * k, 0.0)], s));
            }
        }
        Icon::Pin => {
            p.circle_filled(pos2(m.x, r.top() + h * 0.3), w * 0.26, c);
            p.line_segment([pos2(r.left() + w * 0.18, r.top() + h * 0.55), pos2(r.right() - w * 0.18, r.top() + h * 0.55)], s);
            p.line_segment([pos2(m.x, r.top() + h * 0.55), pos2(m.x, r.bottom())], s);
        }
        Icon::Video => {
            p.rect_stroke(Rect::from_min_size(r.min + vec2(0.0, h * 0.2), vec2(w * 0.66, h * 0.6)), 2.0, s, egui::StrokeKind::Middle);
            p.add(Shape::convex_polygon(vec![pos2(r.left() + w * 0.7, m.y), pos2(r.right(), r.top() + h * 0.25), pos2(r.right(), r.bottom() - h * 0.25)], c, Stroke::NONE));
        }
        Icon::Left | Icon::Right => {
            let d = if matches!(icon, Icon::Left) { -1.0 } else { 1.0 };
            let bar_x = if d < 0.0 { r.right() } else { r.left() };
            p.line_segment([pos2(bar_x, r.top()), pos2(bar_x, r.bottom())], s);
            let tip = pos2(m.x + d * w * 0.25, m.y);
            p.line_segment([pos2(m.x - d * w * 0.3, m.y), tip], s);
            p.add(Shape::line(vec![tip - vec2(d * h * 0.25, h * 0.25), tip, tip - vec2(d * h * 0.25, -h * 0.25)], s));
        }
        Icon::Smile => {
            p.circle_stroke(m, w * 0.46, s);
            p.circle_filled(m + vec2(-w * 0.16, -h * 0.1), 1.6, c);
            p.circle_filled(m + vec2(w * 0.16, -h * 0.1), 1.6, c);
            p.add(Shape::line(arc(m + vec2(0.0, h * 0.02), w * 0.24, 0.5, 2.64), s));
        }
        Icon::Search => {
            let o = m - vec2(w * 0.1, h * 0.1);
            p.circle_stroke(o, w * 0.32, Stroke::new(2.0, c));
            p.line_segment([o + vec2(w * 0.23, h * 0.23), r.right_bottom()], Stroke::new(2.4, c));
        }
        Icon::Person => {
            p.circle_stroke(pos2(m.x, r.top() + h * 0.3), w * 0.22, s);
            p.add(Shape::line(arc(pos2(m.x, r.bottom() + h * 0.05), w * 0.42, -std::f32::consts::PI, 0.0), s));
        }
        Icon::Reply => {
            let tip = pos2(r.left(), m.y - h * 0.1);
            p.add(Shape::line(vec![tip + vec2(w * 0.35, -h * 0.35), tip, tip + vec2(w * 0.35, h * 0.35)], s));
            p.add(Shape::line(vec![tip, pos2(r.right() - w * 0.25, tip.y), pos2(r.right(), r.bottom())], s));
        }
        Icon::Close => {
            p.line_segment([r.left_top(), r.right_bottom()], s);
            p.line_segment([r.right_top(), r.left_bottom()], s);
        }
        Icon::Chat => {
            let b = Rect::from_min_max(r.min, pos2(r.right(), r.bottom() - h * 0.25));
            p.rect_stroke(b, 2.0, s, egui::StrokeKind::Middle);
            p.add(Shape::line(vec![pos2(r.left() + w * 0.25, b.bottom()), pos2(r.left() + w * 0.25, r.bottom()), pos2(r.left() + w * 0.5, b.bottom())], s));
            p.line_segment([b.left_top() + vec2(w * 0.2, h * 0.25), b.right_top() + vec2(-w * 0.2, h * 0.25)], s);
        }
        Icon::Heart => {
            let pts: Vec<Pos2> = (0..=40)
                .map(|i| {
                    let t = i as f32 / 40.0 * std::f32::consts::TAU;
                    // corazón paramétrico, normalizado a la caja
                    let x = 16.0 * t.sin().powi(3);
                    let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
                    m + vec2(x / 34.0 * w, -y / 34.0 * h + h * 0.05)
                })
                .collect();
            let mut mesh = egui::Mesh::default();
            mesh.colored_vertex(m, c);
            for p2 in &pts {
                mesh.colored_vertex(*p2, c);
            }
            for i in 0..pts.len() as u32 - 1 {
                mesh.add_triangle(0, 1 + i, 2 + i);
            }
            p.add(mesh);
        }
        Icon::Share => {
            p.add(Shape::line(vec![pos2(r.left(), m.y), pos2(r.left(), r.bottom()), pos2(r.right(), r.bottom()), pos2(r.right(), m.y)], s));
            p.line_segment([pos2(m.x, r.top()), pos2(m.x, r.bottom() - h * 0.3)], s);
            p.add(Shape::line(vec![pos2(m.x - w * 0.25, r.top() + h * 0.25), pos2(m.x, r.top()), pos2(m.x + w * 0.25, r.top() + h * 0.25)], s));
        }
        Icon::Gauge => {
            let o = pos2(m.x, r.bottom() - h * 0.15);
            p.add(Shape::line(arc(o, w * 0.48, -std::f32::consts::PI, 0.0), s));
            p.line_segment([o, o + vec2(w * 0.28, -h * 0.3)], s);
        }
        Icon::Users => {
            p.circle_stroke(pos2(r.left() + w * 0.35, r.top() + h * 0.3), w * 0.18, s);
            p.add(Shape::line(arc(pos2(r.left() + w * 0.35, r.bottom()), w * 0.33, -std::f32::consts::PI, 0.0), s));
            p.add(Shape::line(vec![pos2(r.right() - w * 0.2, r.top() + h * 0.45), pos2(r.right() - w * 0.2, r.top() + h * 0.85)], s));
            p.add(Shape::line(vec![pos2(r.right() - w * 0.38, r.top() + h * 0.65), pos2(r.right() - w * 0.02, r.top() + h * 0.65)], s));
        }
        Icon::Refresh => {
            p.add(Shape::line(arc(m, w * 0.42, -1.2, 4.2), s));
            let tip = m + vec2((-1.2f32).cos(), (-1.2f32).sin()) * w * 0.42;
            p.add(Shape::line(vec![tip + vec2(-w * 0.25, -h * 0.05), tip, tip + vec2(-w * 0.05, h * 0.25)], s));
        }
        Icon::Info => {
            p.circle_filled(m, w * 0.5, c);
            p.line_segment([pos2(m.x, m.y - h * 0.05), pos2(m.x, m.y + h * 0.28)], Stroke::new(1.8, PANEL));
            p.circle_filled(pos2(m.x, m.y - h * 0.24), 1.2, PANEL);
        }
        Icon::Verified => {
            p.circle_filled(m, w * 0.5, c);
            p.add(Shape::line(vec![m + vec2(-w * 0.22, 0.0), m + vec2(-w * 0.05, h * 0.18), m + vec2(w * 0.24, -h * 0.17)], Stroke::new(1.8, Color32::WHITE)));
        }
        Icon::Bits => {
            // gema de Bits: rombo con una faceta
            let (l2, r2) = (r.left() + w * 0.08, r.right() - w * 0.08);
            p.add(Shape::closed_line(vec![pos2(m.x, r.top()), pos2(r2, m.y + h * 0.08), pos2(m.x, r.bottom()), pos2(l2, m.y + h * 0.08)], s));
            p.add(Shape::line(vec![pos2(l2, m.y + h * 0.08), pos2(m.x, m.y + h * 0.3), pos2(r2, m.y + h * 0.08)], s));
        }
        Icon::WinMin => {
            p.line_segment([pos2(r.left(), m.y), pos2(r.right(), m.y)], Stroke::new(1.0, c));
        }
        Icon::WinMax => {
            p.rect_stroke(r, 1.0, Stroke::new(1.0, c), egui::StrokeKind::Inside);
        }
        Icon::WinRestore => {
            let k = w * 0.22;
            p.rect_stroke(Rect::from_min_max(r.min + vec2(0.0, k), r.max - vec2(k, 0.0)), 1.0, Stroke::new(1.0, c), egui::StrokeKind::Inside);
            p.add(Shape::line(vec![pos2(r.left() + k, r.top() + k), pos2(r.left() + k, r.top()), pos2(r.right(), r.top()), pos2(r.right(), r.bottom() - k), pos2(r.right() - k, r.bottom() - k)], Stroke::new(1.0, c)));
        }
        Icon::Points => {
            p.circle_stroke(m, w * 0.45, Stroke::new(2.0, c));
            p.add(Shape::line(arc(m, w * 0.22, -std::f32::consts::PI, 0.4), Stroke::new(2.0, c)));
        }
        Icon::Bell => {
            // campana: cúpula, faldón y badajo
            let top = pos2(m.x, r.top() + h * 0.12);
            let mut pts = arc(pos2(m.x, r.top() + h * 0.42), w * 0.3, -std::f32::consts::PI, 0.0);
            pts.insert(0, pos2(r.left() + w * 0.2, r.top() + h * 0.72));
            pts.push(pos2(r.right() - w * 0.2, r.top() + h * 0.72));
            p.add(Shape::line(pts, s));
            p.line_segment([pos2(r.left() + w * 0.08, r.top() + h * 0.74), pos2(r.right() - w * 0.08, r.top() + h * 0.74)], s);
            p.line_segment([top, top + vec2(0.0, h * 0.02)], s);
            p.add(Shape::line(arc(pos2(m.x, r.top() + h * 0.82), w * 0.12, 0.0, std::f32::consts::PI), s));
        }
        Icon::Keyboard => {
            p.rect_stroke(r.shrink2(vec2(0.0, h * 0.2)), 2.0, s, egui::StrokeKind::Middle);
            for i in 0..4 {
                p.circle_filled(pos2(r.left() + w * (0.2 + 0.2 * i as f32), m.y - h * 0.08), 1.1, c);
            }
            p.line_segment([pos2(r.left() + w * 0.28, m.y + h * 0.14), pos2(r.right() - w * 0.28, m.y + h * 0.14)], s);
        }
    }
}

/// Misma marca que Windows y las notificaciones: Twitch con el distintivo lite en verde neón.
fn paint_logo(p: &Painter, r: Rect) {
    crate::branding::paint_logo(p, r);
}

fn icon_btn(ui: &mut Ui, icon: Icon, active: bool, tip: &str) -> Response {
    let (rect, r) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
    if r.hovered() {
        ui.painter().rect_filled(rect, 4.0, Color32::from_white_alpha(28));
    }
    paint_icon(ui.painter(), rect.shrink(8.0), icon, if active { PURPLE_L } else { TEXT });
    r.on_hover_text(tip)
}

/// Barra de volumen estilo Twitch: riel translúcido, relleno y bolita blancos. Arrastra o haz clic.
fn volume_bar(ui: &mut Ui, v: &mut f32) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(84.0, 24.0), Sense::click_and_drag());
    let track = Rect::from_center_size(r.center(), vec2(r.width() - 10.0, 4.0));
    let mut changed = false;
    if let Some(p) = resp.interact_pointer_pos().filter(|_| resp.dragged() || resp.clicked()) {
        let nv = ((p.x - track.left()) / track.width()).clamp(0.0, 1.0) * 100.0;
        changed = (nv - *v).abs() > 0.1;
        *v = nv.round();
    }
    let x = track.left() + track.width() * *v / 100.0;
    let pnt = ui.painter();
    pnt.rect_filled(track, 2.0, Color32::from_white_alpha(70));
    pnt.rect_filled(Rect::from_min_max(track.min, pos2(x, track.max.y)), 2.0, Color32::WHITE);
    pnt.circle_filled(pos2(x, track.center().y), if resp.hovered() || resp.dragged() { 7.0 } else { 6.0 }, Color32::WHITE);
    changed
}

/// Fila de menú como las de twitch.tv: ícono, texto y "↗" si abre la web.
fn menu_row(ui: &mut Ui, icon: Icon, text: &str, web: bool) -> Response {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, 4.0, HOVER);
    }
    paint_icon(ui.painter(), Rect::from_center_size(pos2(r.left() + 16.0, r.center().y), vec2(16.0, 16.0)), icon, TEXT);
    ui.painter().text(pos2(r.left() + 36.0, r.center().y), Align2::LEFT_CENTER, text, FontId::proportional(14.0), TEXT);
    if web {
        ui.painter().text(pos2(r.right() - 10.0, r.center().y), Align2::RIGHT_CENTER, "↗", FontId::proportional(13.0), DIM);
    }
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    if web { resp.on_hover_text("Se abre en twitch.tv") } else { resp }
}

fn menu_sep(ui: &mut Ui) {
    ui.add_space(6.0);
    let y = ui.cursor().top();
    ui.painter().line_segment([pos2(ui.max_rect().left(), y), pos2(ui.max_rect().right(), y)], Stroke::new(1.0, LINE));
    ui.add_space(7.0);
}

/// Encabezado de subpágina de menú: flecha para volver y título centrado. Devuelve si se pidió volver.
fn page_head(ui: &mut Ui, title: &str) -> bool {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
    let back = Rect::from_min_size(pos2(r.left(), r.center().y - 15.0), vec2(30.0, 30.0));
    let b = ui.put(back, |ui: &mut Ui| icon_btn(ui, Icon::Left, false, "Volver"));
    ui.painter().text(r.center(), Align2::CENTER_CENTER, title, semi(14.0), TEXT);
    menu_sep(ui);
    b.clicked()
}

/// Texto de ayuda gris, como las notas de los menús de twitch.tv.
fn hint(ui: &mut Ui, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).color(DIM).small()).wrap());
    ui.add_space(6.0);
}

/// Fila con interruptor (switch) como los de twitch.tv. Devuelve si cambió.
fn toggle_row(ui: &mut Ui, text: &str, on: &mut bool) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
    let enabled = ui.is_enabled();
    let p = ui.painter();
    if resp.hovered() && enabled {
        p.rect_filled(r, 4.0, HOVER);
    }
    p.text(pos2(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, text, FontId::proportional(13.5), if enabled { TEXT } else { DIM });
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.12);
    let sw = Rect::from_center_size(pos2(r.right() - 24.0, r.center().y), vec2(34.0, 18.0));
    p.rect_filled(sw, 9.0, Color32::from_rgb(60, 60, 68).lerp_to_gamma(PURPLE, t));
    p.circle_filled(pos2(sw.left() + 9.0 + 16.0 * t, sw.center().y), 6.0, TEXT);
    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && enabled {
        *on = !*on;
        return true;
    }
    false
}

/// Fila con botón de opción (una sola elegida). Devuelve si se eligió.
fn radio_row(ui: &mut Ui, text: &str, on: bool) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(r, 4.0, HOVER);
    }
    let c = pos2(r.left() + 16.0, r.center().y);
    p.circle_stroke(c, 7.5, Stroke::new(1.5, if on { PURPLE_L } else { DIM }));
    if on {
        p.circle_filled(c, 4.0, PURPLE_L);
    }
    p.text(pos2(r.left() + 32.0, r.center().y), Align2::LEFT_CENTER, text, FontId::proportional(13.5), TEXT);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !on
}

/// Fila que abre una subpágina: texto, valor actual en gris y flecha.
fn nav_row(ui: &mut Ui, icon: Option<Icon>, text: &str, value: &str) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(r, 4.0, HOVER);
    }
    if let Some(i) = icon {
        paint_icon(p, Rect::from_center_size(pos2(r.left() + 16.0, r.center().y), vec2(16.0, 16.0)), i, TEXT);
    }
    p.text(pos2(r.left() + if icon.is_some() { 36.0 } else { 8.0 }, r.center().y), Align2::LEFT_CENTER, text, FontId::proportional(14.0), TEXT);
    paint_icon(p, Rect::from_center_size(pos2(r.right() - 14.0, r.center().y), vec2(10.0, 10.0)), Icon::Right, DIM);
    p.text(pos2(r.right() - 26.0, r.center().y), Align2::RIGHT_CENTER, value, FontId::proportional(12.5), DIM);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

fn purple_btn(ui: &mut Ui, text: &str) -> Response {
    ui.add(egui::Button::new(RichText::new(text).font(semi(14.0)).color(Color32::WHITE)).fill(PURPLE).corner_radius(16).min_size(vec2(0.0, 32.0)))
}

// ---------- Imágenes ----------

/// Rect de `h` de alto con la proporción del frame, centrado en `area`.
fn fit(area: Rect, f: &img::Frame) -> Rect {
    let s = vec2(area.height() * f.size.x / f.size.y, area.height());
    Rect::from_center_size(area.center(), s)
}

/// Imagen del store a `h` puntos de alto; mientras carga reserva un hueco cuadrado.
fn image(ui: &mut Ui, store: &img::Store, id: u32, h: f32) -> (Response, bool) {
    match store.get(id) {
        img::Look::Ready(f) => {
            let (r, resp) = ui.allocate_exact_size(vec2(h * f.size.x / f.size.y, h), Sense::hover());
            ui.painter().image(f.tex, r, f.uv, Color32::WHITE);
            (resp, false)
        }
        img::Look::Pending => (ui.allocate_exact_size(vec2(h, h), Sense::hover()).1, false),
        img::Look::Failed => (ui.allocate_exact_size(vec2(0.0, h), Sense::hover()).1, true),
    }
}

fn avatar(ui: &Ui, store: &img::Store, url: &str, rect: Rect, gray: bool) {
    let px = (rect.height() * ui.ctx().pixels_per_point()).round() as u32;
    match (!url.is_empty()).then(|| store.get(store.id(url, px))) {
        Some(img::Look::Ready(f)) => {
            let tint = if gray { Color32::from_gray(110) } else { Color32::WHITE };
            egui::Image::from_texture(SizedTexture::new(f.tex, rect.size())).uv(f.uv).corner_radius(rect.height() / 2.0).tint(tint).paint_at(ui, rect);
        }
        _ => {
            ui.painter().circle_filled(rect.center(), rect.height() / 2.0, HOVER);
        }
    }
}

fn text_trunc(p: &Painter, pos: Pos2, text: &str, font: FontId, color: Color32, max_w: f32) {
    let mut job = LayoutJob::simple_singleline(text.to_string(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_w.max(1.0));
    let g = p.layout_job(job);
    p.galley(pos, g, color);
}

/// Una línea con emoji a color (Twemoji) en vez de cuadros; corta con "…" al llegar a `max_w`.
fn emoji_line(ui: &Ui, store: &img::Store, p: &Painter, pos: Pos2, text: &str, font: FontId, color: Color32, max_w: f32) {
    let h = font.size * 1.2;
    let px = (h * ui.ctx().pixels_per_point()).round() as u32;
    let mut x = pos.x;
    let end = pos.x + max_w;
    for (is_emoji, s) in img::split_emoji(text) {
        if is_emoji {
            if x + h > end {
                return;
            }
            if let img::Look::Ready(f) = store.get(store.id(&img::twemoji_url(&s), px)) {
                p.image(f.tex, Rect::from_min_size(pos2(x, pos.y), vec2(h, h)), f.uv, Color32::WHITE);
            }
            x += h + 2.0;
        } else {
            let g = p.layout_no_wrap(s.clone(), font.clone(), color);
            if x + g.size().x > end {
                return text_trunc(p, pos2(x, pos.y), &s, font, color, end - x);
            }
            x += g.size().x;
            p.galley(pos2(x - g.size().x, pos.y), g, color);
        }
    }
}

pub fn fmt_num(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    out
}

fn live_pill(p: &Painter, center: Pos2) {
    let g = p.layout_no_wrap("EN DIRECTO".into(), semi(10.5), Color32::WHITE);
    let r = Rect::from_center_size(center, g.size() + vec2(8.0, 2.0));
    p.rect_filled(r, 3.0, RED);
    p.galley(r.min + vec2(4.0, 1.0), g, Color32::WHITE);
}

// ---------- Pantalla ----------

/// "12,4 mil" como lo escribe Twitch en español.
fn short_num(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=999_999 => format!("{:.1} mil", n as f32 / 1000.0).replace('.', ",").replace(",0 ", " "),
        _ => format!("{:.1} M", n as f32 / 1_000_000.0).replace('.', ",").replace(",0 ", " "),
    }
}

fn clickable_text(ui: &mut Ui, text: RichText) -> Response {
    ui.add(egui::Label::new(text).sense(Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Radio de esquina de tarjetas, miniaturas y paneles.
const R: f32 = 8.0;

/// Efecto hover de las tarjetas de twitch.tv: la imagen se levanta hacia arriba a la derecha (con
/// easing de 120 ms) y atrás asoma la "L" morada, aquí con esquinas redondeadas. Devuelve el rect a pintar.
fn hover_lift(ui: &Ui, r: Rect, id: egui::Id, hovered: bool) -> Rect {
    let t = ui.ctx().animate_bool_with_time(id.with("lift"), hovered, 0.12);
    let k = 6.0 * t;
    if k < 0.3 {
        return r;
    }
    ui.painter().rect_filled(r, R, PURPLE.gamma_multiply(t));
    r.translate(vec2(k, -k))
}

/// Imagen con esquinas redondeadas (o el placeholder del tamaño final mientras carga).
fn rounded_img(ui: &Ui, store: &img::Store, url: &str, r: Rect, radius: impl Into<egui::CornerRadius> + Copy, tint: Color32) {
    let px = (r.height() * ui.ctx().pixels_per_point()).round() as u32;
    match (!url.is_empty()).then(|| store.get(store.id(url, px))) {
        Some(img::Look::Ready(f)) => egui::Image::from_texture(SizedTexture::new(f.tex, r.size())).uv(f.uv).corner_radius(radius).tint(tint).paint_at(ui, r),
        _ => {
            ui.painter().rect_filled(r, radius, SIDE);
        }
    }
}

/// Pastillas de tags como en twitch.tv; se cortan cuando ya no caben en `max_w`.
fn tag_pills(p: &Painter, pos: Pos2, tags: &[String], max_w: f32) {
    let mut x = pos.x;
    for t in tags {
        let g = p.layout_no_wrap(t.clone(), semi(12.0), TEXT);
        let pill = Rect::from_min_size(pos2(x, pos.y), g.size() + vec2(16.0, 4.0));
        if pill.right() > pos.x + max_w {
            break;
        }
        p.rect_filled(pill, 10.0, Color32::from_rgb(47, 47, 53));
        p.galley(pill.min + vec2(8.0, 2.0), g, TEXT);
        x = pill.right() + 6.0;
    }
}

/// sort pills — devuelve true si cambió
fn sort_pills(ui: &mut Ui, sel: &mut u8) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_space(30.0);
        for (i, label) in ["Más espectadores", "Menos espectadores", "Recientes", "Recomendados"].iter().enumerate() {
            let on = *sel as usize == i;
            let (bg, fg) = if on { (PURPLE, Color32::WHITE) } else { (SIDE, DIM) };
            if ui.add(egui::Button::new(RichText::new(*label).font(FontId::proportional(13.0)).color(fg)).fill(bg).corner_radius(12)).clicked() && !on {
                *sel = i as u8;
                changed = true;
            }
        }
    });
    ui.add_space(8.0);
    changed
}

const LANGUAGES: &[(&str, &str)] = &[
    ("", "Todos"), ("ES", "Español"), ("EN", "English"), ("PT", "Português"),
    ("FR", "Français"), ("DE", "Deutsch"), ("IT", "Italiano"), ("RU", "Русский"),
    ("JA", "日本語"), ("KO", "한국어"), ("ZH", "中文"), ("AR", "العربية"),
    ("TR", "Türkçe"), ("PL", "Polski"), ("NL", "Nederlands"), ("SV", "Svenska"),
    ("DA", "Dansk"), ("NO", "Norsk"), ("FI", "Suomi"), ("HU", "Magyar"),
    ("CS", "Čeština"), ("SK", "Slovenčina"), ("RO", "Română"), ("BG", "Български"),
    ("UK", "Українська"), ("EL", "Ελληνικά"), ("TH", "ไทย"), ("VI", "Tiếng Việt"),
    ("ID", "Bahasa"), ("HI", "हिन्दी"), ("TL", "Filipino"),
];

/// lang dropdown — devuelve true si cambió
fn lang_pills(ui: &mut Ui, sel: &mut String) -> bool {
    let mut changed = false;
    let current = LANGUAGES.iter().find(|(c, _)| *c == sel.as_str()).map(|(_, l)| *l).unwrap_or("Todos");
    ui.horizontal(|ui| {
        ui.add_space(30.0);
        ui.label(RichText::new("Idioma").font(FontId::proportional(13.0)).color(DIM));
        egui::ComboBox::from_id_salt("lang")
            .selected_text(RichText::new(current).font(FontId::proportional(13.0)))
            .width(130.0)
            .show_ui(ui, |ui| {
                for (code, label) in LANGUAGES {
                    if ui.selectable_label(sel.as_str() == *code, *label).clicked() {
                        *sel = code.to_string();
                        changed = true;
                    }
                }
            });
    });
    changed
}

/// tag chips — devuelve true si la selección cambió
fn tag_chips(ui: &mut Ui, tags: &[String], sel: &mut String) -> bool {
    if tags.is_empty() { return false; }
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.add_space(30.0);
        ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
        if !sel.is_empty() {
            if ui.add(egui::Button::new(RichText::new("✕ Limpiar").font(FontId::proportional(12.0)).color(DIM)).fill(HOVER).corner_radius(10)).clicked() {
                sel.clear();
                changed = true;
            }
        }
        for t in tags.iter().take(20) {
            let on = sel == t;
            let (bg, fg) = if on { (PURPLE, Color32::WHITE) } else { (SIDE, DIM) };
            if ui.add(egui::Button::new(RichText::new(t).font(FontId::proportional(12.0)).color(fg)).fill(bg).corner_radius(10)).clicked() {
                if on { sel.clear(); } else { *sel = t.clone(); }
                changed = true;
            }
        }
    });
    ui.add_space(8.0);
    changed
}

/// Tarjeta de directo como en el directorio de Twitch: miniatura con EN DIRECTO y espectadores;
/// debajo avatar, título, canal, categoría y tags.
fn stream_card(ui: &mut Ui, store: &img::Store, l: &Live, w: f32) -> bool {
    let th = w * 9.0 / 16.0;
    let (r, resp) = ui.allocate_exact_size(vec2(w, th + 96.0), Sense::click());
    let thumb = Rect::from_min_size(r.min, vec2(w, th));
    let shown = hover_lift(ui, thumb, resp.id, resp.hovered());
    rounded_img(ui, store, &l.preview, shown, R, Color32::WHITE);
    let p = ui.painter();
    let g = p.layout_no_wrap("EN DIRECTO".into(), semi(12.5), Color32::WHITE);
    let lr = Rect::from_min_size(shown.min + vec2(10.0, 10.0), g.size() + vec2(10.0, 2.0));
    p.rect_filled(lr, 4.0, RED);
    p.galley(lr.min + vec2(5.0, 1.0), g, Color32::WHITE);
    if let Some(v) = l.viewers {
        let g = p.layout_no_wrap(format!("{} espectadores", short_num(v)), FontId::proportional(12.5), Color32::WHITE);
        let br = Rect::from_min_size(shown.left_bottom() + vec2(10.0, -28.0), g.size() + vec2(10.0, 4.0));
        p.rect_filled(br, 4.0, Color32::from_black_alpha(190));
        p.galley(br.min + vec2(5.0, 2.0), g, Color32::WHITE);
    }
    let a = Rect::from_min_size(pos2(r.left(), thumb.bottom() + 10.0), vec2(40.0, 40.0));
    avatar(ui, store, &l.avatar, a, false);
    let tx = a.right() + 10.0;
    let tw = r.right() - tx;
    let p = ui.painter();
    emoji_line(ui, store, p, pos2(tx, thumb.bottom() + 8.0), &l.title, semi(14.0), if resp.hovered() { PURPLE_L } else { TEXT }, tw);
    text_trunc(p, pos2(tx, thumb.bottom() + 29.0), &l.name, FontId::proportional(13.5), DIM, tw);
    text_trunc(p, pos2(tx, thumb.bottom() + 47.0), &l.game, FontId::proportional(13.5), DIM, tw);
    tag_pills(p, pos2(tx, thumb.bottom() + 70.0), &l.tags, tw);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Rejilla responsiva de tarjetas; devuelve el login al que se le dio clic.
fn stream_grid(ui: &mut Ui, store: &img::Store, list: &[Live]) -> Option<String> {
    let mut go = None;
    let avail = ui.available_width() - 60.0;
    let cols = (avail / 300.0).floor().max(1.0) as usize;
    let w = (avail - 20.0 * (cols as f32 - 1.0)) / cols as f32;
    for row in list.chunks(cols) {
        ui.horizontal(|ui| {
            ui.add_space(30.0);
            ui.spacing_mut().item_spacing.x = 20.0;
            for l in row {
                if stream_card(ui, store, l, w) {
                    go = Some(l.login.clone());
                }
            }
        });
        ui.add_space(14.0);
    }
    go
}

fn page_title(ui: &mut Ui, t: &str) {
    ui.add_space(28.0);
    ui.horizontal(|ui| {
        ui.add_space(30.0);
        ui.label(RichText::new(t).font(semi(36.0)).color(TEXT));
    });
    ui.add_space(10.0);
}

fn section_title(ui: &mut Ui, t: &str) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(30.0);
        ui.label(RichText::new(t).font(semi(18.0)).color(TEXT));
    });
    ui.add_space(10.0);
}

/// Pestañas de página como las de twitch.tv (texto con subrayado morado en la activa).
fn tabs(ui: &mut Ui, labels: &[&str], sel: &mut u8) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_space(30.0);
        ui.spacing_mut().item_spacing.x = 22.0;
        let mut active = Rect::NOTHING;
        for (i, l) in labels.iter().enumerate() {
            let on = *sel as usize == i;
            let r = clickable_text(ui, RichText::new(*l).font(semi(15.0)).color(if on { PURPLE_L } else { TEXT }));
            if on {
                active = r.rect;
            }
            if r.clicked() && !on {
                *sel = i as u8;
                changed = true;
            }
        }
        // el subrayado se desliza a la pestaña nueva (180 ms con easing)
        if active.is_positive() {
            let id = egui::Id::unique(("tabs", labels[0]));
            let ctx = ui.ctx();
            let x0 = ctx.animate_value_with_time(id.with(0), active.left(), 0.18);
            let x1 = ctx.animate_value_with_time(id.with(1), active.right(), 0.18);
            let y = active.bottom() + 6.0;
            ui.painter().line_segment([pos2(x0, y), pos2(x1, y)], Stroke::new(2.0, PURPLE_L));
        }
    });
    let y = ui.cursor().top() + 6.0;
    ui.painter().line_segment([pos2(ui.max_rect().left() + 30.0, y), pos2(ui.max_rect().right() - 30.0, y)], Stroke::new(1.0, LINE));
    ui.add_space(24.0);
    changed
}

impl App {
    pub fn draw(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        let gl = frame.gl().cloned();
        if let Some(gl) = &gl { self.prepare_handoff(gl, frame); }
        let on_chan = self.page == Page::Channel && !self.chan.is_empty();
        let fullscreen = ui.input(|i| i.viewport().fullscreen.unwrap_or(false));
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            if fullscreen {
                ui.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            }
            self.theater = false;
            self.card = None;
        }
        // Arrastrar un .txt con la línea de cookies de twitch.tv a la ventana = iniciar sesión.
        let dropped: Vec<_> = ui.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        for path in dropped {
            if let Ok(raw) = std::fs::read_to_string(path) {
                if raw.contains("auth-token=") {
                    crate::twitch::import_web(&self.net, raw);
                }
            }
        }
        let chrome = !(on_chan && (fullscreen || self.theater));
        if chrome {
            self.top_bar(ui, gl.as_deref());
            self.sidebar(ui);
        }
        // Chat flotante solo si el video queda de buen tamaño; si no, pasa a lateral para no desaparecer.
        let rest = ui.available_rect_before_wrap();
        let est_vh = if chrome { (rest.width() * 9.0 / 16.0).min(rest.height() - 86.0) } else { rest.height() };
        let popout = on_chan && self.cfg.chat_popout;
        let overlay = self.cfg.chat_overlay && self.cfg.video && est_vh > 300.0 && !popout;
        if popout {
            self.chat_popout(ui);
        }
        if on_chan && !fullscreen && self.chat_open && !overlay && !popout {
            // Chat lateral, integrado como en twitch.tv; a la izquierda o derecha del video
            let panel = if self.cfg.chat_left { egui::Panel::left("chat") } else { egui::Panel::right("chat") };
            // tope de ancho: un mensaje sin espacios no debe empujar el panel y achicar el video
            panel.resizable(true).default_size(340.0).min_size(260.0).max_size((rest.width() * 0.45).max(300.0)).frame(egui::Frame::new().fill(PANEL).stroke(Stroke::new(1.0, LINE))).show(ui, |ui| self.chat(ui, false));
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG)).show(ui, |ui| {
            if !on_chan {
                return self.browse(ui);
            }
            let avail = ui.available_rect_before_wrap();
            // angosto: la info del canal usa dos renglones para no encimarse
            let info_h = if !chrome { 0.0 } else if avail.width() < COMPACT_W { 162.0 } else { 86.0 };
            let vh = match (self.cfg.video, chrome) {
                (false, _) => 0.0,
                (true, false) => avail.height(),
                (true, true) => (avail.width() * 9.0 / 16.0).min(avail.height() - info_h).max(0.0),
            };
            let vrect = Rect::from_min_size(avail.min, vec2(avail.width(), vh));
            if self.cfg.video {
                self.player(ui, frame, vrect);
            }
            if overlay && self.chat_open {
                self.live_chat(ui, vrect);
            }
            if chrome {
                let irect = Rect::from_min_size(pos2(avail.min.x, vrect.max.y), vec2(avail.width(), info_h));
                self.info_bar(ui, irect);
                let below = Rect::from_min_max(pos2(avail.min.x + 20.0, irect.max.y + 6.0), pos2(avail.max.x - 20.0, avail.max.y));
                if below.height() > 60.0 {
                    self.about(ui, below, gl.as_deref());
                }
            }
            if !self.chat_open && !fullscreen && !popout {
                let (x, icon) = if self.cfg.chat_left { (avail.left() + 6.0, Icon::Right) } else { (avail.right() - 36.0, Icon::Left) };
                let r = Rect::from_min_size(pos2(x, avail.top() + 6.0), vec2(30.0, 30.0));
                if ui.put(r, |ui: &mut Ui| icon_btn(ui, icon, false, "Mostrar chat")).clicked() {
                    self.chat_open = true;
                }
            }
        });
        if on_chan && self.theater && !fullscreen {
            self.mini_bar(ui);
        }
        self.overlays(ui);
        resize_edges(ui);
    }

    /// "Chat emergente": el mismo chat en su propia ventana nativa (otro viewport de egui, misma app y
    /// mismo hilo de IRC: no cuesta otra conexión). Cerrarla lo regresa a la ventana principal.
    fn chat_popout(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let title = format!("TwitchLite · Chat de {}", if self.info.name.is_empty() { &self.chan } else { &self.info.name });
        let vb = egui::ViewportBuilder::default().with_title(title).with_icon(crate::branding::window_icon()).with_inner_size([360.0, 680.0]).with_min_inner_size([260.0, 320.0]).with_always_on_top();
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("chat_popout"), vb, |ui, _| {
            self.chat_open = true;
            self.chat(ui, false);
            if ui.input(|i| i.viewport().close_requested()) {
                self.cfg.chat_popout = false;
            }
        });
    }

    /// Chat flotante estilo TikTok/LinkedIn Live: sin caja. Los mensajes se apilan desde abajo sobre
    /// un degradado oscuro, entran subiendo y los viejos se desvanecen hacia arriba. Abajo, una
    /// píldora translúcida para escribir. Los controles aparecen solo con el mouse encima.
    fn live_chat(&mut self, ui: &mut Ui, vrect: Rect) {
        let w = (vrect.width() * 0.32).clamp(300.0, 420.0);
        let x = if self.cfg.chat_left { vrect.left() + 16.0 } else { vrect.right() - w - 16.0 };
        let bottom = vrect.bottom() - 96.0; // libre la barra de controles, el "EN DIRECTO" y la línea de puntos
        let input_r = Rect::from_min_size(pos2(x, bottom - 40.0), vec2(w, 40.0));
        let area = Rect::from_min_max(pos2(x, vrect.top() + vrect.height() * 0.3), pos2(x + w, input_r.top() - 10.0));
        let zone = Rect::from_min_max(pos2(x - 16.0, area.top() - 40.0), pos2(x + w + 16.0, vrect.bottom()));
        // degradado: transparente arriba, oscuro abajo, solo detrás de la columna del chat
        let p = ui.painter_at(vrect);
        let mut mesh = egui::Mesh::default();
        let (z, a) = (Color32::TRANSPARENT, Color32::from_black_alpha(200));
        mesh.colored_vertex(zone.left_top(), z);
        mesh.colored_vertex(zone.right_top(), z);
        mesh.colored_vertex(zone.right_bottom(), a);
        mesh.colored_vertex(zone.left_bottom(), a);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        p.add(mesh);
        let hover = ui.rect_contains_pointer(zone);
        ui.interact(Rect::from_min_max(area.min, input_r.max), egui::Id::unique("live_chat_bg"), Sense::click()); // un clic aquí no pausa el video

        // mensajes de abajo hacia arriba con los altos medidos; los que no caben no se arman
        let gap = 4.0;
        let mut start = self.msgs.len();
        let mut total = 0.0;
        while start > 0 && total + self.msgs[start - 1].h + gap <= area.height() {
            total += self.msgs[start - 1].h + gap;
            start -= 1;
        }
        let logged = self.me.is_some();
        let mut y = area.bottom() - total;
        let mut act = None;
        let mut animating = false;
        for m in self.msgs.range_mut(start..) {
            let age = m.born.elapsed().as_secs_f32();
            let t = (age / 0.25).min(1.0); // entrada de 250 ms
            animating |= t < 1.0;
            let slide = (1.0 - t) * 18.0;
            // desvanecido hacia arriba: opaco en el tercio de abajo, tenue arriba
            let fade = ((y - area.top()) / (area.height() * 0.45)).clamp(0.2, 1.0);
            let mut mu = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(pos2(x, y + slide), vec2(w, 600.0))));
            mu.set_opacity(fade * t.max(0.15));
            if let Some(a) = draw_msg(&mut mu, m, &self.store, &self.cfg, logged) {
                act = Some(a);
            }
            m.h = mu.min_rect().height().max(1.0);
            y += m.h + gap;
        }
        if animating {
            ui.ctx().request_repaint(); // solo durante los 250 ms de entrada
        }
        match act {
            Some(MsgAct::Name(login, pos)) => {
                self.card = None;
                self.card_pos = pos;
                crate::twitch::load_card(&self.net, login);
            }
            Some(MsgAct::Reply(id, name, body)) => self.reply = Some((id, name, body)),
            None => {}
        }

        // píldora para escribir
        let id = egui::Id::unique("chat_input");
        let focused = ui.memory(|m| m.has_focus(id));
        p.rect_filled(input_r, 20.0, Color32::from_black_alpha(if focused { 200 } else { 140 }));
        p.rect_stroke(input_r, 20.0, Stroke::new(1.0, if focused { PURPLE_L } else { Color32::from_white_alpha(45) }), egui::StrokeKind::Inside);
        let mut c = ui.new_child(UiBuilder::new().max_rect(input_r.shrink2(vec2(14.0, 4.0))).layout(Layout::left_to_right(Align::Center)));
        c.spacing_mut().item_spacing.x = 4.0;
        let hint = match (&self.reply, logged) {
            (Some((_, n, _)), _) => format!("Respondiendo a @{n}"),
            (None, true) => "Enviar un mensaje".into(),
            (None, false) => "Inicia sesión para chatear".into(),
        };
        let tab = focused && c.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab));
        let r = c.add_enabled(logged, egui::TextEdit::singleline(&mut self.input).id(id).hint_text(hint).char_limit(500).frame(egui::Frame::NONE).desired_width(c.available_width() - 70.0));
        if r.changed() {
            self.tab = None;
        }
        if r.lost_focus() && c.input(|i| i.key_pressed(Key::Enter)) {
            self.send();
            r.request_focus();
        }
        if tab && self.complete() {
            if let Some(mut st) = egui::TextEdit::load_state(c.ctx(), id) {
                st.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(self.input.chars().count()))));
                st.store(c.ctx(), id);
            }
        }
        if icon_btn(&mut c, Icon::Smile, self.picker, "Emotes").clicked() {
            self.picker = !self.picker;
        }
        if let Some(b) = self.points {
            p.text(pos2(input_r.left() + 8.0, input_r.bottom() + 12.0), Align2::LEFT_CENTER, format!("{} puntos", fmt_num(b)), FontId::proportional(12.0), Color32::from_white_alpha(170));
        }
        if self.picker {
            let mut insert = None;
            egui::Area::new(egui::Id::unique("live_picker")).order(egui::Order::Foreground).pivot(Align2::LEFT_BOTTOM).fixed_pos(input_r.left_top() - vec2(0.0, 8.0)).show(ui.ctx(), |ui| {
                ui.set_width(w);
                self.picker(ui, &mut insert);
            });
            if let Some(name) = insert {
                if !self.input.is_empty() && !self.input.ends_with(' ') {
                    self.input.push(' ');
                }
                self.input.push_str(&name);
                self.input.push(' ');
            }
        }

        // controles al pasar el mouse: cambiar de lado, pasar a lateral, ajustes
        if hover {
            let row = Rect::from_min_size(pos2(x + w - 102.0, area.top() - 36.0), vec2(102.0, 32.0));
            p.rect_filled(row, 16.0, Color32::from_black_alpha(160));
            let mut b = ui.new_child(UiBuilder::new().max_rect(row.shrink2(vec2(4.0, 1.0))).layout(Layout::left_to_right(Align::Center)));
            b.spacing_mut().item_spacing.x = 2.0;
            let side = if self.cfg.chat_left { Icon::Right } else { Icon::Left };
            if icon_btn(&mut b, side, false, "Cambiar de lado").clicked() {
                self.cfg.chat_left = !self.cfg.chat_left;
                self.cfg.save();
            }
            if icon_btn(&mut b, Icon::Theater, false, "Pasar al chat lateral").clicked() {
                self.cfg.chat_overlay = false;
                self.cfg.save();
            }
            let g = icon_btn(&mut b, Icon::Gear, self.chat_menu, "Ajustes del chat");
            if g.clicked() {
                self.chat_menu = !self.chat_menu;
            }
            if self.chat_menu {
                self.chat_settings(ui, g.rect);
            }
        } else if self.chat_menu {
            let g = Rect::from_min_size(pos2(x + w - 34.0, area.top() - 36.0), vec2(30.0, 30.0));
            self.chat_settings(ui, g);
        }
    }

    /// Modo mínimo (cine): sin barra de Twitch. Arriba queda una franja invisible para arrastrar la
    /// ventana; al pasar el mouse aparecen el nombre del canal y los controles de ventana.
    fn mini_bar(&mut self, ui: &mut Ui) {
        let screen = ui.ctx().content_rect();
        let r = Rect::from_min_size(screen.min, vec2(screen.width(), 40.0));
        let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        let drag = ui.interact(r, egui::Id::unique("mini_drag"), Sense::click_and_drag());
        if drag.drag_started_by(egui::PointerButton::Primary) {
            ui.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if drag.double_clicked() {
            ui.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
        }
        if !ui.rect_contains_pointer(r) {
            return;
        }
        let mut mesh = egui::Mesh::default();
        let (a, z) = (Color32::from_black_alpha(200), Color32::TRANSPARENT);
        mesh.colored_vertex(r.left_top(), a);
        mesh.colored_vertex(r.right_top(), a);
        mesh.colored_vertex(r.right_bottom() + vec2(0.0, 20.0), z);
        mesh.colored_vertex(r.left_bottom() + vec2(0.0, 20.0), z);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        ui.painter().add(mesh);
        let name = if self.info.name.is_empty() { self.chan.clone() } else { self.info.name.clone() };
        ui.painter().text(pos2(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, name, semi(13.5), Color32::WHITE);
        let mut c = ui.new_child(UiBuilder::new().max_rect(r).layout(Layout::right_to_left(Align::Center)));
        c.spacing_mut().item_spacing.x = 0.0;
        for (icon, tip) in [(Icon::Close, "Cerrar"), (if maximized { Icon::WinRestore } else { Icon::WinMax }, "Maximizar"), (Icon::WinMin, "Minimizar")] {
            let (br, b) = c.allocate_exact_size(vec2(44.0, 40.0), Sense::click());
            if b.hovered() {
                c.painter().rect_filled(br, 0.0, if matches!(icon, Icon::Close) { Color32::from_rgb(196, 43, 28) } else { Color32::from_white_alpha(25) });
            }
            paint_icon(c.painter(), Rect::from_center_size(br.center(), vec2(10.0, 10.0)), icon, Color32::WHITE);
            if b.on_hover_text(tip).clicked() {
                c.send_viewport_cmd(match icon {
                    Icon::Close => egui::ViewportCommand::Close,
                    Icon::WinMin => egui::ViewportCommand::Minimized(true),
                    _ => egui::ViewportCommand::Maximized(!maximized),
                });
            }
        }
        c.add_space(8.0);
        if icon_btn(&mut c, Icon::Theater, true, "Salir del modo mínimo (Esc)").clicked() {
            self.theater = false;
        }
    }

    /// Páginas sin canal: Siguiendo, Explorar y una categoría.
    fn browse(&mut self, ui: &mut Ui) {
        let mut go = None;
        let mut cat = None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| match self.page.clone() {
            Page::Following | Page::Channel => {
                page_title(ui, "Siguiendo");
                if self.me.is_none() {
                    ui.horizontal(|ui| {
                        ui.add_space(30.0);
                        ui.label(RichText::new("Inicia sesión para ver los canales que sigues.").color(DIM));
                        ui.add_space(12.0);
                        if purple_btn(ui, "Iniciar sesión").clicked() {
                            self.login_open = true;
                        }
                    });
                    ui.add_space(20.0);
                    section_title(ui, "Canales en directo");
                    if let Some(c) = stream_grid(ui, &self.store, &self.top) {
                        go = Some(c);
                    }
                    return;
                }
                tabs(ui, &["En directo", "Canales"], &mut self.follow_tab);
                if self.follow_tab == 0 {
                    let live: Vec<Live> = self.follows.iter().filter(|l| l.viewers.is_some()).cloned().collect();
                    if live.is_empty() {
                        section_title(ui, "Ninguno de los canales que sigues está en directo");
                    } else {
                        if let Some(c) = self.hero_carousel(ui, &live) {
                            go = Some(c);
                        }
                        section_title(ui, "Canales en directo que sigues");
                        // 2 filas y "Mostrar más", como la portada de twitch.tv
                        let cols = ((ui.available_width() - 60.0) / 300.0).floor().max(1.0) as usize;
                        let shown = if self.follow_more { live.len() } else { live.len().min(cols * 2) };
                        if let Some(c) = stream_grid(ui, &self.store, &live[..shown]) {
                            go = Some(c);
                        }
                        if live.len() > cols * 2 {
                            ui.vertical_centered(|ui| {
                                let t = if self.follow_more { "Mostrar menos" } else { "Mostrar más" };
                                if clickable_text(ui, RichText::new(t).font(semi(14.0)).color(PURPLE_L)).clicked() {
                                    self.follow_more = !self.follow_more;
                                }
                            });
                            ui.add_space(10.0);
                        }
                    }
                    section_title(ui, "Recomendados");
                    if let Some(c) = stream_grid(ui, &self.store, &self.top) {
                        go = Some(c);
                    }
                } else {
                    // Todos los que sigues, como la pestaña "Canales" de twitch.tv: avatar grande y estado
                    let avail = ui.available_width() - 60.0;
                    let cols = (avail / 200.0).floor().max(1.0) as usize;
                    let w = (avail - 16.0 * (cols as f32 - 1.0)) / cols as f32;
                    for row in self.follows.chunks(cols) {
                        ui.horizontal(|ui| {
                            ui.add_space(30.0);
                            ui.spacing_mut().item_spacing.x = 16.0;
                            for l in row {
                                let (r, resp) = ui.allocate_exact_size(vec2(w, 74.0), Sense::click());
                                ui.painter().rect_filled(r, 6.0, if resp.hovered() { HOVER } else { SIDE });
                                let live = l.viewers.is_some();
                                let a = Rect::from_min_size(r.min + vec2(12.0, 12.0), vec2(50.0, 50.0));
                                if live {
                                    ui.painter().circle_stroke(a.center(), 27.0, Stroke::new(2.0, RED));
                                }
                                avatar(ui, &self.store, &l.avatar, a, !live);
                                let p = ui.painter();
                                let tw = r.right() - a.right() - 22.0;
                                text_trunc(p, pos2(a.right() + 12.0, r.top() + 16.0), &l.name, semi(14.5), TEXT, tw);
                                let sub = match l.viewers {
                                    Some(v) => format!("{} · {}", l.game, short_num(v)),
                                    None => "Desconectado".into(),
                                };
                                text_trunc(p, pos2(a.right() + 12.0, r.top() + 38.0), &sub, FontId::proportional(13.0), if live { RED } else { DIM }, tw);
                                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    go = Some(l.login.clone());
                                }
                            }
                        });
                        ui.add_space(12.0);
                    }
                    ui.horizontal(|ui| {
                        ui.add_space(30.0);
                        ui.label(RichText::new("Twitch solo deja ver tus 100 follows más recientes desde fuera de su web.").color(DIM).small());
                    });
                }
            }
            Page::Explore => {
                page_title(ui, "Explorar");
                tabs(ui, &["Categorías", "Canales en directo"], &mut self.explore_tab);
                if self.explore_tab == 1 {
                    let s = sort_pills(ui, &mut self.explore_sort);
                    let l = lang_pills(ui, &mut self.explore_lang);
                    if s || l {
                        twitch::load_top(&self.net, sort_str(self.explore_sort), &self.explore_lang);
                    }
                    if let Some(c) = stream_grid(ui, &self.store, &self.top) {
                        go = Some(c);
                    }
                    return;
                }
                let avail = ui.available_width() - 60.0;
                let cols = (avail / 170.0).floor().max(1.0) as usize;
                let w = (avail - 14.0 * (cols as f32 - 1.0)) / cols as f32;
                for row in self.games.chunks(cols) {
                    ui.horizontal(|ui| {
                        ui.add_space(30.0);
                        ui.spacing_mut().item_spacing.x = 14.0;
                        for g in row {
                            let (r, resp) = ui.allocate_exact_size(vec2(w, w * 4.0 / 3.0 + 50.0), Sense::click());
                            let art = Rect::from_min_size(r.min, vec2(w, w * 4.0 / 3.0));
                            let shown = hover_lift(ui, art, resp.id, resp.hovered());
                            rounded_img(ui, &self.store, &g.art, shown, R, Color32::WHITE);
                            let name_c = if resp.hovered() { PURPLE_L } else { TEXT };
                            text_trunc(ui.painter(), pos2(r.left(), art.bottom() + 8.0), &g.name, semi(14.5), name_c, w);
                            text_trunc(ui.painter(), pos2(r.left(), art.bottom() + 28.0), &format!("{} espectadores", short_num(g.viewers)), FontId::proportional(13.0), DIM, w);
                            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                cat = Some(g.name.clone());
                            }
                        }
                    });
                    ui.add_space(16.0);
                }
            }
            Page::Category(g) => {
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    if icon_btn(ui, Icon::Left, false, "Volver a Explorar").clicked() {
                        cat = Some(String::new());
                    }
                });
                // header: nombre + total de espectadores en vivo
                ui.horizontal(|ui| {
                    ui.add_space(30.0);
                    ui.label(RichText::new(&g).font(semi(36.0)).color(TEXT));
                    if !self.cat_streams.1.is_empty() {
                        let total: u64 = self.cat_streams.1.iter().filter_map(|l| l.viewers).sum();
                        ui.add_space(12.0);
                        ui.label(RichText::new(format!("{} espectadores", short_num(total))).font(FontId::proportional(15.0)).color(DIM));
                    }
                });
                ui.add_space(10.0);
                let s = sort_pills(ui, &mut self.cat_sort);
                let l = lang_pills(ui, &mut self.cat_lang);
                if s || l {
                    self.cat_streams.1.clear();
                    twitch::load_streams(&self.net, g.clone(), sort_str(self.cat_sort), &self.cat_lang);
                }
                // tags de los streams como filtro
                let mut all_tags: Vec<String> = self.cat_streams.1.iter().flat_map(|l| l.tags.iter().cloned()).collect();
                all_tags.sort();
                all_tags.dedup();
                tag_chips(ui, &all_tags, &mut self.cat_tag);
                if self.cat_streams.1.is_empty() {
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        ui.label(RichText::new("Cargando…").color(DIM));
                    });
                }
                let filtered: Vec<Live> = if self.cat_tag.is_empty() {
                    self.cat_streams.1.clone()
                } else {
                    self.cat_streams.1.iter().filter(|l| l.tags.iter().any(|t| t == &self.cat_tag)).cloned().collect()
                };
                go = stream_grid(ui, &self.store, &filtered);
            }
            Page::Drops => {
                page_title(ui, "Drops y recompensas");
                section_title(ui, "Inventario");
                if self.drops.is_empty() {
                    ui.horizontal(|ui| {
                        ui.add_space(30.0);
                        ui.label(RichText::new("Aún no tienes drops, o se están cargando.").color(DIM));
                    });
                }
                // rejilla de recompensas: imagen, nombre y fecha
                let avail = ui.available_width() - 60.0;
                let cols = (avail / 180.0).floor().max(1.0) as usize;
                let w = (avail - 14.0 * (cols as f32 - 1.0)) / cols as f32;
                let px = (96.0 * ui.ctx().pixels_per_point()) as u32;
                for row in self.drops.chunks(cols) {
                    ui.horizontal(|ui| {
                        ui.add_space(30.0);
                        ui.spacing_mut().item_spacing.x = 14.0;
                        for d in row {
                            let (r, _) = ui.allocate_exact_size(vec2(w, 170.0), Sense::hover());
                            ui.painter().rect_filled(r, 10.0, SIDE);
                            let ir = Rect::from_center_size(pos2(r.center().x, r.top() + 62.0), vec2(96.0, 96.0));
                            if let img::Look::Ready(f) = self.store.get(self.store.id(&d.image, px)) {
                                ui.painter().image(f.tex, fit(ir, &f), f.uv, Color32::WHITE);
                            }
                            text_trunc(ui.painter(), pos2(r.left() + 12.0, r.top() + 120.0), &d.name, semi(13.5), TEXT, w - 24.0);
                            text_trunc(ui.painter(), pos2(r.left() + 12.0, r.top() + 142.0), &format!("Obtenido el {}", d.date), FontId::proportional(12.5), DIM, w - 24.0);
                        }
                    });
                    ui.add_space(14.0);
                }
            }
            Page::Subs => {
                page_title(ui, "Suscripciones");
                if self.subs.is_empty() {
                    ui.horizontal(|ui| {
                        ui.add_space(30.0);
                        ui.label(RichText::new("No tienes suscripciones activas, o se están cargando.").color(DIM));
                    });
                }
                for s in &self.subs {
                    ui.horizontal(|ui| {
                        ui.add_space(30.0);
                        let (r, resp) = ui.allocate_exact_size(vec2((ui.available_width() - 30.0).min(720.0), 72.0), Sense::click());
                        ui.painter().rect_filled(r, 10.0, if resp.hovered() { HOVER } else { SIDE });
                        avatar(ui, &self.store, &s.avatar, Rect::from_min_size(r.min + vec2(14.0, 12.0), vec2(48.0, 48.0)), false);
                        let p = ui.painter();
                        p.text(pos2(r.left() + 76.0, r.top() + 24.0), Align2::LEFT_CENTER, &s.name, semi(15.0), TEXT);
                        p.text(pos2(r.left() + 76.0, r.top() + 48.0), Align2::LEFT_CENTER, &s.ends, FontId::proportional(13.0), DIM);
                        let g = p.layout_no_wrap(s.tier.clone(), semi(12.5), TEXT);
                        let pill = Rect::from_min_size(pos2(r.right() - g.size().x - 30.0, r.center().y - 12.0), g.size() + vec2(16.0, 6.0));
                        p.rect_filled(pill, 12.0, PURPLE);
                        p.galley(pill.min + vec2(8.0, 3.0), g, TEXT);
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Ir al canal").clicked() {
                            go = Some(s.login.clone());
                        }
                    });
                    ui.add_space(10.0);
                }
            }
        });
        if let Some(c) = cat {
            self.go(if c.is_empty() { Page::Explore } else { Page::Category(c) });
        }
        if let Some(c) = go {
            self.join(&c);
        }
    }

    /// Carrusel destacado de Siguiendo, como el de la portada de twitch.tv: vista previa grande al
    /// centro con su ficha a la derecha y los vecinos más chicos y oscurecidos a los lados.
    /// Usa la imagen de vista previa, no video: un reproductor por tarjeta dispararía CPU y RAM.
    fn hero_carousel(&mut self, ui: &mut Ui, live: &[Live]) -> Option<String> {
        let n = live.len();
        let i = self.hero % n;
        let avail = ui.available_width() - 60.0;
        // Alto calculado para que todo quepa entero: vecina + 16 + video + ficha + 16 + vecina.
        // (16/9·h + 0.8·h + 2·0.8·16/9·h ≈ 5.42·h). Si no alcanza, sin vecinas.
        let with_sides = n > 1 && (avail - 32.0) / 5.42 >= 150.0;
        let h = if with_sides { ((avail - 32.0) / 5.42).min(320.0) } else { ((avail) / 2.6).clamp(120.0, 320.0) };
        let (cw, info_w) = (h * 16.0 / 9.0, h * 0.8);
        let area = Rect::from_min_size(ui.cursor().min + vec2(30.0, 6.0), vec2(avail, h + 12.0));
        ui.allocate_rect(area, Sense::hover());
        let left = area.center().x - (cw + info_w) / 2.0;
        // transición al cambiar: el destacado entra deslizándose 40 px con easing (250 ms)
        let e = egui::emath::easing::cubic_out((self.hero_t.elapsed().as_secs_f32() / 0.25).min(1.0));
        if e < 1.0 {
            ui.ctx().request_repaint();
        }
        let slide = vec2(self.hero_dir * 40.0 * (1.0 - e), 0.0);
        let main = Rect::from_min_size(pos2(left, area.top() + 6.0), vec2(cw, h)).translate(slide);
        let info = Rect::from_min_size(pos2(main.right(), main.top()), vec2(info_w, h));
        let mut go = None;
        // vecinas: 80% de alto, oscurecidas y redondeadas, enteras dentro del área (sin corte ni degradado)
        let sh = h * 0.8;
        let sw = sh * 16.0 / 9.0;
        let prev = Rect::from_min_size(pos2(left - 16.0 - sw, area.top() + 6.0 + (h - sh) / 2.0), vec2(sw, sh));
        let next = Rect::from_min_size(pos2(left + cw + info_w + 16.0, prev.top()), vec2(sw, sh));
        if with_sides {
            for (r, l) in [(prev, &live[(i + n - 1) % n]), (next, &live[(i + 1) % n])] {
                rounded_img(ui, &self.store, &l.preview, r, R, Color32::from_gray(70));
            }
        }
        let l = &live[i];
        let mut fade = ui.new_child(UiBuilder::new().max_rect(area));
        fade.set_opacity(e.max(0.2));
        rounded_img(&fade, &self.store, &l.preview, main, egui::CornerRadius { nw: R as u8, sw: R as u8, ne: 0, se: 0 }, Color32::WHITE);
        let p = fade.painter().clone();
        live_pill(&p, main.left_top() + vec2(48.0, 18.0));
        p.rect_filled(info, egui::CornerRadius { nw: 0, sw: 0, ne: R as u8, se: R as u8 }, PANEL);
        let a = Rect::from_min_size(info.min + vec2(14.0, 16.0), vec2(40.0, 40.0));
        avatar(ui, &self.store, &l.avatar, a, false);
        let tx = a.right() + 10.0;
        let tw = info.right() - tx - 10.0;
        text_trunc(&p, pos2(tx, info.top() + 16.0), &l.name, semi(15.0), PURPLE_L, tw);
        text_trunc(&p, pos2(tx, info.top() + 36.0), &l.game, FontId::proportional(13.0), PURPLE_L, tw);
        if let Some(v) = l.viewers {
            text_trunc(&p, pos2(info.left() + 14.0, info.top() + 66.0), &format!("{} espectadores", fmt_num(v)), FontId::proportional(13.0), TEXT, info_w - 28.0);
        }
        tag_pills(&p, pos2(info.left() + 14.0, info.top() + 90.0), &l.tags, info_w - 28.0);
        let mut job = LayoutJob::simple(l.title.clone(), FontId::proportional(13.0), DIM, info_w - 28.0);
        job.wrap.max_rows = 4;
        let g = p.layout_job(job);
        p.galley(pos2(info.left() + 14.0, info.top() + 120.0), g, DIM);
        let main_r = ui.interact(main.union(info), egui::Id::unique("hero_main"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        if main_r.clicked() {
            go = Some(l.login.clone());
        }
        let mut step = None;
        if n > 1 {
            if with_sides && ui.interact(prev, egui::Id::unique("hero_prev"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                step = Some(n - 1);
            }
            if with_sides && ui.interact(next, egui::Id::unique("hero_next"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                step = Some(1);
            }
            // flechas en las orillas
            for (x, icon, d) in [(area.left() + 22.0, Icon::Left, n - 1), (area.right() - 22.0, Icon::Right, 1)] {
                let c = Rect::from_center_size(pos2(x, main.center().y), vec2(40.0, 40.0));
                let r = ui.interact(c, egui::Id::unique(("hero_arrow", x as i32)), Sense::click());
                p.circle_filled(c.center(), 20.0, if r.hovered() { Color32::from_rgb(60, 60, 68) } else { Color32::from_black_alpha(170) });
                // chevron simple (< o >)
                let sgn = if matches!(icon, Icon::Left) { -1.0 } else { 1.0 };
                let m = c.center() + vec2(sgn * 2.0, 0.0);
                p.add(Shape::line(vec![m + vec2(-sgn * 5.0, -8.0), m + vec2(sgn * 4.0, 0.0), m + vec2(-sgn * 5.0, 8.0)], Stroke::new(2.2, TEXT)));
                if r.clicked() {
                    step = Some(d);
                }
            }
        }
        if let Some(d) = step {
            self.hero = (i + d) % n;
            self.hero_t = Instant::now();
            self.hero_dir = if d == 1 { 1.0 } else { -1.0 };
        }
        ui.add_space(14.0);
        go
    }

    fn top_bar(&mut self, ui: &mut Ui, gl: Option<&eframe::glow::Context>) {
        let mut search_rect = Rect::NOTHING;
        egui::Panel::top("top").exact_size(50.0).frame(egui::Frame::new().fill(PANEL).inner_margin(Margin { left: 10, right: 0, top: 0, bottom: 0 }).stroke(Stroke::new(1.0, Color32::BLACK))).show(ui, |ui| {
            // Ventana sin marco: esta barra hace de barra de título. El fondo se registra primero
            // para que los botones queden encima; arrastrar mueve la ventana, doble clic maximiza.
            let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
            let title = ui.interact(ui.max_rect(), egui::Id::unique("titlebar"), Sense::click_and_drag());
            if title.drag_started_by(egui::PointerButton::Primary) {
                ui.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if title.double_clicked() {
                ui.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }
            ui.horizontal_centered(|ui| {
                let wide_brand = ui.max_rect().width() >= 1100.0;
                let (r, logo) = ui.allocate_exact_size(vec2(if wide_brand { 130.0 } else { 40.0 }, 40.0), Sense::click());
                paint_logo(ui.painter(), Rect::from_min_size(r.min, vec2(40.0, 40.0)));
                if wide_brand {
                    ui.painter().text(pos2(r.left() + 46.0, r.center().y), Align2::LEFT_CENTER, "Twitch", semi(18.0), TEXT);
                    ui.painter().text(pos2(r.left() + 105.0, r.center().y + 4.0), Align2::LEFT_CENTER, "lite", semi(11.0), crate::branding::NEON);
                }
                if logo.on_hover_text("TwitchLite · Inicio").clicked() {
                    self.go(Page::Following);
                }
                ui.add_space(14.0);
                for (label, page) in [("Siguiendo", Page::Following), ("Explorar", Page::Explore)] {
                    let active = self.page == page || (matches!(self.page, Page::Category(_)) && page == Page::Explore);
                    let r = clickable_text(ui, RichText::new(label).font(semi(17.0)).color(if active { PURPLE_L } else { TEXT }));
                    if active {
                        ui.painter().line_segment([pos2(r.rect.left(), 47.0), pos2(r.rect.right(), 47.0)], Stroke::new(2.0, PURPLE_L));
                    }
                    if r.clicked() {
                        self.go(page);
                    }
                    ui.add_space(14.0);
                }
                let right_w = 370.0;
                let mid = (ui.available_width() - right_w).max(0.0);
                ui.allocate_ui_with_layout(vec2(mid, 36.0), Layout::left_to_right(Align::Center), |ui| {
                    let sw = mid.min(400.0);
                    ui.add_space(((mid - sw) / 2.0).max(0.0));
                    ui.spacing_mut().item_spacing.x = 0.0;
                    // caja como la de twitch.tv: borde gris, morado al escribir, botón de lupa pegado
                    let sid = egui::Id::unique("search");
                    let focused = ui.memory(|m| m.has_focus(sid));
                    let border = if focused { PURPLE_L } else { Color32::from_rgb(64, 64, 70) };
                    let fr = egui::Frame::new().fill(BG).stroke(Stroke::new(if focused { 2.0 } else { 1.0 }, border)).corner_radius(egui::CornerRadius { nw: 6, sw: 6, ne: 0, se: 0 }).inner_margin(Margin::symmetric(10, 7)).show(ui, |ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.search).id(sid).hint_text("Buscar").frame(egui::Frame::NONE).desired_width(sw - 58.0))
                    });
                    let r = fr.inner;
                    if r.changed() {
                        self.search_t = Some(Instant::now());
                        if self.search.trim().len() < 2 {
                            self.search_res.clear();
                        }
                    }
                    let go = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    let (br, b) = ui.allocate_exact_size(vec2(38.0, fr.response.rect.height()), Sense::click());
                    ui.painter().rect_filled(br, egui::CornerRadius { nw: 0, sw: 0, ne: 6, se: 6 }, if b.hovered() { Color32::from_rgb(60, 60, 68) } else { Color32::from_rgb(47, 47, 53) });
                    paint_icon(ui.painter(), br.shrink(10.0), Icon::Search, TEXT);
                    if go || b.clicked() {
                        // Enter: el primer resultado si coincide; si no, el texto tal cual como canal
                        let typed = self.search.trim().to_lowercase();
                        let c = self.search_res.iter().find(|l| l.login == typed).or(self.search_res.first()).map(|l| l.login.clone()).unwrap_or(typed);
                        self.join(&c);
                    }
                    // dropdown visible cuando hay resultados, no solo con foco
                    if r.has_focus() || !self.search_res.is_empty() {
                        search_rect = fr.response.rect.union(br);
                    }
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // controles de ventana (la app no usa el marco de Windows)
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for (icon, tip) in [(Icon::Close, "Cerrar"), (if maximized { Icon::WinRestore } else { Icon::WinMax }, "Maximizar"), (Icon::WinMin, "Minimizar")] {
                        let (r, b) = ui.allocate_exact_size(vec2(46.0, 50.0), Sense::click());
                        if b.hovered() {
                            ui.painter().rect_filled(r, 0.0, if matches!(icon, Icon::Close) { Color32::from_rgb(196, 43, 28) } else { Color32::from_white_alpha(20) });
                        }
                        paint_icon(ui.painter(), Rect::from_center_size(r.center(), vec2(10.0, 10.0)), icon, TEXT);
                        if b.on_hover_text(tip).clicked() {
                            ui.send_viewport_cmd(match icon {
                                Icon::Close => egui::ViewportCommand::Close,
                                Icon::WinMin => egui::ViewportCommand::Minimized(true),
                                _ => egui::ViewportCommand::Maximized(!maximized),
                            });
                        }
                    }
                    ui.add_space(12.0);
                    ui.spacing_mut().item_spacing.x = 6.0;
                    match &self.me {
                        Some(_) => {
                            let (r, b) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
                            avatar(ui, &self.store, &self.my_avatar, r, false);
                            if b.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                self.menu_open = !self.menu_open;
                                self.menu_page = 0;
                            }
                            // campana de notificaciones como en twitch.tv, con punto rojo si hay sin leer
                            if self.session().is_some() {
                                let bell = icon_btn(ui, Icon::Bell, self.menu_open && self.menu_page == 2, "Notificaciones");
                                if self.notifs.iter().any(|n| !n.read) {
                                    ui.painter().circle_filled(bell.rect.right_top() + vec2(-8.0, 8.0), 4.0, RED);
                                }
                                if bell.clicked() {
                                    self.open_notifs();
                                }
                            }
                        }
                        None => {
                            if icon_btn(ui, Icon::Person, self.menu_open, "Menú").clicked() {
                                self.menu_open = !self.menu_open;
                            }
                            if purple_btn(ui, "Iniciar sesión").clicked() {
                                self.login_open = true;
                            }
                        }
                    }
                    if icon_btn(ui, Icon::Pin, self.cfg.on_top, "Siempre encima").clicked() {
                        self.cfg.on_top = !self.cfg.on_top;
                        self.cfg.save();
                        let lvl = if self.cfg.on_top { egui::WindowLevel::AlwaysOnTop } else { egui::WindowLevel::Normal };
                        ui.send_viewport_cmd(egui::ViewportCommand::WindowLevel(lvl));
                    }
                    if icon_btn(ui, Icon::Video, self.cfg.video, if self.cfg.video { "Apagar video (solo chat: libera casi toda la RAM)" } else { "Encender video" }).clicked() {
                        let on = !self.cfg.video;
                        self.set_video(on, gl);
                    }
                });
            });
        });
        // Sugerencias del buscador
        if search_rect.is_positive() && !self.search_res.is_empty() {
            // Escape cierra el dropdown
            if ui.input(|i| i.key_pressed(Key::Escape)) {
                self.search_res.clear();
            } else {
                let mut go = None;
                let dropdown = egui::Area::new(egui::Id::unique("search_res")).order(egui::Order::Foreground).fixed_pos(search_rect.left_bottom() + vec2(0.0, 4.0)).show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).fill(PANEL).corner_radius(6).inner_margin(6).show(ui, |ui| {
                        ui.set_width(search_rect.width() - 12.0);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for l in &self.search_res {
                            if side_row(ui, &self.store, l, true, "") {
                                go = Some(l.login.clone());
                            }
                        }
                    });
                });
                if let Some(c) = go {
                    self.join(&c);
                } else if ui.input(|i| i.pointer.any_pressed()) {
                    let ptr = ui.input(|i| i.pointer.interact_pos().unwrap_or_default());
                    if !search_rect.contains(ptr) && !dropdown.response.rect.contains(ptr) {
                        self.search_res.clear();
                    }
                }
            }
        }
    }

    fn sidebar(&mut self, ui: &mut Ui) {
        // el ancho se anima (150 ms); el contenido cambia a la mitad del recorrido
        let w = ui.ctx().animate_value_with_time(egui::Id::unique("side_w"), if self.cfg.sidebar { SIDE_W } else { SIDE_MIN }, 0.15);
        let open = w > (SIDE_W + SIDE_MIN) / 2.0;
        let mut go = None;
        egui::Panel::left("side").exact_size(w).resizable(false).frame(egui::Frame::new().fill(SIDE)).show(ui, |ui| {
            egui::ScrollArea::vertical().scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if open {
                        ui.add_space(10.0);
                        ui.label(RichText::new("Para ti").font(semi(16.0)).color(TEXT));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_space(6.0);
                            if icon_btn(ui, Icon::Left, false, "Contraer").clicked() {
                                self.cfg.sidebar = false;
                                self.cfg.save();
                            }
                        });
                    } else {
                        ui.add_space(11.0);
                        if icon_btn(ui, Icon::Right, false, "Expandir").clicked() {
                            self.cfg.sidebar = true;
                            self.cfg.save();
                        }
                    }
                });
                ui.add_space(6.0);
                if self.me.is_some() && !self.follows.is_empty() {
                    section(ui, "Canales que sigo", open);
                    let n = if self.follows_all { self.follows.len() } else { 10.min(self.follows.len()) };
                    for l in &self.follows[..n] {
                        if side_row(ui, &self.store, l, open, &self.chan) {
                            go = Some(l.login.clone());
                        }
                    }
                    if open && self.follows.len() > 10 {
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.add_space(10.0);
                            let t = if self.follows_all { "Mostrar menos" } else { "Mostrar más" };
                            if clickable_text(ui, RichText::new(t).color(PURPLE_L)).clicked() {
                                self.follows_all = !self.follows_all;
                            }
                        });
                    }
                    ui.add_space(14.0);
                }
                // como "Los espectadores de X también ven" de twitch.tv, pero con datos públicos:
                // otros directos de la misma categoría del canal que estás viendo
                if !self.chan.is_empty() && !self.related.1.is_empty() {
                    let title = format!("Más en {}", self.related.0);
                    section(ui, &title, open);
                    for l in &self.related.1 {
                        if side_row(ui, &self.store, l, open, &self.chan) {
                            go = Some(l.login.clone());
                        }
                    }
                    ui.add_space(14.0);
                }
                section(ui, "Canales en directo", open);
                for l in self.top.iter().take(12) {
                    if side_row(ui, &self.store, l, open, &self.chan) {
                        go = Some(l.login.clone());
                    }
                }
                ui.add_space(10.0);
            });
        });
        if let Some(c) = go {
            self.join(&c);
        }
    }

    fn prepare_handoff(&mut self, gl: &eframe::glow::Context, frame: &mut eframe::Frame) {
        for p in self.retired_players.drain(..) { p.destroy(gl); }
        if self.handoff.is_none() { return; }
        if self.paused || self.rewound || !self.cfg.video { self.cancel_handoff(); return; }
        if self.ad_player.is_none() {
            let player = self.get_proc.clone().ok_or_else(|| "Sin contexto OpenGL".to_string())
                .and_then(|gp| crate::mpv::Player::standby(gp, self.net.ctx.clone(), self.cfg.volume));
            match player {
                Ok(p) => {
                    p.play(&self.handoff.as_ref().unwrap().candidate.selected.1);
                    self.ad_player = Some(p);
                    crate::tlog("Alternative warmup: muted player created");
                }
                Err(e) => {
                    self.cancel_handoff();
                    self.ad_recovery.failed(Instant::now());
                    crate::tlog(&format!("Alternative warmup failed: {e}"));
                    return;
                }
            }
        }
        let p = self.ad_player.as_mut().unwrap();
        p.texture(gl, frame, [2, 2]);
        let handoff = self.handoff.as_mut().unwrap();
        let ready = handoff.warmup.ready(p.num("time-pos"), p.frame_serial(), handoff.candidate.selected.0 != "audio_only", p.flag("paused-for-cache") || p.flag("idle-active"));
        if ready {
            crate::tlog(&format!("Alternative ready: frames={:.0}, position={:.3}", p.frame_serial().unwrap_or(0.0), p.num("time-pos").unwrap_or(0.0)));
            let candidate = self.handoff.take().unwrap().candidate;
            let next = self.ad_player.take().unwrap();
            if let Some(old) = self.player.replace(next) {
                old.set("mute", "yes");
                old.destroy(gl);
            }
            self.accept_alternative(candidate);
        } else if handoff.warmup.expired(Instant::now()) {
            self.cancel_handoff();
            self.ad_recovery.failed(Instant::now());
            crate::tlog("Alternative warmup failed: no moving frames; current playback preserved");
        }
    }

    fn player(&mut self, ui: &mut Ui, frame: &mut eframe::Frame, rect: Rect) {
        ui.painter().rect_filled(rect, 0.0, Color32::BLACK);
        let resp = ui.interact(rect, egui::Id::unique("video"), Sense::click());
        // rect_contains_pointer y no resp.hovered(): los botones de la barra le quitan el hover al
        // video, la barra se ocultaba, volvía el hover y reaparecía (parpadeo cuadro a cuadro).
        let inside = ui.rect_contains_pointer(rect);
        let bar = Rect::from_min_max(pos2(rect.min.x, rect.max.y - 46.0), rect.max);
        let on_bar = ui.input(|i| i.pointer.hover_pos()).is_some_and(|p| bar.contains(p));
        let moved = ui.input(|i| i.pointer.delta() != Vec2::ZERO);
        if inside && (moved || on_bar) {
            self.controls_t = Instant::now(); // con el mouse sobre la barra no se oculta
        }
        let ppp = ui.ctx().pixels_per_point();
        self.want_h = (rect.height() * ppp) as u32;
        if let Some(p) = &mut self.player {
            let ar = p.aspect();
            let size = if rect.width() / rect.height().max(1.0) > ar { vec2(rect.height() * ar, rect.height()) } else { vec2(rect.width(), rect.width() / ar) };
            let vr = Rect::from_center_size(rect.center(), size);
            let px = [(size.x * ppp).round().max(1.0) as i32, (size.y * ppp).round().max(1.0) as i32];
            if let Some(gl) = frame.gl().cloned() {
                let id = p.texture(&gl, frame, px);
                ui.painter().image(id, vr, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
        }
        let msg = if !self.status.is_empty() { self.status.as_str() } else if self.player.is_none() { "Cargando…" } else { "" };
        if !msg.is_empty() {
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, msg, FontId::proportional(16.0), DIM);
        }
        if let (true, Some(p)) = (self.stats_open, &self.player) {
            // "Estadísticas avanzadas del video" como en Twitch; se refrescan cada segundo
            let n = |k: &str| p.num(k).unwrap_or(0.0);
            let hw = p.text("hwdec-current");
            let codec = p.text("video-codec");
            let rows = [
                ("Resolución", format!("{}x{}", n("width") as i64, n("height") as i64)),
                ("Variante", self.playing.clone()),
                ("FPS", format!("{:.0}", n("estimated-vf-fps"))),
                ("Códec", codec.split(" (").next().unwrap_or_default().to_string()),
                ("Decodificación", if hw.is_empty() || hw == "no" { "software".into() } else { format!("GPU ({hw})") }),
                ("Bitrate de video", format!("{:.0} kbps", n("video-bitrate") / 1000.0)),
                ("Bitrate de audio", format!("{:.0} kbps", n("audio-bitrate") / 1000.0)),
                ("Velocidad de descarga", format!("{:.1} Mbps", n("cache-speed") * 8.0 / 1e6)),
                ("Búfer (atrás del directo)", format!("{:.1} s", self.buffer)),
                ("Se puede regresar", self.rewind_range().map(|(s, _, e)| format!("{:.0} s", e - s)).unwrap_or_default()),                ("Velocidad", format!("{:.2}x{}", n("speed"), if self.catching_up { " (alcanzando el directo)" } else { "" })),
                ("Frames perdidos", format!("{} · decodificador {}", n("frame-drop-count") as i64, n("decoder-frame-drop-count") as i64)),
                ("Baja latencia", (if self.cfg.low_latency { "activada" } else { "desactivada" }).into()),
            ];
            // del lado contrario al chat flotante, para no encimarse
            let chat_left = self.cfg.chat_overlay && self.cfg.chat_left && self.chat_open;
            let x = if chat_left { rect.right() - 342.0 } else { rect.left() + 12.0 };
            let box_r = Rect::from_min_size(pos2(x, rect.top() + 12.0), vec2(330.0, 16.0 + rows.len() as f32 * 18.0));
            let pt = ui.painter_at(rect); // recortado al video si la ventana es chica
            pt.rect_filled(box_r, 6.0, Color32::from_black_alpha(200));
            for (i, (k, v)) in rows.iter().enumerate() {
                let y = box_r.top() + 8.0 + i as f32 * 18.0;
                pt.text(pos2(box_r.left() + 10.0, y), Align2::LEFT_TOP, *k, FontId::proportional(12.0), DIM);
                pt.text(pos2(box_r.left() + 175.0, y), Align2::LEFT_TOP, v, FontId::proportional(12.0), TEXT);
            }
        }
        if self.paused {
            // como Twitch: botón grande de play al centro cuando está en pausa
            let c = rect.center();
            ui.painter().circle_filled(c, 34.0, Color32::from_black_alpha(140));
            paint_icon(ui.painter(), Rect::from_center_size(c + vec2(4.0, 0.0), vec2(28.0, 30.0)), Icon::Play, Color32::WHITE);
        }
        let visible = self.controls_t.elapsed() < Duration::from_millis(2500);
        // TWITCHLITE_CONTROLS: deja la barra fija (solo para capturas de prueba)
        let show = rect.height() > 60.0 && (self.paused || self.quality_open || (inside && visible) || std::env::var_os("TWITCHLITE_CONTROLS").is_some());
        let mut gear = None;
        if show {
            if !self.paused {
                ui.ctx().request_repaint_after(Duration::from_millis(2600)); // para ocultar la barra sola
            }
            let mut mesh = egui::Mesh::default();
            let (t, b) = (Color32::TRANSPARENT, Color32::from_black_alpha(230));
            mesh.colored_vertex(bar.left_top() - vec2(0.0, 50.0), t);
            mesh.colored_vertex(bar.right_top() - vec2(0.0, 50.0), t);
            mesh.colored_vertex(bar.right_bottom(), b);
            mesh.colored_vertex(bar.left_bottom(), b);
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            ui.painter().add(mesh);
            let mut b = ui.new_child(UiBuilder::new().max_rect(bar.shrink2(vec2(10.0, 8.0))).layout(Layout::left_to_right(Align::Center)));
            if icon_btn(&mut b, if self.paused { Icon::Play } else { Icon::Pause }, false, if self.paused { "Reproducir (K)" } else { "Pausa (K)" }).clicked() {
                self.toggle_pause();
            }
            if icon_btn(&mut b, if self.muted || self.cfg.volume == 0.0 { Icon::Mute } else { Icon::Vol }, false, "Silenciar (M)").clicked() {
                self.muted = !self.muted;
                if let Some(p) = &self.player {
                    p.set("mute", if self.muted { "yes" } else { "no" });
                }
            }
            // angosto: sin slider ni botones secundarios, como el player chico de twitch.tv
            let narrow = bar.width() < 620.0;
            if !narrow && volume_bar(&mut b, &mut self.cfg.volume) {
                if let Some(p) = &self.player {
                    p.set("volume", &self.cfg.volume.to_string());
                    if self.muted && self.cfg.volume > 0.0 {
                        self.muted = false;
                        p.set("mute", "no");
                    }
                }
                self.cfg.save();
            }
            if self.info.viewers.is_some() {
                // Como twitch.tv: tiempo en vivo arriba a la izquierda y "EN DIRECTO" arriba a la derecha.
                // Si te quedaste >8 s atrás, en su lugar aparece "IR AL DIRECTO" en la barra.
                if let Some(t0) = self.info.started {
                    let s = (crate::twitch::now_unix() - t0).max(0);
                    b.painter().text(pos2(bar.left() + 14.0, bar.top() - 10.0), Align2::LEFT_CENTER, format!("{:02}:{:02}:{:02}", s / 3600, s % 3600 / 60, s % 60), FontId::proportional(13.0), Color32::WHITE);
                }
                self.seek_bar(&b, bar);
                if self.buffer > 8.0 || self.paused {
                    b.add_space(10.0);
                    if b.add(egui::Button::new(RichText::new("IR AL DIRECTO").font(semi(10.5)).color(Color32::WHITE)).fill(Color32::from_rgb(70, 70, 78)).corner_radius(3)).on_hover_text(format!("Vas {:.0} s atrás", self.buffer)).clicked() {
                        self.go_live();
                    }
                } else {
                    live_pill(b.painter(), pos2(bar.right() - 52.0, bar.top() - 10.0));
                }
            }
            b.with_layout(Layout::right_to_left(Align::Center), |b| {
                if icon_btn(b, Icon::Full, false, "Pantalla completa (F)").clicked() {
                    let fs = b.input(|i| i.viewport().fullscreen.unwrap_or(false));
                    b.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fs));
                }
                if !narrow && icon_btn(b, Icon::Chat, self.cfg.chat_overlay, "Chat encima del video (C)").clicked() {
                    self.cfg.chat_overlay = !self.cfg.chat_overlay;
                    self.chat_open = true;
                    self.cfg.save();
                }
                if !narrow && icon_btn(b, Icon::Theater, self.theater, "Modo cine (T)").clicked() {
                    self.theater = !self.theater;
                }
                let g = icon_btn(b, Icon::Gear, self.quality_open, "Ajustes");
                if g.clicked() {
                    self.quality_open = !self.quality_open;
                }
                gear = Some(g.rect);
                if self.info.viewers.is_some() {
                    if b.add_enabled(!self.ad_recovery.pending(), egui::Button::new(RichText::new("Skip Ad").font(semi(10.0)).color(Color32::WHITE)).fill(Color32::from_rgb(70, 70, 78)).corner_radius(3)).on_hover_text("Buscar una variante sin publicidad. Se intenta automáticamente al detectar un anuncio.").clicked() {
                        self.request_ad_skip(true);
                    }
                }
                if self.ad_detected {
                    b.add(egui::Label::new(RichText::new("AD").font(semi(10.0)).color(RED)));
                }
            });
        }
        let pointer_on_bar = ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| show && bar.contains(p));
        if resp.double_clicked() {
            let fs = ui.input(|i| i.viewport().fullscreen.unwrap_or(false));
            ui.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fs));
        } else if resp.clicked() && !pointer_on_bar {
            self.toggle_pause();
        }
        if let (true, Some(g)) = (self.quality_open, gear) {
            let mut pick = None;
            let mut mini = false;
            let area = egui::Area::new(egui::Id::unique("quality")).order(egui::Order::Foreground).pivot(Align2::RIGHT_BOTTOM).fixed_pos(g.right_top() - vec2(0.0, 6.0)).show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).fill(PANEL).corner_radius(8).inner_margin(10).show(ui, |ui| {
                    ui.set_width(260.0);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    if toggle_row(ui, "Baja latencia", &mut self.cfg.low_latency) {
                        self.cfg.save();
                    }
                    toggle_row(ui, "Estadísticas avanzadas", &mut self.stats_open);
                    if menu_row(ui, Icon::Theater, "Mini reproductor", false).on_hover_text("T").clicked() {
                        mini = true;
                    }
                    if menu_row(ui, Icon::Keyboard, "Atajos de teclado", false).clicked() {
                        self.menu_open = true;
                        self.menu_page = 5;
                    }
                    menu_sep(ui);
                    ui.label(RichText::new("Calidad").font(semi(12.5)).color(DIM));
                    ui.add_space(4.0);
                    let auto = self.cfg.quality == "Auto";
                    if radio_row(ui, &format!("Auto ({})", if auto { self.playing.as_str() } else { "según la ventana" }), auto) {
                        pick = Some(None);
                    }
                    for q in &self.qualities {
                        let name = if q.0 == "audio_only" { "Solo audio" } else { q.0.as_str() };
                        if radio_row(ui, name, !auto && q.0 == self.cfg.quality) {
                            pick = Some(Some(q.clone()));
                        }
                    }
                });
            });
            if mini {
                self.theater = true;
                self.quality_open = false;
            }
            if let Some(q) = pick {
                self.pick_quality(q);
                self.quality_open = false;
            } else if ui.input(|i| i.pointer.any_click()) && !area.response.hovered() && !g.contains(ui.input(|i| i.pointer.interact_pos()).unwrap_or_default()) {
                self.quality_open = false;
            }
        }
    }

    /// Barra de progreso del directo: lo que mpv tiene guardado (hasta ~40 s atrás) con la posición
    /// actual. Arrastrar regresa dentro de ese rango; soltar en la orilla derecha vuelve al directo.
    fn seek_bar(&mut self, ui: &Ui, bar: Rect) {
        let Some((start, pos, end)) = self.rewind_range() else { return };
        let span = (end - start).max(1.0);
        let track = Rect::from_min_max(pos2(bar.left() + 90.0, bar.top() - 12.0), pos2(bar.right() - 110.0, bar.top() - 8.0));
        if track.width() < 60.0 {
            return;
        }
        let hit = track.expand2(vec2(0.0, 8.0));
        let r = ui.interact(hit, egui::Id::unique("seek"), Sense::click_and_drag());
        let x_of = |t: f64| track.left() + ((t - start) / span).clamp(0.0, 1.0) as f32 * track.width();
        let t_of = |x: f32| start + ((x - track.left()) / track.width()).clamp(0.0, 1.0) as f64 * span;
        let pnt = ui.painter();
        let hot = r.hovered() || r.dragged();
        let th = if hot { track.expand2(vec2(0.0, 1.5)) } else { track };
        pnt.rect_filled(th, 3.0, Color32::from_white_alpha(60));
        let px = r.interact_pointer_pos().or(r.hover_pos()).map(|p| p.x);
        let shown = if r.dragged() { px.map(t_of).unwrap_or(pos) } else { pos };
        pnt.rect_filled(Rect::from_min_max(th.min, pos2(x_of(shown), th.max.y)), 3.0, PURPLE);
        if hot {
            pnt.circle_filled(pos2(x_of(shown), th.center().y), 6.0, Color32::WHITE);
        }
        if let Some(x) = px.filter(|_| hot) {
            let behind = end - t_of(x);
            let label = if behind < 1.5 { "Directo".to_string() } else { format!("-{:.0} s", behind) };
            let g = pnt.layout_no_wrap(label, semi(12.0), Color32::WHITE);
            let tip = Rect::from_center_size(pos2(x, track.top() - 16.0), g.size() + vec2(10.0, 4.0));
            pnt.rect_filled(tip, 4.0, Color32::from_black_alpha(210));
            pnt.galley(tip.min + vec2(5.0, 2.0), g, Color32::WHITE);
        }
        let target = px.filter(|_| r.drag_stopped() || r.clicked()).map(t_of);
        if let Some(t) = target {
            if end - t < 1.5 {
                self.go_live();
            } else if let Some(p) = &self.player {
                p.set("time-pos", &format!("{t:.2}"));
                self.rewound = true;
            }
        }
        if !self.paused {
            ui.ctx().request_repaint_after(Duration::from_millis(250)); // la posición avanza sola
        }
    }

    fn toggle_pause(&mut self) {
        if let Some(p) = &self.player {
            p.toggle_pause();
            self.paused = !self.paused;
        }
    }

    fn info_bar(&mut self, ui: &mut Ui, rect: Rect) {
        let p = ui.painter_at(rect);
        let live = self.info.viewers.is_some();
        let a = Rect::from_min_size(rect.min + vec2(20.0, 12.0), vec2(56.0, 56.0));
        if live {
            p.circle_stroke(a.center(), 30.5, Stroke::new(2.0, RED));
        }
        avatar(ui, &self.store, &self.info.avatar, a, false);
        if live {
            live_pill(&p, pos2(a.center().x, a.bottom() + 4.0));
        }
        // Izquierda, como twitch.tv: nombre (+ verificado), título, categoría y tags
        let x = a.right() + 16.0;
        let compact = rect.width() < COMPACT_W;
        let right_w = if compact { 16.0 } else { 330.0 };
        let tw = (rect.right() - x - right_w).max(60.0);
        let name = if self.info.name.is_empty() { self.chan.clone() } else { self.info.name.clone() };
        let nw = p.layout_no_wrap(name.clone(), semi(18.0), TEXT).size().x.min(tw - 22.0);
        text_trunc(&p, pos2(x, rect.top() + 8.0), &name, semi(18.0), TEXT, tw - 22.0);
        if self.info.partner {
            paint_icon(&p, Rect::from_min_size(pos2(x + nw + 6.0, rect.top() + 13.0), vec2(15.0, 15.0)), Icon::Verified, PURPLE_L);
        }
        emoji_line(ui, &self.store, &p, pos2(x, rect.top() + 34.0), &self.info.title, semi(14.0), TEXT, tw);
        let game = self.info.game.clone();
        let gw = p.layout_no_wrap(game.clone(), FontId::proportional(13.5), PURPLE_L).size().x.min(tw);
        let gr = Rect::from_min_size(pos2(x, rect.top() + 58.0), vec2(gw, 18.0));
        text_trunc(&p, gr.min, &game, FontId::proportional(13.5), PURPLE_L, tw);
        let mut tx = gr.right() + 10.0;
        for t in &self.info.tags {
            let tg = p.layout_no_wrap(t.clone(), semi(12.0), TEXT);
            let pill = Rect::from_min_size(pos2(tx, rect.top() + 58.0), tg.size() + vec2(16.0, 4.0));
            if pill.right() > x + tw {
                break;
            }
            p.rect_filled(pill, 10.0, Color32::from_rgb(47, 47, 53));
            p.galley(pill.min + vec2(8.0, 2.0), tg, TEXT);
            tx = pill.right() + 6.0;
        }
        if !game.is_empty() && ui.interact(gr, egui::Id::unique("game_link"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Ver categoría").clicked() {
            self.go(Page::Category(game));
            return;
        }

        // Derecha arriba: seguir y suscribirse (Twitch solo los permite desde su web: se abren ahí)
        let following = self.follows.iter().any(|l| l.login == self.chan);
        // angosto: botones y datos bajan a un segundo renglón (botones a la derecha, datos a la izquierda)
        let row2 = rect.top() + 88.0;
        // angosto: renglón 2 = datos, renglón 3 = corazón y Suscribirse (desde la izquierda)
        let top = if compact {
            Rect::from_min_max(pos2(rect.left() + 20.0, row2 + 36.0), pos2(rect.left() + 240.0, row2 + 70.0))
        } else {
            Rect::from_min_max(pos2(rect.right() - right_w, rect.top() + 6.0), pos2(rect.right() - 16.0, rect.top() + 40.0))
        };
        let mut c = ui.new_child(UiBuilder::new().max_rect(top).layout(if compact { Layout::left_to_right(Align::Center) } else { Layout::right_to_left(Align::Center) }));
        // píldoras como las de twitch.tv
        if c.add(egui::Button::new(RichText::new("Suscribirse").font(semi(14.0)).color(Color32::WHITE)).fill(PURPLE).corner_radius(17).min_size(vec2(0.0, 34.0))).on_hover_text("Se abre en twitch.tv").clicked() {
            c.ctx().open_url(egui::OpenUrl::new_tab(format!("https://www.twitch.tv/subs/{}", self.chan)));
        }
        c.add_space(8.0);
        let (hr, heart) = c.allocate_exact_size(vec2(48.0, 34.0), Sense::click());
        c.painter().rect_filled(hr, 17.0, if heart.hovered() { Color32::from_rgb(60, 60, 68) } else { Color32::from_rgb(47, 47, 53) });
        paint_icon(c.painter(), Rect::from_center_size(hr.center(), vec2(16.0, 15.0)), Icon::Heart, if following { PURPLE_L } else { TEXT });
        if heart.on_hover_text(if following { "Siguiendo (se gestiona en twitch.tv)" } else { "Seguir (se abre en twitch.tv)" }).clicked() {
            c.ctx().open_url(egui::OpenUrl::new_tab(format!("https://www.twitch.tv/{}", self.chan)));
        }

        // Datos y controles discretos debajo del video: recargar y compartir.
        // Ancho: alineados a la derecha como twitch.tv. Angosto: segundo renglón, desde la izquierda.
        let y = if compact { row2 + 17.0 } else { rect.top() + 62.0 };
        let mut items: Vec<(Option<Icon>, String, Color32, FontId, &str)> = Vec::new();
        match self.info.viewers {
            Some(v) => {
                items.push((Some(Icon::Person), fmt_num(v), RED, semi(13.5), "Espectadores"));
                if let Some(t0) = self.info.started {
                    let s = (crate::twitch::now_unix() - t0).max(0);
                    items.push((None, format!("{}:{:02}:{:02}", s / 3600, s % 3600 / 60, s % 60), TEXT, FontId::proportional(13.5), "Tiempo en directo"));
                    ui.ctx().request_repaint_after(Duration::from_secs(1)); // el reloj avanza como en twitch.tv
                }
                if self.player.is_some() {
                    items.push((Some(Icon::Gauge), format!("{:.2} s", self.buffer), TEXT, FontId::proportional(13.5), "Retraso respecto al directo (búfer del reproductor)"));
                }
            }
            None if !self.info.name.is_empty() => items.push((None, "Desconectado".into(), DIM, FontId::proportional(13.5), "")),
            None => {}
        }
        if self.copied_t.is_some_and(|t| t.elapsed() < Duration::from_secs(2)) {
            items.push((None, "¡Enlace copiado!".into(), PURPLE_L, FontId::proportional(13.0), ""));
            ui.ctx().request_repaint_after(Duration::from_secs(2));
        }
        let galleys: Vec<_> = items.iter().map(|(_, t, c, f, _)| p.layout_no_wrap(t.clone(), f.clone(), *c)).collect();
        let widths: Vec<f32> = items.iter().zip(&galleys).map(|(it, g)| g.size().x + if it.0.is_some() { 20.0 } else { 0.0 }).collect();
        let total: f32 = widths.iter().sum::<f32>() + 16.0 * items.len() as f32 + 60.0;
        let mut x = if compact { rect.left() + 20.0 } else { rect.right() - 16.0 - total };
        for ((it, g), w) in items.iter().zip(galleys).zip(&widths) {
            let tx = if let Some(icon) = it.0 {
                paint_icon(&p, Rect::from_min_size(pos2(x, y - 7.0), vec2(14.0, 13.0)), icon, it.2);
                x + 20.0
            } else {
                x
            };
            p.galley(pos2(tx, y - 9.0), g, it.2);
            if !it.4.is_empty() {
                ui.interact(Rect::from_min_size(pos2(x, y - 10.0), vec2(*w, 20.0)), egui::Id::unique(("stat", it.4)), Sense::hover()).on_hover_text(it.4);
            }
            x += w + 16.0;
        }
        let refresh = Rect::from_min_size(pos2(x, y - 13.0), vec2(26.0, 26.0));
        let pending = self.stream_pending.is_some();
        let enabled = self.cfg.video && !pending;
        let attention = self.cfg.video && (pending || self.ad_recovery.pending() || !self.status.is_empty()
            || (!self.paused && !self.rewound && (self.health.needs_attention(Instant::now()) || self.buffer >= 10.0)));
        let pulse = if attention && ui.style().animation_time > 0.0 {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
            ui.input(|i| ((i.time * std::f64::consts::TAU / 1.6).sin() * 0.5 + 0.5) as f32)
        } else { 0.0 };
        let accent = Color32::from_rgb((145.0 + 46.0 * pulse) as u8, (71.0 + 77.0 * pulse) as u8, 255);
        let reload = ui.interact(refresh, egui::Id::unique("refresh_stream"), if enabled { Sense::click() } else { Sense::hover() });
        if attention {
            p.rect_filled(refresh.expand(2.0), 6.0, Color32::from_rgba_unmultiplied(191, 148, 255, (60.0 + 80.0 * pulse) as u8));
        }
        p.rect_filled(refresh, 4.0, if !self.cfg.video { SIDE } else if enabled && reload.hovered() { PURPLE_L } else { accent });
        paint_icon(&p, refresh.shrink(5.0), Icon::Refresh, if self.cfg.video { Color32::WHITE } else { DIM });
        if reload.on_hover_cursor(if enabled { egui::CursorIcon::PointingHand } else { egui::CursorIcon::Default })
            .on_hover_text(if pending { "Reconectando al directo…" } else if attention { "La reproducción se detuvo o está retrasada. Recargar y volver al directo (F5)" } else { "Recargar el stream y volver al directo (F5)" }).clicked() && enabled {
            self.go_live();
        }
        let share = Rect::from_min_size(pos2(x + 34.0, y - 13.0), vec2(26.0, 26.0));
        let sh = ui.interact(share, egui::Id::unique("share"), Sense::click());
        if sh.hovered() {
            p.rect_filled(share, 4.0, HOVER);
        }
        paint_icon(&p, share.shrink(6.0), Icon::Share, TEXT);
        if sh.on_hover_text("Copiar enlace del canal").clicked() {
            ui.ctx().copy_text(format!("https://www.twitch.tv/{}", self.chan));
            self.copied_t = Some(Instant::now());
        }
    }

    /// Zona bajo el player: tabs (Acerca de | Clips | Vídeos) con carga bajo demanda.
    fn about(&mut self, ui: &mut Ui, rect: Rect, gl: Option<&eframe::glow::Context>) {
        let mut c = ui.new_child(UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min)));
        if !self.cfg.video {
            c.add_space(6.0);
            if purple_btn(&mut c, "Ver video").clicked() {
                self.set_video(true, gl);
            }
            c.add_space(10.0);
        }
        if self.info.name.is_empty() {
            return;
        }
        // Tabs
        c.horizontal(|ui| {
            for (label, idx) in [("Acerca de", 0u8), ("Clips", 1), ("Vídeos", 2)] {
                let sel = self.chan_tab == idx;
                let color = if sel { PURPLE_L } else { DIM };
                let r = ui.add(egui::Label::new(RichText::new(label).font(semi(14.0)).color(color)).sense(Sense::click()));
                if sel {
                    let p = ui.painter();
                    p.line_segment([pos2(r.rect.left(), r.rect.bottom() + 2.0), pos2(r.rect.right(), r.rect.bottom() + 2.0)], Stroke::new(2.0, PURPLE_L));
                }
                if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !sel {
                    self.chan_tab = idx;
                    match idx {
                        1 if !self.clips_loaded => twitch::load_clips(&self.net, self.chan.clone(), "ALL_TIME"),
                        2 if !self.videos_loaded => twitch::load_videos(&self.net, self.chan.clone()),
                        _ => {}
                    }
                }
                ui.add_space(16.0);
            }
        });
        c.add_space(8.0);
        match self.chan_tab {
            0 => self.tab_about(&mut c),
            1 => self.tab_clips(&mut c),
            2 => self.tab_videos(&mut c),
            _ => {}
        }
    }

    fn tab_about(&mut self, c: &mut Ui) {
        let w = c.available_width();
        egui::Frame::new().fill(PANEL).corner_radius(6).inner_margin(egui::Margin::symmetric(16, 10)).show(c, |ui| {
            ui.set_width(w - 32.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Acerca de {}", self.info.name)).font(semi(15.0)).color(TEXT));
                ui.add(egui::Label::new(RichText::new(format!("  ·  {} seguidores", fmt_num(self.info.followers))).color(DIM)).truncate());
            });
            if !self.info.bio.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new(&self.info.bio).color(DIM));
            }
            if !self.info.socials.is_empty() {
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    for (name, title, url) in &self.info.socials {
                        let label = if title.is_empty() { name } else { title };
                        if ui.add(egui::Label::new(RichText::new(format!("🔗 {label}")).color(PURPLE_L)).sense(Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(url).clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                        }
                        ui.add_space(8.0);
                    }
                });
            }
        });
        if !self.info.panels.is_empty() {
            c.add_space(10.0);
            let px = (160.0 * c.ctx().pixels_per_point()).round() as u32;
            let pw = 300.0;
            let mut open = None;
            egui::ScrollArea::vertical().id_salt("panels").show(c, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(16.0, 16.0);
                    for p in &self.info.panels {
                        ui.allocate_ui_with_layout(vec2(pw, 0.0), Layout::top_down(Align::Min), |ui| {
                            ui.set_width(pw);
                            if !p.title.is_empty() {
                                ui.label(RichText::new(&p.title).font(semi(15.0)).color(TEXT));
                                ui.add_space(4.0);
                            }
                            if !p.image.is_empty() {
                                if let img::Look::Ready(f) = self.store.get(self.store.id(&p.image, px)) {
                                    let size = vec2(pw, pw * f.size.y / f.size.x);
                                    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
                                    egui::Image::from_texture(SizedTexture::new(f.tex, size)).uv(f.uv).corner_radius(6.0).paint_at(ui, r);
                                    if !p.link.is_empty() && resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(&p.link).clicked() {
                                        open = Some(p.link.clone());
                                    }
                                } else {
                                    ui.allocate_exact_size(vec2(pw, 100.0), Sense::hover());
                                }
                                ui.add_space(4.0);
                            }
                            if !p.desc.is_empty() {
                                ui.label(RichText::new(&p.desc).color(DIM));
                            }
                        });
                    }
                });
            });
            if let Some(u) = open {
                c.ctx().open_url(egui::OpenUrl::new_tab(u));
            }
        }
    }

    fn tab_clips(&mut self, c: &mut Ui) {
        if !self.clips_loaded {
            c.label(RichText::new("Cargando clips...").color(DIM));
            return;
        }
        if self.clips.is_empty() {
            c.label(RichText::new("Sin clips").color(DIM));
            return;
        }
        let w = 280.0;
        let mut open = None;
        egui::ScrollArea::vertical().id_salt("clips_tab").show(c, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
                let px = (w * ui.ctx().pixels_per_point()).round() as u32;
                for clip in &self.clips {
                    ui.allocate_ui_with_layout(vec2(w, 0.0), Layout::top_down(Align::Min), |ui| {
                        ui.set_width(w);
                        let h = w * 9.0 / 16.0;
                        if !clip.thumb.is_empty() {
                            if let img::Look::Ready(f) = self.store.get(self.store.id(&clip.thumb, px)) {
                                let size = vec2(w, h);
                                let (r, resp) = ui.allocate_exact_size(size, Sense::click());
                                egui::Image::from_texture(SizedTexture::new(f.tex, size)).uv(f.uv).corner_radius(6.0).paint_at(ui, r);
                                // duración
                                let dur = format!("{}:{:02}", clip.duration / 60, clip.duration % 60);
                                let dg = ui.painter().layout_no_wrap(dur.clone(), FontId::proportional(11.0), Color32::WHITE);
                                let dr = Rect::from_min_size(pos2(r.right() - dg.size().x - 12.0, r.bottom() - 22.0), dg.size() + vec2(8.0, 4.0));
                                ui.painter().rect_filled(dr, 4.0, Color32::from_black_alpha(180));
                                ui.painter().galley(dr.min + vec2(4.0, 2.0), dg, Color32::WHITE);
                                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    open = Some(format!("https://clips.twitch.tv/{}", clip.slug));
                                }
                            } else {
                                ui.allocate_exact_size(vec2(w, h), Sense::hover());
                            }
                        }
                        ui.add_space(4.0);
                        ui.add(egui::Label::new(RichText::new(&clip.title).font(semi(13.0)).color(TEXT)).truncate()).on_hover_text(format!("{}\nClip de {}", clip.title, clip.curator));
                        ui.label(RichText::new(format!("{} vistas  ·  {}", fmt_num(clip.views), clip.game)).font(FontId::proportional(12.0)).color(DIM));
                    });
                }
            });
        });
        if let Some(u) = open {
            c.ctx().open_url(egui::OpenUrl::new_tab(u));
        }
    }

    fn tab_videos(&mut self, c: &mut Ui) {
        if !self.videos_loaded {
            c.label(RichText::new("Cargando vídeos...").color(DIM));
            return;
        }
        if self.videos.is_empty() {
            c.label(RichText::new("Sin vídeos").color(DIM));
            return;
        }
        let w = 280.0;
        let mut open = None;
        egui::ScrollArea::vertical().id_salt("vods_tab").show(c, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
                let px = (w * ui.ctx().pixels_per_point()).round() as u32;
                for vid in &self.videos {
                    ui.allocate_ui_with_layout(vec2(w, 0.0), Layout::top_down(Align::Min), |ui| {
                        ui.set_width(w);
                        let h = w * 9.0 / 16.0;
                        if !vid.thumb.is_empty() {
                            if let img::Look::Ready(f) = self.store.get(self.store.id(&vid.thumb, px)) {
                                let size = vec2(w, h);
                                let (r, resp) = ui.allocate_exact_size(size, Sense::click());
                                egui::Image::from_texture(SizedTexture::new(f.tex, size)).uv(f.uv).corner_radius(6.0).paint_at(ui, r);
                                let dur = format!("{}:{:02}:{:02}", vid.length / 3600, vid.length % 3600 / 60, vid.length % 60);
                                let dg = ui.painter().layout_no_wrap(dur.clone(), FontId::proportional(11.0), Color32::WHITE);
                                let dr = Rect::from_min_size(pos2(r.right() - dg.size().x - 12.0, r.bottom() - 22.0), dg.size() + vec2(8.0, 4.0));
                                ui.painter().rect_filled(dr, 4.0, Color32::from_black_alpha(180));
                                ui.painter().galley(dr.min + vec2(4.0, 2.0), dg, Color32::WHITE);
                                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    open = Some(format!("https://www.twitch.tv/videos/{}", vid.id));
                                }
                            } else {
                                ui.allocate_exact_size(vec2(w, h), Sense::hover());
                            }
                        }
                        ui.add_space(4.0);
                        ui.add(egui::Label::new(RichText::new(&vid.title).font(semi(13.0)).color(TEXT)).truncate()).on_hover_text(format!("{}\n{}", vid.title, vid.date));
                        ui.label(RichText::new(format!("{} vistas  ·  {}", fmt_num(vid.views), vid.game)).font(FontId::proportional(12.0)).color(DIM));
                    });
                }
            });
        });
        if let Some(u) = open {
            c.ctx().open_url(egui::OpenUrl::new_tab(u));
        }
    }

    /// `overlay`: dibujado encima del video, sin fondo propio (lo pone el vidrio translúcido).
    fn chat(&mut self, ui: &mut Ui, overlay: bool) {
        let bg = if overlay { Color32::TRANSPARENT } else { PANEL };
        let line = if overlay { Color32::from_white_alpha(20) } else { LINE };
        let mut gear = Rect::NOTHING;
        let mut users = Rect::NOTHING;
        egui::Panel::top("chat_head").exact_size(if overlay { 30.0 } else { 40.0 }).frame(egui::Frame::new().fill(bg).stroke(Stroke::new(1.0, line))).show(ui, |ui| {
            let r = ui.max_rect();
            let btn = |i: f32| Rect::from_min_size(pos2(r.right() - 34.0 * i, r.center().y - 15.0), vec2(30.0, 30.0));
            if overlay {
                // agarradera (puntos) + botones: cambiar de lado, ajustes, volver al chat lateral
                for k in 0..6 {
                    let d = pos2(r.left() + 12.0 + (k % 2) as f32 * 5.0, r.center().y - 5.0 + (k / 2) as f32 * 5.0);
                    ui.painter().circle_filled(d, 1.4, DIM);
                }
                ui.painter().text(pos2(r.left() + 28.0, r.center().y), Align2::LEFT_CENTER, "Chat", semi(12.5), DIM);
                if ui.put(btn(1.0), |ui: &mut Ui| icon_btn(ui, Icon::Theater, false, "Pasar al chat lateral")).clicked() {
                    self.cfg.chat_overlay = false;
                    self.cfg.save();
                }
                let side = if self.cfg.chat_left { Icon::Right } else { Icon::Left };
                if ui.put(btn(3.0), |ui: &mut Ui| icon_btn(ui, side, false, "Cambiar de lado")).clicked() {
                    self.cfg.chat_left = !self.cfg.chat_left;
                    self.cfg.save();
                }
                users = btn(2.0);
            } else {
                // como twitch.tv: ocultar a un lado, título al centro, lista de usuarios al otro
                ui.painter().text(r.center(), Align2::CENTER_CENTER, "Chat del stream", semi(13.5), TEXT);
                let near = Rect::from_min_size(pos2(r.left() + 6.0, r.center().y - 15.0), vec2(30.0, 30.0));
                let (hide, hide_icon, u) = if self.cfg.chat_left { (btn(1.0), Icon::Left, near) } else { (near, Icon::Right, btn(1.0)) };
                if ui.put(hide, |ui: &mut Ui| icon_btn(ui, hide_icon, false, "Ocultar chat")).clicked() {
                    self.chat_open = false;
                }
                users = u;
            }
            if ui.put(users, |ui: &mut Ui| icon_btn(ui, Icon::Users, self.users_open, "Usuarios en el chat")).clicked() {
                self.users_open = !self.users_open;
            }
        });
        let mut insert = None;
        let mut rewards_at = None;
        egui::Panel::bottom("say").frame(egui::Frame::new().fill(bg).inner_margin(Margin::symmetric(10, 8))).show(ui, |ui| {
            if self.picker {
                self.picker(ui, &mut insert);
                ui.add_space(6.0);
            }
            if let Some((_, name, body)) = self.reply.clone() {
                ui.horizontal(|ui| {
                    paint_icon(ui.painter(), Rect::from_min_size(ui.cursor().min + vec2(0.0, 3.0), vec2(14.0, 12.0)), Icon::Reply, DIM);
                    ui.add_space(20.0);
                    ui.add(egui::Label::new(RichText::new(format!("Respondiendo a @{name}: {body}")).color(DIM).small()).truncate());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let (r, x) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
                        paint_icon(ui.painter(), r.shrink(5.0), Icon::Close, TEXT);
                        if x.clicked() {
                            self.reply = None;
                        }
                    });
                });
                ui.add_space(4.0);
            }
            let modes = self.modes_line();
            if !modes.is_empty() {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                    paint_icon(ui.painter(), r.shrink(1.0), Icon::Info, DIM);
                    ui.add_space(4.0);
                    // con wrap: una línea de modos larga ensanchaba el panel y cortaba el botón Enviar
                    ui.add(egui::Label::new(RichText::new(modes).font(semi(12.5)).color(TEXT)).wrap());
                });
                ui.add_space(4.0);
            }
            let id = egui::Id::unique("chat_input");
            // sugerencias al escribir "@usuario" o ":emote", como twitch.tv
            let sugg = self.suggestions();
            if !sugg.is_empty() && ui.memory(|m| m.has_focus(id)) {
                let px = self.px.emote;
                let mut chosen = None;
                egui::Frame::new().fill(SIDE).corner_radius(R).inner_margin(4).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (text, url) in &sugg {
                        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(r, 4.0, HOVER);
                        }
                        let mut x = r.left() + 8.0;
                        if let Some(u) = url {
                            if let img::Look::Ready(f) = self.store.get(self.store.id(u, px)) {
                                ui.painter().image(f.tex, fit(Rect::from_min_size(pos2(x, r.top() + 4.0), vec2(22.0, 22.0)), &f), f.uv, Color32::WHITE);
                            }
                            x += 30.0;
                        }
                        ui.painter().text(pos2(x, r.center().y), Align2::LEFT_CENTER, text, FontId::proportional(13.5), TEXT);
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            chosen = Some(text.clone());
                        }
                    }
                });
                if let Some(c) = chosen {
                    self.apply_suggestion(&c);
                    ui.memory_mut(|m| m.request_focus(id));
                }
                ui.add_space(6.0);
            }
            let tab = ui.memory(|m| m.has_focus(id)) && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab));
            let logged = self.me.is_some();
            let hint = if logged { "Enviar un mensaje" } else { "Inicia sesión para chatear" };
            let mut send = false;
            // caja como la de twitch.tv: borde gris, morado al escribir, botón de emotes adentro a la derecha
            let focused = ui.memory(|m| m.has_focus(id));
            let border = if focused { PURPLE_L } else { Color32::from_rgb(64, 64, 70) };
            let w = ui.available_width();
            egui::Frame::new().fill(BG).stroke(Stroke::new(if focused { 2.0 } else { 1.0 }, border)).corner_radius(6).inner_margin(Margin { left: 10, right: 2, top: 3, bottom: 3 }).show(ui, |ui| {
                ui.allocate_ui_with_layout(vec2(w - 12.0, 30.0), Layout::left_to_right(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let r = ui.add_enabled(logged, egui::TextEdit::singleline(&mut self.input).id(id).hint_text(hint).char_limit(500).frame(egui::Frame::NONE).desired_width(ui.available_width() - 32.0).margin(vec2(0.0, 6.0)));
                    if r.changed() {
                        self.tab = None;
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        send = true;
                        r.request_focus();
                    }
                    if icon_btn(ui, Icon::Smile, self.picker, "Emotes").clicked() {
                        self.picker = !self.picker;
                    }
                });
            });
            if tab && self.complete() {
                if let Some(mut st) = egui::TextEdit::load_state(ui.ctx(), id) {
                    let end = egui::text::CCursor::new(self.input.chars().count());
                    st.cursor.set_char_range(Some(egui::text::CCursorRange::one(end)));
                    st.store(ui.ctx(), id);
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                // como twitch.tv: Bits y puntos del canal a la izquierda
                if let Some(b) = self.bits {
                    let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                    paint_icon(ui.painter(), r.shrink(2.0), Icon::Bits, PURPLE_L);
                    ui.label(RichText::new(fmt_num(b)).font(semi(14.0)).color(TEXT)).on_hover_text("Tus Bits");
                    ui.add_space(8.0);
                }
                if let Some(b) = self.points {
                    // como twitch.tv: clic en los puntos abre las recompensas del canal
                    let r = ui.add(egui::Button::new(RichText::new(format!("     {}", fmt_num(b))).font(semi(14.0)).color(TEXT)).fill(if self.rewards_open { HOVER } else { Color32::TRANSPARENT }).corner_radius(4));
                    paint_icon(ui.painter(), Rect::from_min_size(r.rect.left_center() + vec2(8.0, -8.0), vec2(16.0, 16.0)).shrink(1.0), Icon::Points, PURPLE_L);
                    rewards_at = Some(r.rect);
                    if r.on_hover_text("Recompensas del canal").clicked() {
                        self.rewards_open = !self.rewards_open;
                        if self.rewards_open {
                            crate::twitch::load_rewards(&self.net, self.session(), self.chan.clone());
                        }
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add_enabled(logged, egui::Button::new(RichText::new("Enviar").font(semi(14.0)).color(Color32::WHITE)).fill(PURPLE).corner_radius(17).min_size(vec2(72.0, 34.0))).clicked() {
                        send = true;
                    }
                    let g = icon_btn(ui, Icon::Gear, self.chat_menu, "Ajustes del chat");
                    if g.clicked() {
                        self.chat_menu = !self.chat_menu;
                    }
                    gear = g.rect;
                    if !logged && clickable_text(ui, RichText::new("Iniciar sesión").color(PURPLE_L)).clicked() {
                        self.login_open = true;
                    }
                });
            });
            if send {
                self.send();
            }
        });
        if let Some(name) = insert {
            if !self.input.is_empty() && !self.input.ends_with(' ') {
                self.input.push(' ');
            }
            self.input.push_str(&name);
            self.input.push(' ');
        }

        let mut act = None;
        let logged = self.me.is_some();
        egui::CentralPanel::default().frame(egui::Frame::new().fill(bg).inner_margin(Margin { left: 6, right: 4, top: 4, bottom: 4 })).show(ui, |ui| {
            let area = ui.max_rect();
            // Como twitch.tv: el chat sigue al fondo hasta que TÚ subes (rueda o arrastrar la barra);
            // ahí se pausa. Vuelve a seguir al llegar al fondo o con el aviso. No se usa stick_to_bottom
            // de egui porque los altos estimados de mensajes aún no medidos lo "despegaban" solo.
            if std::mem::take(&mut self.jump_bottom) {
                self.chat_stuck = true;
            }
            // subir = rueda hacia arriba, o agarrar la barra de scroll (franja derecha del chat)
            let bar = Rect::from_min_max(pos2(area.right() - 14.0, area.top()), area.max);
            let grab = ui.input(|i| i.pointer.primary_pressed() && i.pointer.interact_pos().is_some_and(|p| bar.contains(p)));
            let wheel_up = (ui.rect_contains_pointer(area) && ui.input(|i| i.smooth_scroll_delta.y > 0.5)) || grab;
            if wheel_up {
                self.chat_stuck = false;
            }
            // "Pausar chat" de twitch.tv: además del scroll, con el cursor encima y/o manteniendo Alt
            let over = ui.rect_contains_pointer(area);
            let alt = ui.input(|i| i.modifiers.alt);
            let hold = match self.cfg.pause_mode {
                1 => over,
                2 => alt,
                3 => over || alt,
                _ => false,
            };
            let follow = self.chat_stuck && !hold;
            let mut sa = egui::ScrollArea::vertical().auto_shrink(false).id_salt("chat_scroll");
            if follow {
                // el alto real (no f32::MAX): con un offset infinito el recorte de mensajes visibles no pintaba nada
                sa = sa.vertical_scroll_offset((self.chat_content_h - area.height() + 8.0).max(0.0));
            }
            let out = sa.show_viewport(ui, |ui, view| {
                ui.spacing_mut().item_spacing = vec2(0.0, 1.0);
                let top = ui.min_rect().top();
                // Solo se arman (y animan) los mensajes visibles; el resto ocupa su alto medido.
                for m in self.msgs.iter_mut() {
                    let y = ui.cursor().top() - top;
                    if y + m.h < view.min.y - 100.0 || y > view.max.y + 100.0 {
                        ui.allocate_space(vec2(ui.available_width(), m.h));
                        continue;
                    }
                    if let Some(a) = draw_msg(ui, m, &self.store, &self.cfg, logged) {
                        act = Some(a);
                    }
                    m.h = (ui.cursor().top() - top - y - ui.spacing().item_spacing.y).max(1.0);
                }
            });
            let at_bottom = out.state.clamped_offset().y + out.inner_rect.height() >= out.content_size.y - 4.0;
            if !self.chat_stuck && at_bottom && !wheel_up {
                self.chat_stuck = true; // bajaste hasta el final: vuelve a seguir el chat
            }
            // si el contenido cambió de alto (mensajes recién medidos), un cuadro más para asentar el fondo
            if follow && (out.content_size.y - self.chat_content_h).abs() > 0.5 {
                ui.ctx().request_repaint();
            }
            self.chat_content_h = out.content_size.y;
            if !follow {
                // como Twitch: al subir, el chat se pausa y aparece el aviso para volver
                let why = if self.chat_stuck { if alt && self.cfg.pause_mode >= 2 { "Chat en pausa (Alt)" } else { "Chat en pausa por pasar el cursor" } } else { "Chat en pausa por desplazamiento  ↓" };
                let r = Rect::from_center_size(pos2(area.center().x, area.bottom() - 26.0), vec2(area.width() - 24.0, 32.0));
                let b = ui.put(r, egui::Button::new(RichText::new(why).font(semi(13.0)).color(TEXT)).fill(Color32::from_rgb(47, 47, 53)).corner_radius(4));
                if b.clicked() {
                    self.jump_bottom = true;
                }
            }
        });
        match act {
            Some(MsgAct::Name(login, pos)) => {
                self.card = None;
                self.card_pos = pos;
                crate::twitch::load_card(&self.net, login);
            }
            Some(MsgAct::Reply(id, name, body)) => self.reply = Some((id, name, body)),
            None => {}
        }
        if self.chat_menu {
            self.chat_settings(ui, gear);
        }
        if self.users_open {
            self.users_list(ui, users);
        }
        match rewards_at {
            Some(r) if self.rewards_open => self.rewards_list(ui, r),
            _ => self.rewards_open = false,
        }
    }

    /// Recompensas de puntos del canal (como el popup de twitch.tv). Solo se muestran: canjear gasta
    /// puntos reales y twitch.tv lo protege con su verificación anti-bots, así que se hace allá.
    fn rewards_list(&mut self, ui: &mut Ui, anchor: Rect) {
        let px = (40.0 * ui.ctx().pixels_per_point()).round() as u32;
        let chan = self.chan.clone();
        let area = egui::Area::new(egui::Id::unique("rewards")).order(egui::Order::Foreground).pivot(Align2::LEFT_BOTTOM).fixed_pos(anchor.left_top() - vec2(0.0, 6.0)).show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).fill(PANEL).corner_radius(8).inner_margin(12).show(ui, |ui| {
                ui.set_width(320.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Recompensas de {}", self.info.name)).font(semi(14.0)).color(TEXT));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(b) = self.points {
                            ui.label(RichText::new(fmt_num(b)).font(semi(13.0)).color(PURPLE_L));
                        }
                    });
                });
                ui.add_space(8.0);
                if self.rewards.0 != chan {
                    hint(ui, "Cargando…");
                    return;
                }
                if self.rewards.1.is_empty() {
                    hint(ui, "Este canal no tiene recompensas personalizadas activas.");
                }
                let mine = self.points.unwrap_or(0);
                egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                    // rejilla de 3 como en twitch.tv: fondo del color de la recompensa, ícono y costo
                    for row in self.rewards.1.chunks(3) {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            for rw in row {
                                let (r, resp) = ui.allocate_exact_size(vec2(96.0, 112.0), Sense::hover());
                                let bg = crate::name_color(&rw.color, "", false);
                                let p = ui.painter();
                                p.rect_filled(Rect::from_min_size(r.min, vec2(r.width(), 64.0)), 6.0, bg);
                                if let img::Look::Ready(f) = self.store.get(self.store.id(&rw.image, px)) {
                                    p.image(f.tex, fit(Rect::from_center_size(r.min + vec2(48.0, 32.0), vec2(40.0, 40.0)), &f), f.uv, Color32::WHITE);
                                }
                                let can = mine >= rw.cost;
                                let cost = Rect::from_center_size(pos2(r.center().x, r.top() + 64.0), vec2(64.0, 20.0));
                                p.rect_filled(cost, 10.0, if can { PURPLE } else { Color32::from_rgb(47, 47, 53) });
                                p.text(cost.center(), Align2::CENTER_CENTER, short_num(rw.cost), semi(12.0), Color32::WHITE);
                                let mut job = LayoutJob::simple(rw.title.clone(), FontId::proportional(12.0), TEXT, r.width());
                                job.wrap.max_rows = 2;
                                job.halign = Align::Center;
                                let g = p.layout_job(job);
                                p.galley(pos2(r.center().x, r.top() + 80.0), g, TEXT);
                                let tip = if rw.prompt.is_empty() { rw.title.clone() } else { format!("{}\n{}", rw.title, rw.prompt) };
                                resp.on_hover_text(tip);
                            }
                        });
                        ui.add_space(8.0);
                    }
                });
                ui.add_space(4.0);
                if clickable_text(ui, RichText::new("Canjear en twitch.tv ↗").color(PURPLE_L)).clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(format!("https://www.twitch.tv/{chan}")));
                }
            });
        });
        let pos = ui.input(|i| i.pointer.interact_pos()).unwrap_or_default();
        if ui.input(|i| i.pointer.any_click()) && !area.response.rect.contains(pos) && !anchor.contains(pos) {
            self.rewards_open = false;
        }
    }

    /// Lista de quienes han hablado (Twitch ya no publica la lista completa de espectadores sin ser moderador).
    fn users_list(&mut self, ui: &mut Ui, anchor: Rect) {
        let mut pick = None;
        let area = egui::Area::new(egui::Id::unique("users")).order(egui::Order::Foreground).pivot(Align2::RIGHT_TOP).fixed_pos(anchor.right_bottom() + vec2(0.0, 4.0)).show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).fill(PANEL).corner_radius(6).inner_margin(12).show(ui, |ui| {
                ui.set_width(240.0);
                ui.label(RichText::new(format!("En el chat ({})", self.chatters.len())).font(semi(14.0)).color(TEXT));
                ui.label(RichText::new("Quienes han escrito desde que entraste").color(DIM).small());
                ui.separator();
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for n in self.chatters.iter().rev() {
                        if clickable_text(ui, RichText::new(n).color(TEXT)).clicked() {
                            pick = Some((n.clone(), ui.input(|i| i.pointer.interact_pos()).unwrap_or_default()));
                        }
                    }
                });
            });
        });
        let pos = ui.input(|i| i.pointer.interact_pos()).unwrap_or_default();
        if let Some((n, p)) = pick {
            self.users_open = false;
            self.card_pos = p;
            crate::twitch::load_card(&self.net, n.to_lowercase());
        } else if ui.input(|i| i.pointer.any_click()) && !area.response.rect.contains(pos) && !anchor.contains(pos) {
            self.users_open = false;
        }
    }

    /// Ajustes del chat con las mismas páginas que el engrane de twitch.tv.
    fn chat_settings(&mut self, ui: &mut Ui, gear: Rect) {
        let screen = ui.ctx().content_rect();
        // se abre hacia donde haya espacio (el chat flotante tiene el engrane arriba)
        let up = gear.center().y > screen.center().y;
        let (pivot, at) = if up { (Align2::RIGHT_BOTTOM, gear.right_top() - vec2(0.0, 6.0)) } else { (Align2::RIGHT_TOP, gear.right_bottom() + vec2(0.0, 6.0)) };
        let max_h = if up { gear.top() - screen.top() - 60.0 } else { screen.bottom() - gear.bottom() - 20.0 };
        let mut save = false;
        let area = egui::Area::new(egui::Id::unique("chat_menu")).order(egui::Order::Foreground).pivot(pivot).fixed_pos(at).show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).fill(PANEL).corner_radius(8).inner_margin(10).show(ui, |ui| {
                ui.set_width(300.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                egui::ScrollArea::vertical().max_height(max_h.max(200.0)).auto_shrink([false, true]).show(ui, |ui| {
                    let page = self.chat_page;
                    if page != 0 {
                        let title = ["", "Identidad en el chat", "Filtros del chat", "Pausar chat", "Apariencia del chat", "Posición del chat"][page as usize % 6];
                        if page_head(ui, title) {
                            self.chat_page = 0;
                        }
                    }
                    match page {
                        1 => self.identity_page(ui),
                        2 => {
                            save |= toggle_row(ui, "Habilitar filtros de chat", &mut self.cfg.filter);
                            hint(ui, "Twitch marca cada mensaje por categoría; lo marcado se tapa con *.");
                            ui.add_enabled_ui(self.cfg.filter, |ui| {
                                for (bit, label) in [(1u8, "Discriminación"), (2, "Lenguaje sexualmente explícito"), (4, "Hostilidad"), (8, "Lenguaje soez")] {
                                    let mut on = self.cfg.filter_cats & bit != 0;
                                    if toggle_row(ui, label, &mut on) {
                                        self.cfg.filter_cats ^= bit;
                                        save = true;
                                    }
                                }
                            });
                            menu_sep(ui);
                            save |= toggle_row(ui, "Ocultar mensajes con enlaces", &mut self.cfg.hide_links);
                            ui.add_space(6.0);
                            ui.label(RichText::new("Palabras bloqueadas, separadas por coma").color(DIM).small());
                            ui.add_space(4.0);
                            save |= ui.add(egui::TextEdit::singleline(&mut self.cfg.blocked).hint_text("spoiler, palabra").desired_width(f32::INFINITY)).changed();
                            ui.add_space(4.0);
                        }
                        3 => {
                            hint(ui, "Mientras el chat está en pausa no se mueve; lo nuevo espera abajo.");
                            for (i, label) in ["Solo al desplazar", "Al pasar el cursor", "Mantener presionado Alt", "Pasar el cursor o mantener Alt"].iter().enumerate() {
                                if radio_row(ui, label, self.cfg.pause_mode == i as u8) {
                                    self.cfg.pause_mode = i as u8;
                                    save = true;
                                }
                            }
                        }
                        4 => {
                            ui.label(RichText::new("Tamaño de fuente").color(DIM).small());
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                for (size, label) in [(12.0, "Chico"), (13.5, "Mediano"), (15.5, "Grande"), (18.0, "Enorme")] {
                                    if ui.add(egui::Button::selectable((self.cfg.chat_font - size).abs() < 0.3, label).min_size(vec2(68.0, 28.0))).clicked() {
                                        self.cfg.chat_font = size;
                                        save = true;
                                    }
                                }
                            });
                            ui.add_space(6.0);
                            save |= toggle_row(ui, "Colores legibles", &mut self.cfg.readable);
                            if toggle_row(ui, "Emotes animados", &mut self.cfg.animate) {
                                self.store.animate = self.cfg.animate;
                                save = true;
                            }
                            save |= toggle_row(ui, "Congelar animaciones sin foco", &mut self.cfg.pause_unfocused);
                            save |= toggle_row(ui, "Mostrar respuestas en el chat", &mut self.cfg.replies);
                            save |= toggle_row(ui, "Mostrar marcas de tiempo", &mut self.cfg.timestamps);
                            menu_sep(ui);
                            ui.label(RichText::new("Mensajes eliminados").color(DIM).small());
                            for (i, label) in ["Quitarlos", "Mostrar \"<mensaje eliminado>\"", "Mostrar tachados"].iter().enumerate() {
                                if radio_row(ui, label, self.cfg.deleted_mode == i as u8) {
                                    self.cfg.deleted_mode = i as u8;
                                    save = true;
                                }
                            }
                        }
                        5 => {
                            for (over, label) in [(false, "Al lado del video"), (true, "Encima del video")] {
                                if radio_row(ui, label, self.cfg.chat_overlay == over) {
                                    self.cfg.chat_overlay = over;
                                    save = true;
                                }
                            }
                            menu_sep(ui);
                            for (left, label) in [(true, "A la izquierda"), (false, "A la derecha")] {
                                if radio_row(ui, label, self.cfg.chat_left == left) {
                                    self.cfg.chat_left = left;
                                    save = true;
                                }
                            }
                        }
                        _ => {
                            if self.me.is_some() {
                                if nav_row(ui, Some(Icon::Chat), "Identidad en el chat", "") {
                                    self.chat_page = 1;
                                    if let Some(t) = self.session() {
                                        crate::twitch::load_identity(&self.net, t, self.chan.clone());
                                    }
                                }
                                menu_sep(ui);
                            }
                            ui.label(RichText::new("Ajustes del chat").font(semi(12.5)).color(DIM));
                            ui.add_space(4.0);
                            save |= toggle_row(ui, "Mostrar marcas de tiempo", &mut self.cfg.timestamps);
                            let pages: [(&str, String, u8); 4] = [
                                ("Filtros del chat", (if self.cfg.filter { "Activados" } else { "Desactivados" }).into(), 2),
                                ("Pausar chat", ["Desplazar", "Cursor", "Alt", "Cursor o Alt"][self.cfg.pause_mode as usize % 4].into(), 3),
                                ("Apariencia del chat", String::new(), 4),
                                ("Posición del chat", format!("{} · {}", if self.cfg.chat_overlay { "Encima" } else { "Lateral" }, if self.cfg.chat_left { "izq." } else { "der." }), 5),
                            ];
                            for (label, value, p) in pages {
                                if nav_row(ui, None, label, &value) {
                                    self.chat_page = p;
                                }
                            }
                            menu_sep(ui);
                            if menu_row(ui, Icon::Users, "Usuarios en el chat", false).clicked() {
                                self.users_open = true;
                                self.chat_menu = false;
                            }
                            if menu_row(ui, Icon::Close, "Ocultar chat", false).clicked() {
                                self.chat_open = false;
                                self.chat_menu = false;
                            }
                            if menu_row(ui, Icon::Chat, if self.cfg.chat_popout { "Regresar el chat a la ventana" } else { "Chat emergente" }, false).clicked() {
                                self.cfg.chat_popout = !self.cfg.chat_popout;
                                self.chat_menu = false;
                            }
                        }
                    }
                });
            });
        });
        if save {
            self.cfg.save();
        }
        let pos = ui.input(|i| i.pointer.interact_pos()).unwrap_or_default();
        if ui.input(|i| i.pointer.any_click()) && !area.response.rect.contains(pos) && !gear.contains(pos) {
            self.chat_menu = false;
            self.chat_page = 0;
        }
    }

    /// Abre (o cierra) la bandeja de notificaciones en el menú de usuario y la recarga.
    fn open_notifs(&mut self) {
        if self.menu_open && self.menu_page == 2 {
            self.menu_open = false;
            return;
        }
        self.menu_open = true;
        self.menu_page = 2;
        if let Some(t) = self.session() {
            crate::twitch::load_notifs(&self.net, t);
        }
    }

    /// "Identidad en el chat": vista previa, color del nombre y emblemas (global y del canal).
    fn identity_page(&mut self, ui: &mut Ui) {
        let Some(token) = self.session() else {
            hint(ui, "Para cambiar color y emblemas entra con tu sesión de twitch.tv (menú › Iniciar sesión).");
            return;
        };
        let Some(id) = &self.identity else {
            hint(ui, "Cargando…");
            return;
        };
        let px = (22.0 * ui.ctx().pixels_per_point()).round() as u32;
        let login = self.me.as_ref().map(|m| m.login.clone()).unwrap_or_default();
        // vista previa como en twitch.tv: emblemas elegidos + nombre en su color
        egui::Frame::new().fill(BG).corner_radius(6).inner_margin(10).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                for b in id.globals.iter().chain(&id.chans).filter(|b| format!("{}/{}", b.set, b.ver) == id.global_sel || format!("{}/{}", b.set, b.ver) == id.chan_sel) {
                    image(ui, &self.store, self.store.id(&b.url, px), BADGE_H);
                }
                ui.label(RichText::new(&login).font(semi(14.0)).color(crate::name_color(&id.color, &login, self.cfg.readable)));
                ui.label(RichText::new(": ¡Hola!").color(TEXT));
            });
        });
        ui.add_space(10.0);
        ui.label(RichText::new("Color del nombre").font(semi(13.0)).color(TEXT));
        ui.add_space(6.0);
        // los 15 colores de twitch.tv para cualquier cuenta
        const COLORS: [&str; 15] = ["#FF0000", "#0000FF", "#00FF00", "#B22222", "#FF7F50", "#9ACD32", "#FF4500", "#2E8B57", "#DAA520", "#D2691E", "#5F9EA0", "#1E90FF", "#FF69B4", "#8A2BE2", "#00FF7F"];
        let mut pick = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            for hex in COLORS {
                let (r, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
                let on = id.color.eq_ignore_ascii_case(hex);
                if on || resp.hovered() {
                    ui.painter().circle_stroke(r.center(), 12.5, Stroke::new(2.0, if on { TEXT } else { DIM }));
                }
                ui.painter().circle_filled(r.center(), 9.5, crate::name_color(hex, "", false));
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !on {
                    pick = Some(hex.to_string());
                }
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.hex_input).hint_text("#9147FF (Turbo/Prime)").char_limit(7).desired_width(150.0));
            let ok = self.hex_input.len() == 7 && self.hex_input.starts_with('#') && u32::from_str_radix(&self.hex_input[1..], 16).is_ok();
            if ui.add_enabled(ok, egui::Button::new("Usar color")).clicked() {
                pick = Some(self.hex_input.to_uppercase());
            }
        });
        let mut badge = None;
        for (title, list, sel, chan) in [("Emblema global", &id.globals, &id.global_sel, false), ("Emblema del canal", &id.chans, &id.chan_sel, true)] {
            if list.is_empty() {
                continue;
            }
            ui.add_space(10.0);
            ui.label(RichText::new(title).font(semi(13.0)).color(TEXT));
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                for b in list {
                    let (r, resp) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click());
                    let on = &format!("{}/{}", b.set, b.ver) == sel;
                    if on {
                        ui.painter().rect_stroke(r, 6.0, Stroke::new(2.0, PURPLE_L), egui::StrokeKind::Inside);
                    } else if resp.hovered() {
                        ui.painter().rect_filled(r, 6.0, HOVER);
                    }
                    if let img::Look::Ready(f) = self.store.get(self.store.id(&b.url, px)) {
                        ui.painter().image(f.tex, fit(r.shrink(6.0), &f), f.uv, Color32::WHITE);
                    }
                    if resp.on_hover_text(&b.title).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !on {
                        badge = Some((b.clone(), chan));
                    }
                }
            });
        }
        ui.add_space(6.0);
        if let Some(c) = pick {
            crate::twitch::set_color(&self.net, token.clone(), self.chan.clone(), c);
        }
        if let Some((b, chan)) = badge {
            let chan_id = if chan { id.chan_id.clone() } else { String::new() };
            crate::twitch::set_badge(&self.net, token, self.chan.clone(), chan_id, b);
        }
    }

    fn picker(&mut self, ui: &mut Ui, insert: &mut Option<String>) {
        egui::Frame::new().fill(SIDE).corner_radius(6).inner_margin(8).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.picker_tab, 0, "Canal");
                ui.selectable_value(&mut self.picker_tab, 1, "Mis emotes");
                ui.add(egui::TextEdit::singleline(&mut self.picker_q).hint_text("Buscar").desired_width(f32::INFINITY));
            });
            let q = self.picker_q.to_lowercase();
            let src = if self.picker_tab == 0 { &self.chan_emotes } else { &self.my_emotes };
            let list: Vec<&(String, String)> = src.iter().filter(|e| q.is_empty() || e.0.to_lowercase().contains(&q)).collect();
            let per_row = ((ui.available_width() / 38.0) as usize).max(1);
            let px = self.px.emote;
            // show_rows: solo se construyen (y descargan) las filas visibles
            egui::ScrollArea::vertical().max_height(200.0).auto_shrink([false, true]).show_rows(ui, 36.0, list.len().div_ceil(per_row), |ui, rows| {
                for row in rows {
                    ui.horizontal(|ui| {
                        for e in list.iter().skip(row * per_row).take(per_row) {
                            let (r, resp) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::click());
                            if resp.hovered() {
                                ui.painter().rect_filled(r, 4.0, HOVER);
                            }
                            if let img::Look::Ready(f) = self.store.get(self.store.id(&e.1, px)) {
                                ui.painter().image(f.tex, fit(r.shrink(4.0), &f), f.uv, Color32::WHITE);
                            }
                            if resp.on_hover_text(&e.0).clicked() {
                                *insert = Some(e.0.clone());
                            }
                        }
                    });
                }
            });
        });
    }

    fn overlays(&mut self, ui: &mut Ui) {
        let pos = ui.input(|i| i.pointer.interact_pos()).unwrap_or_default();
        let clicked = ui.input(|i| i.pointer.any_click());
        if self.menu_open {
            let screen = ui.ctx().content_rect();
            let mut nav: Option<Page> = None;
            let mut join = None;
            let mut on_top = None;
            let area = egui::Area::new(egui::Id::unique("menu")).order(egui::Order::Foreground).pivot(Align2::RIGHT_TOP).fixed_pos(pos2(screen.right() - 10.0, 52.0)).show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).fill(PANEL).corner_radius(8).inner_margin(10).show(ui, |ui| {
                    ui.set_width(300.0);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    egui::ScrollArea::vertical().max_height(screen.height() - 80.0).auto_shrink([false, true]).show(ui, |ui| {
                        let page = self.menu_page;
                        if page != 0 {
                            let title = ["", "Identidad en el chat", "Notificaciones", "Cartera", "Ajustes de la app", "Atajos de teclado"][page as usize % 6];
                            if page_head(ui, title) {
                                self.menu_page = 0;
                            }
                        }
                        let login = self.me.as_ref().map(|m| m.login.clone());
                        match page {
                            1 => self.identity_page(ui),
                            2 => {
                                if self.notifs.is_empty() {
                                    hint(ui, if self.session().is_some() { "No tienes notificaciones." } else { "Entra con tu sesión de twitch.tv para ver tus notificaciones." });
                                }
                                let px = (40.0 * ui.ctx().pixels_per_point()).round() as u32;
                                for n in &self.notifs {
                                    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::hover());
                                    let p = ui.painter();
                                    if !n.read {
                                        p.circle_filled(pos2(r.right() - 6.0, r.center().y), 4.0, PURPLE_L);
                                    }
                                    let a = Rect::from_min_size(r.min + vec2(4.0, 9.0), vec2(40.0, 40.0));
                                    if let img::Look::Ready(f) = self.store.get(self.store.id(&n.thumb, px)) {
                                        egui::Image::from_texture(SizedTexture::new(f.tex, a.size())).uv(f.uv).corner_radius(20.0).paint_at(ui, a);
                                    } else {
                                        p.circle_filled(a.center(), 20.0, HOVER);
                                    }
                                    let mut job = LayoutJob::simple(n.body.clone(), FontId::proportional(13.0), TEXT, r.width() - 70.0);
                                    job.wrap.max_rows = 2;
                                    p.galley(pos2(a.right() + 10.0, r.top() + 6.0), p.layout_job(job), TEXT);
                                    p.text(pos2(a.right() + 10.0, r.bottom() - 6.0), Align2::LEFT_BOTTOM, &n.date, FontId::proportional(11.5), DIM);
                                }
                            }
                            3 => {
                                ui.horizontal(|ui| {
                                    let (r, _) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
                                    paint_icon(ui.painter(), r.shrink(3.0), Icon::Bits, PURPLE_L);
                                    ui.add_space(6.0);
                                    ui.label(RichText::new(self.bits.map(fmt_num).unwrap_or_else(|| "—".into())).font(semi(26.0)).color(TEXT));
                                    ui.label(RichText::new(" Bits").color(DIM));
                                });
                                ui.add_space(6.0);
                                hint(ui, "Comprar Bits y pagar suscripciones pasa por el checkout de twitch.tv.");
                                for (label, url) in [("Comprar Bits", "https://www.twitch.tv/bits"), ("Suscripciones y pagos", "https://www.twitch.tv/wallet")] {
                                    if menu_row(ui, Icon::Bits, label, true).clicked() {
                                        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                                    }
                                }
                            }
                            4 => {
                                let mut save = false;
                                let mut top = self.cfg.on_top;
                                if toggle_row(ui, "Siempre encima", &mut top) {
                                    on_top = Some(top);
                                }
                                save |= toggle_row(ui, "Notificaciones de escritorio", &mut self.cfg.notify);
                                hint(ui, "Canales que entran en directo, menciones, respuestas y susurros.");
                                save |= toggle_row(ui, "Baja latencia", &mut self.cfg.low_latency);
                                if toggle_row(ui, "Emotes animados", &mut self.cfg.animate) {
                                    self.store.animate = self.cfg.animate;
                                    save = true;
                                }
                                save |= toggle_row(ui, "Congelar animaciones sin foco", &mut self.cfg.pause_unfocused);
                                save |= toggle_row(ui, "Barra lateral abierta", &mut self.cfg.sidebar);
                                save |= toggle_row(ui, "Reclamar bonus de puntos", &mut self.cfg.auto_claim);
                                hint(ui, "Reclama el bonus de puntos del canal automáticamente cada 60 s.");
                                if save {
                                    self.cfg.save();
                                }
                                menu_sep(ui);
                                let (used, budget, n) = self.store.stats();
                                hint(ui, &format!("Imágenes en memoria: {used}/{budget} MB · {n}"));
                                menu_sep(ui);
                                ui.horizontal(|ui| {
                                    let (r, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
                                    paint_logo(ui.painter(), r);
                                    ui.vertical(|ui| {
                                        ui.label(RichText::new(crate::branding::NAME).font(semi(13.0)).color(TEXT));
                                        ui.label(RichText::new(format!("v{} · Romanovak", crate::branding::VERSION)).color(DIM).small());
                                    });
                                });
                            }
                            5 => {
                                for (k, what) in [
                                    ("Espacio / K", "Pausar o reanudar"),
                                    ("M", "Silenciar"),
                                    ("F", "Pantalla completa"),
                                    ("T", "Mini reproductor (modo mínimo)"),
                                    ("C", "Chat encima o al lado"),
                                    ("Esc", "Salir de pantalla completa"),
                                    ("Tab", "Autocompletar nombre o emote"),
                                    ("Alt", "Pausar el chat (si lo activas)"),
                                ] {
                                    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
                                    let g = ui.painter().layout_no_wrap(k.into(), semi(12.0), TEXT);
                                    let kr = Rect::from_min_size(pos2(r.left() + 6.0, r.center().y - 11.0), vec2(g.size().x + 14.0, 22.0));
                                    ui.painter().rect_filled(kr, 4.0, Color32::from_rgb(47, 47, 53));
                                    ui.painter().galley(kr.center() - g.size() / 2.0, g, TEXT);
                                    ui.painter().text(pos2(r.left() + 110.0, r.center().y), Align2::LEFT_CENTER, what, FontId::proportional(13.5), TEXT);
                                }
                            }
                            _ => {
                                if let Some(login) = &login {
                                    ui.horizontal(|ui| {
                                        let (r, _) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::hover());
                                        avatar(ui, &self.store, &self.my_avatar, r, false);
                                        ui.add_space(10.0);
                                        ui.label(RichText::new(login).font(semi(16.0)).color(TEXT));
                                    });
                                    menu_sep(ui);
                                    if menu_row(ui, Icon::Person, "Canal", false).clicked() {
                                        join = Some(login.clone());
                                    }
                                    if menu_row(ui, Icon::Chat, "Identidad en el chat", false).clicked() {
                                        self.menu_page = 1;
                                        if let Some(t) = self.session() {
                                            crate::twitch::load_identity(&self.net, t, self.chan.clone());
                                        }
                                    }
                                    if menu_row(ui, Icon::Bell, "Notificaciones", false).clicked() {
                                        self.open_notifs();
                                        self.menu_open = true;
                                    }
                                    menu_sep(ui);
                                    if menu_row(ui, Icon::Heart, "Suscripciones", false).clicked() {
                                        nav = Some(Page::Subs);
                                    }
                                    if menu_row(ui, Icon::Points, "Drops y recompensas", false).clicked() {
                                        nav = Some(Page::Drops);
                                    }
                                    if nav_row(ui, Some(Icon::Bits), "Cartera", &self.bits.map(|b| format!("{} Bits", fmt_num(b))).unwrap_or_default()) {
                                        self.menu_page = 3;
                                    }
                                    menu_sep(ui);
                                    // Lo que solo existe en twitch.tv se abre ahí (marcado con ↗)
                                    for (icon, label, url) in [
                                        (Icon::Gauge, "Panel de control del creador", format!("https://dashboard.twitch.tv/u/{login}/home")),
                                        (Icon::Video, "Resumen de los streams", format!("https://dashboard.twitch.tv/u/{login}/stream-summary")),
                                        (Icon::Gear, "Configuración de la cuenta", "https://www.twitch.tv/settings/profile".to_string()),
                                        (Icon::Info, "Centro de privacidad", "https://www.twitch.tv/settings/security".to_string()),
                                    ] {
                                        if menu_row(ui, icon, label, true).clicked() {
                                            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                                        }
                                    }
                                    menu_sep(ui);
                                }
                                if menu_row(ui, Icon::Gear, "Ajustes de la app", false).clicked() {
                                    self.menu_page = 4;
                                }
                                if menu_row(ui, Icon::Keyboard, "Atajos de teclado", false).clicked() {
                                    self.menu_page = 5;
                                }
                                menu_sep(ui);
                                if self.me.is_some() {
                                    if menu_row(ui, Icon::Right, "Cerrar sesión", false).clicked() {
                                        crate::twitch::logout();
                                        self.me = None;
                                        self.my_avatar.clear();
                                        self.follows.clear();
                                        self.my_emotes.clear();
                                        self.points = None;
                                        self.bits = None;
                                        self.identity = None;
                                        self.notifs.clear();
                                        self.menu_open = false;
                                        let c = self.chan.clone();
                                        self.join(&c);
                                    }
                                } else if menu_row(ui, Icon::Person, "Iniciar sesión", false).clicked() {
                                    self.login_open = true;
                                    self.menu_open = false;
                                }
                            }
                        }
                    });
                });
            });
            if let Some(top) = on_top {
                self.cfg.on_top = top;
                self.cfg.save();
                let lvl = if top { egui::WindowLevel::AlwaysOnTop } else { egui::WindowLevel::Normal };
                ui.send_viewport_cmd(egui::ViewportCommand::WindowLevel(lvl));
            }
            if let Some(p) = nav {
                self.menu_open = false;
                self.go(p);
            }
            if let Some(c) = join {
                self.menu_open = false;
                self.join(&c);
            }
            if clicked && !area.response.rect.contains(pos) && pos.y > 50.0 {
                self.menu_open = false;
            }
        }
        if let Some(card) = &self.card {
            let mut close = false;
            let mut mention = None;
            let screen = ui.ctx().content_rect();
            let p = pos2(self.card_pos.x.min(screen.right() - 330.0), self.card_pos.y.min(screen.bottom() - 260.0));
            let area = egui::Area::new(egui::Id::unique("card")).order(egui::Order::Foreground).fixed_pos(p).show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).fill(PANEL).corner_radius(8).inner_margin(14).show(ui, |ui| {
                    ui.set_width(300.0);
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(56.0, 56.0), Sense::hover());
                        avatar(ui, &self.store, &card.avatar, r, false);
                        ui.add_space(8.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&card.name).font(semi(17.0)).color(TEXT));
                            ui.label(RichText::new(format!("Cuenta creada el {}", card.created)).color(DIM).small());
                            ui.label(RichText::new(format!("{} seguidores", fmt_num(card.followers))).color(DIM).small());
                        });
                        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                            let (r, x) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
                            paint_icon(ui.painter(), r.shrink(4.0), Icon::Close, TEXT);
                            close = x.clicked();
                        });
                    });
                    if !card.bio.is_empty() {
                        ui.add_space(6.0);
                        ui.label(RichText::new(&card.bio).color(DIM));
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if purple_btn(ui, "Ir al canal").clicked() {
                            mention = Some((card.login.clone(), true));
                        }
                        if ui.add(egui::Button::new("Mencionar").min_size(vec2(0.0, 30.0)).corner_radius(4)).clicked() {
                            mention = Some((card.name.clone(), false));
                        }
                    });
                });
            });
            if clicked && !area.response.rect.contains(pos) {
                close = true;
            }
            match mention {
                Some((login, true)) => {
                    self.card = None;
                    self.join(&login);
                }
                Some((name, false)) => {
                    self.input.push_str(&format!("@{name} "));
                    self.card = None;
                }
                None if close => self.card = None,
                None => {}
            }
        }
        if self.login_open {
            self.login_window(ui);
        }
    }

    fn login_window(&mut self, ui: &mut Ui) {
        let mut open = true;
        egui::Window::new(RichText::new("Iniciar sesión").font(semi(16.0))).open(&mut open).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ui.ctx(), |ui| {
            ui.set_width(360.0);
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
                paint_logo(ui.painter(), r);
                ui.vertical(|ui| {
                    ui.label(RichText::new(crate::branding::NAME).font(semi(20.0)).color(TEXT));
                    ui.label(RichText::new("Conecta tu cuenta de Twitch").color(DIM).small());
                });
            });
            ui.add_space(12.0);
            ui.label(RichText::new("1. Abre Twitch e inicia sesión").font(semi(13.0)).color(TEXT));
            if ui.add(egui::Button::new(RichText::new("Abrir twitch.tv").color(Color32::WHITE)).fill(PURPLE).corner_radius(4)).clicked() {
                let _ = std::process::Command::new("cmd").args(["/C", "start", "https://www.twitch.tv"]).spawn();
            }
            ui.add_space(6.0);
            ui.label(RichText::new("2. Copia tu token de sesión").font(semi(13.0)).color(TEXT));
            ui.label(RichText::new("F12 → Application → Cookies → auth-token").color(DIM).small());
            ui.add_space(6.0);
            ui.label(RichText::new("3. Pégalo aquí").font(semi(13.0)).color(TEXT));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.login_paste).password(true).hint_text("Token o URL").desired_width(260.0));
                if ui.add_enabled(!self.login_paste.trim().is_empty(), egui::Button::new(RichText::new("Entrar").color(Color32::WHITE)).fill(PURPLE).corner_radius(4)).clicked() {
                    self.login_err.clear();
                    crate::twitch::import_web(&self.net, std::mem::take(&mut self.login_paste));
                }
            });
            ui.label(RichText::new("Se guarda en el Administrador de credenciales, nunca en un archivo.").color(DIM).small());
            if !self.login_err.is_empty() {
                ui.label(RichText::new(&self.login_err).color(RED).small());
            }
            ui.add_space(6.0);
            if !self.cfg.client_id.is_empty() || self.device.is_some() {
                egui::CollapsingHeader::new(RichText::new("Device Code (avanzado)").color(DIM).small()).show(ui, |ui| {
                    if self.device.is_none() {
                        if ui.add(egui::Button::new("Iniciar")).clicked() {
                            crate::twitch::device_login(&self.net, self.cfg.client_id.clone());
                        }
                    }
                    if let Some((code, uri)) = &self.device {
                        ui.label("Confirma este código en tu navegador:");
                        ui.label(RichText::new(code).font(semi(28.0)).color(PURPLE_L));
                        ui.hyperlink_to("Abrir de nuevo", uri);
                    }
                });
            }
        });
        self.login_open = open;
    }
}

/// Sin marco de Windows hay que dar bordes para redimensionar: franjas de 5 px en orillas y esquinas,
/// registradas al final para quedar encima de todo.
fn resize_edges(ui: &mut Ui) {
    let (max, fs) = ui.input(|i| (i.viewport().maximized.unwrap_or(false), i.viewport().fullscreen.unwrap_or(false)));
    if max || fs {
        return;
    }
    use egui::{CursorIcon as C, ResizeDirection as D};
    let r = ui.ctx().content_rect();
    let (e, k) = (5.0, 12.0); // grosor del borde y tamaño de las esquinas
    let zones = [
        (Rect::from_min_max(r.min, pos2(r.left() + k, r.top() + k)), D::NorthWest, C::ResizeNwSe),
        (Rect::from_min_max(pos2(r.right() - k, r.top()), pos2(r.right(), r.top() + k)), D::NorthEast, C::ResizeNeSw),
        (Rect::from_min_max(pos2(r.left(), r.bottom() - k), pos2(r.left() + k, r.bottom())), D::SouthWest, C::ResizeNeSw),
        (Rect::from_min_max(r.max - vec2(k, k), r.max), D::SouthEast, C::ResizeNwSe),
        (Rect::from_min_max(r.min, pos2(r.right(), r.top() + e)), D::North, C::ResizeVertical),
        (Rect::from_min_max(pos2(r.left(), r.bottom() - e), r.max), D::South, C::ResizeVertical),
        (Rect::from_min_max(r.min, pos2(r.left() + e, r.bottom())), D::West, C::ResizeHorizontal),
        (Rect::from_min_max(pos2(r.right() - e, r.top()), r.max), D::East, C::ResizeHorizontal),
    ];
    for (i, (z, dir, cursor)) in zones.into_iter().enumerate() {
        let resp = ui.interact(z, egui::Id::unique(("resize", i)), Sense::drag()).on_hover_cursor(cursor);
        if resp.drag_started() {
            ui.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
        }
    }
}

fn section(ui: &mut Ui, title: &str, open: bool) {
    if open {
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.add(egui::Label::new(RichText::new(title).font(semi(14.0)).color(TEXT)).truncate());
        });
        ui.add_space(6.0);
    } else {
        ui.add_space(4.0);
    }
}

fn side_row(ui: &mut Ui, store: &img::Store, l: &Live, open: bool, current: &str) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::click());
    if resp.hovered() || (!current.is_empty() && l.login == current) {
        ui.painter().rect_filled(r, 0.0, HOVER);
    }
    let live = l.viewers.is_some();
    let a = Rect::from_min_size(pos2(r.left() + 11.0, r.center().y - 15.0), vec2(30.0, 30.0));
    avatar(ui, store, &l.avatar, a, !live);
    let p = ui.painter_at(r);
    if open {
        let right = if live { 70.0 } else { 96.0 };
        let w = r.width() - 52.0 - right;
        text_trunc(&p, pos2(r.left() + 50.0, r.top() + 4.0), &l.name, semi(14.0), TEXT, w);
        text_trunc(&p, pos2(r.left() + 50.0, r.top() + 22.0), &l.game, FontId::proportional(13.0), DIM, w);
        match l.viewers {
            Some(v) => {
                // como el sidebar de twitch.tv: "3503" sin separador, "11.336" con separador
                let n = if v < 10_000 { v.to_string() } else { fmt_num(v) };
                let g = p.layout_no_wrap(n, FontId::proportional(13.0), TEXT);
                let x = r.right() - 10.0 - g.size().x;
                p.galley(pos2(x, r.top() + 6.0), g, TEXT);
                p.circle_filled(pos2(x - 8.0, r.top() + 14.0), 4.0, RED);
            }
            None => {
                p.text(pos2(r.right() - 10.0, r.top() + 13.0), Align2::RIGHT_CENTER, "Desconectado", FontId::proportional(12.5), DIM);
            }
        }
    } else if live {
        p.circle_filled(a.right_top() + vec2(-2.0, 2.0), 4.0, RED);
    }
    let tip = match l.viewers {
        Some(v) => format!("{} · {} · {}", l.name, l.game, fmt_num(v)),
        None => format!("{} · Desconectado", l.name),
    };
    let resp = if open { resp } else { resp.on_hover_text(tip) };
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

enum MsgAct {
    Name(String, Pos2),
    Reply(String, String, String),
}

/// Pinta un mensaje. Clic en el nombre abre su tarjeta; al pasar el mouse aparece "Responder".
fn draw_msg(ui: &mut Ui, m: &Msg, store: &img::Store, cfg: &crate::Cfg, logged: bool) -> Option<MsgAct> {
    let (ts, font) = (cfg.timestamps, cfg.chat_font);
    let mut act = None;
    let accent = m.accent != Color32::TRANSPARENT;
    let mut frame = egui::Frame::new().inner_margin(Margin { left: if accent { 10 } else { 6 }, right: 6, top: 3, bottom: 3 }).corner_radius(4);
    if m.mention {
        frame = frame.fill(Color32::from_rgba_unmultiplied(145, 71, 255, 50));
    } else if accent {
        frame = frame.fill(Color32::from_white_alpha(8));
    }
    let body = FontId::proportional(font);
    // "Mensaje eliminado" breve (como twitch.tv): se cambia el contenido por el aviso
    let brief = m.deleted && cfg.deleted_mode == 1;
    let r = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        if m.deleted && !brief {
            ui.set_opacity(0.35);
        }
        if !m.head.is_empty() {
            ui.label(RichText::new(&m.head).font(semi(font - 2.0)).color(if accent { m.accent.gamma_multiply(1.4) } else { PURPLE_L }));
        }
        if cfg.replies && !m.reply.is_empty() {
            ui.add(egui::Label::new(RichText::new(&m.reply).color(DIM).small()).truncate());
        }
        if brief {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.label(RichText::new(format!("{}: ", m.name)).font(semi(font)).color(m.color));
                ui.label(RichText::new("<mensaje eliminado>").font(body.clone()).color(DIM).italics());
            });
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let color = if m.system { DIM } else if m.action { m.color } else { TEXT };
            if ts && !m.time.is_empty() {
                ui.label(RichText::new(format!("{} ", m.time)).font(FontId::proportional(font - 1.5)).color(DIM));
            }
            for &b in &m.badges {
                image(ui, store, b, BADGE_H);
                ui.add_space(3.0);
            }
            if !m.name.is_empty() {
                let r = ui.add(egui::Label::new(RichText::new(&m.name).font(semi(font)).color(m.color)).sense(Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand);
                if r.clicked() {
                    act = Some(MsgAct::Name(m.login.clone(), r.rect.left_bottom()));
                }
                ui.label(RichText::new(if m.action { " " } else { ": " }).font(body.clone()).color(TEXT));
            }
            for p in &m.parts {
                match p {
                    Part::Text(t) => {
                        let mut rt = RichText::new(t).font(body.clone()).color(color);
                        if m.system || m.action {
                            rt = rt.italics();
                        }
                        if m.deleted {
                            rt = rt.strikethrough();
                        }
                        ui.label(rt);
                    }
                    Part::Emote(layers) => {
                        let tip = layers.iter().map(|l| l.1.as_str()).collect::<Vec<_>>().join(" + ");
                        let (r, failed) = image(ui, store, layers[0].0, EMOTE_H);
                        if failed {
                            ui.label(RichText::new(&layers[0].1).font(body.clone()).color(color));
                        }
                        for (id, _) in &layers[1..] {
                            if let img::Look::Ready(f) = store.get(*id) {
                                ui.painter().image(f.tex, fit(r.rect, &f), f.uv, Color32::WHITE);
                            }
                        }
                        r.on_hover_text(tip);
                    }
                    Part::Emoji(id, text) => {
                        if image(ui, store, *id, EMOJI_H.max(font + 4.0)).1 {
                            ui.label(RichText::new(text).font(body.clone()).color(color)); // sin Twemoji: texto plano
                        }
                    }
                    Part::Link(u) => {
                        ui.hyperlink(u);
                    }
                }
            }
        });
    });
    if accent {
        let rr = r.response.rect;
        ui.painter().rect_filled(Rect::from_min_size(rr.min, vec2(3.0, rr.height())), 1.5, m.accent);
    }
    // Botón "Responder" al pasar el mouse (solo con sesión y mensajes reales)
    if logged && !m.system && !m.id.is_empty() && ui.rect_contains_pointer(r.response.rect) {
        let b = Rect::from_min_size(pos2(r.response.rect.right() - 30.0, r.response.rect.top() + 2.0), vec2(26.0, 24.0));
        ui.painter().rect_filled(b, 4.0, Color32::from_rgb(47, 47, 53));
        paint_icon(ui.painter(), b.shrink2(vec2(7.0, 6.0)), Icon::Reply, TEXT);
        if ui.interact(b, egui::Id::unique(("reply", &m.id)), Sense::click()).on_hover_text("Responder").clicked() {
            act = Some(MsgAct::Reply(m.id.clone(), m.name.clone(), m.body.clone()));
        }
    }
    act
}
