use eframe::egui;
use std::sync::{Arc, OnceLock};

pub const NAME: &str = "TwitchLite";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const ICON_PNG: &[u8] = include_bytes!("../assets/twitchlite-256.png");
pub const NEON: egui::Color32 = egui::Color32::from_rgb(57, 255, 20);

/// Se decodifica una sola vez y se comparte entre las ventanas nativas.
pub fn window_icon() -> Arc<egui::IconData> {
    static ICON: OnceLock<Arc<egui::IconData>> = OnceLock::new();
    ICON.get_or_init(|| {
        let image = image::load_from_memory(ICON_PNG).expect("Icono integrado inválido").into_rgba8();
        Arc::new(egui::IconData { width: image.width(), height: image.height(), rgba: image.into_raw() })
    }).clone()
}

/// Una textura por contexto; no se vuelve a cargar ni se duplica en cada cuadro.
pub fn paint_logo(painter: &egui::Painter, rect: egui::Rect) {
    let ctx = painter.ctx();
    let id = egui::Id::unique("twitchlite.brand.icon");
    let texture = ctx.data(|data| data.get_temp::<egui::TextureHandle>(id)).unwrap_or_else(|| {
        let icon = window_icon();
        let image = egui::ColorImage::from_rgba_unmultiplied([icon.width as usize, icon.height as usize], &icon.rgba);
        let texture = ctx.load_texture("TwitchLite", image, egui::TextureOptions::LINEAR);
        ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
        texture
    });
    painter.image(texture.id(), rect, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
}
