// Motor de imágenes propio (emotes, badges, avatares, emoji). Reemplaza a egui_extras porque:
// - Decodifica directo al tamaño en pantalla (un emote 2x de 56 px se guarda a 42 px si es lo que se ve).
// - Quita frames repetidos de los animados y los empaca todos en UNA textura (atlas en rejilla):
//   un objeto de GPU por emote en vez de uno por frame.
// - No guarda los pixeles en RAM después de subirlos a la GPU.
// - Presupuesto fijo de memoria con desalojo SIEVE (Zhang et al., NSDI '24): más simple que LRU
//   y con mejor tasa de aciertos en cargas tipo web. Lo desalojado se recarga del caché en disco,
//   no de la red.
// - Un reloj global para todas las animaciones: se pide UN repintado para el próximo cambio de
//   frame visible, con tope de FPS.
use eframe::egui::{ColorImage, Context, Rect, TextureHandle, TextureId, TextureOptions, Vec2, pos2, vec2};
use image::{AnimationDecoder, RgbaImage, imageops};
use std::collections::HashMap;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const WORKERS: usize = 4;
const MAX_FRAMES: usize = 200;

pub struct Frame {
    pub tex: TextureId,
    pub uv: Rect,
    pub size: Vec2, // pixeles del frame
}

pub enum Look {
    Ready(Frame),
    Pending,
    Failed,
}

struct Anim {
    tex: TextureHandle,
    w: u32,
    h: u32,
    cols: u32,
    rows: u32,
    ends: Vec<u32>, // fin acumulado de cada frame en ms
    bytes: usize,
}

enum State {
    Idle,
    Loading,
    Ready(Anim),
    Failed,
}

struct Slot {
    url: String,
    h: u32,
    state: State,
    visited: bool,
}

/// SIEVE: cola FIFO + bit de "visitado" + una mano que recorre de lo más viejo a lo más nuevo.
/// Un acierto solo prende el bit (sin mover nodos, a diferencia de LRU). Al desalojar, la mano
/// apaga bits hasta encontrar uno apagado y lo saca.
#[derive(Default)]
pub struct Sieve {
    q: Vec<u32>, // índice 0 = más viejo
    hand: usize,
}

impl Sieve {
    pub fn insert(&mut self, id: u32) {
        self.q.push(id);
    }
    /// `visited(id)` debe devolver el bit y apagarlo.
    pub fn evict(&mut self, mut visited: impl FnMut(u32) -> bool) -> Option<u32> {
        // Termina: tras una vuelta completa todos los bits quedan apagados.
        while !self.q.is_empty() {
            if self.hand >= self.q.len() {
                self.hand = 0;
            }
            let id = self.q[self.hand];
            if visited(id) {
                self.hand += 1;
            } else {
                self.q.remove(self.hand);
                return Some(id);
            }
        }
        None
    }
}

struct Inner {
    slots: Vec<Slot>,
    keys: HashMap<(String, u32), u32>,
    sieve: Sieve,
    used: usize,
    budget: usize,
    wake_ms: Option<u32>,
}

pub struct Store {
    inner: Arc<Mutex<Inner>>,
    jobs: Sender<(u32, String, u32)>,
    t0: Instant,
    pub animate: bool,
}

fn cache_dir() -> PathBuf {
    let base = std::env::var_os(if cfg!(windows) { "LOCALAPPDATA" } else { "HOME" }).unwrap_or_default();
    let d = PathBuf::from(base).join(if cfg!(windows) { "TwitchLite\\cache" } else { "Library/Caches/TwitchLite" });
    let size: u64 = std::fs::read_dir(&d).into_iter().flatten().flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum();
    if size > 200 << 20 {
        let _ = std::fs::remove_dir_all(&d);
    }
    let _ = std::fs::create_dir_all(&d);
    d
}

fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
}

impl Store {
    pub fn new(ctx: Context, budget: usize) -> Self {
        let inner = Arc::new(Mutex::new(Inner { slots: Vec::new(), keys: HashMap::new(), sieve: Sieve::default(), used: 0, budget, wake_ms: None }));
        let (tx, rx) = channel::<(u32, String, u32)>();
        let rx = Arc::new(Mutex::new(rx));
        let dir = Arc::new(cache_dir());
        for _ in 0..WORKERS {
            let (rx, inner, ctx, dir) = (rx.clone(), inner.clone(), ctx.clone(), dir.clone());
            std::thread::spawn(move || worker(&rx, &inner, &ctx, &dir));
        }
        Store { inner, jobs: tx, t0: Instant::now(), animate: true }
    }

    /// Id estable para (url, alto en px). Barato: un lookup en HashMap.
    pub fn id(&self, url: &str, h: u32) -> u32 {
        let mut g = self.inner.lock().unwrap();
        if let Some(&id) = g.keys.get(&(url.to_string(), h)) {
            return id;
        }
        let id = g.slots.len() as u32;
        g.slots.push(Slot { url: url.to_string(), h, state: State::Idle, visited: false });
        g.keys.insert((url.to_string(), h), id);
        id
    }

    pub fn get(&self, id: u32) -> Look {
        let now = self.t0.elapsed().as_millis() as u64;
        let mut g = self.inner.lock().unwrap();
        let animate = self.animate;
        let Inner { slots, wake_ms, .. } = &mut *g;
        let Some(slot) = slots.get_mut(id as usize) else { return Look::Failed };
        match &slot.state {
            State::Idle => {
                slot.state = State::Loading;
                let _ = self.jobs.send((id, slot.url.clone(), slot.h));
                Look::Pending
            }
            State::Loading => Look::Pending,
            State::Failed => Look::Failed,
            State::Ready(a) => {
                slot.visited = true;
                let total = *a.ends.last().unwrap_or(&0);
                let idx = if a.ends.len() > 1 && animate && total > 0 {
                    let t = (now % total as u64) as u32;
                    let i = a.ends.partition_point(|&e| e <= t);
                    let left = a.ends[i] - t;
                    *wake_ms = Some(wake_ms.map_or(left, |w| w.min(left)));
                    i as u32
                } else {
                    0
                };
                let (cw, ch) = (a.w + 2, a.h + 2); // 1 px de margen transparente por lado: sin sangrado entre celdas
                let (tw, th) = ((a.cols * cw) as f32, (a.rows * ch) as f32);
                let (x, y) = ((idx % a.cols) * cw + 1, (idx / a.cols) * ch + 1);
                let uv = Rect::from_min_size(pos2(x as f32 / tw, y as f32 / th), vec2(a.w as f32 / tw, a.h as f32 / th));
                Look::Ready(Frame { tex: a.tex.id(), uv, size: vec2(a.w as f32, a.h as f32) })
            }
        }
    }

    /// Tiempo hasta el próximo cambio de frame entre lo que se pintó en esta pasada.
    pub fn take_wake(&self) -> Option<Duration> {
        self.inner.lock().unwrap().wake_ms.take().map(|ms| Duration::from_millis(ms as u64))
    }

    /// (MB usados, presupuesto MB, imágenes residentes) para el panel de diagnóstico.
    pub fn stats(&self) -> (usize, usize, usize) {
        let g = self.inner.lock().unwrap();
        (g.used >> 20, g.budget >> 20, g.sieve.q.len())
    }
}

fn worker(rx: &Mutex<Receiver<(u32, String, u32)>>, inner: &Mutex<Inner>, ctx: &Context, dir: &std::path::Path) {
    loop {
        let job = rx.lock().unwrap().recv();
        let Ok((id, url, h)) = job else { return };
        let path = dir.join(format!("{:016x}", fnv(&url)));
        let bytes = std::fs::read(&path).ok().or_else(|| {
            let b = crate::twitch::get_bytes(&url)?;
            let _ = std::fs::write(&path, &b);
            Some(b)
        });
        let decoded = bytes.and_then(|b| decode(&b, h));
        // La textura se crea ANTES de tomar nuestro lock: así nunca esperamos al de egui con el nuestro tomado.
        let ready = decoded.map(|(img, w, fh, cols, rows, ends)| {
            let bytes = img.pixels.len() * 4;
            let tex = ctx.load_texture(&url, img, TextureOptions::LINEAR);
            Anim { tex, w, h: fh, cols, rows, ends, bytes }
        });
        let mut g = inner.lock().unwrap();
        let Inner { slots, sieve, used, budget, .. } = &mut *g;
        match ready {
            Some(a) => {
                *used += a.bytes;
                slots[id as usize].state = State::Ready(a);
                slots[id as usize].visited = true;
                sieve.insert(id);
                while *used > *budget {
                    let Some(v) = sieve.evict(|i| std::mem::replace(&mut slots[i as usize].visited, false)) else { break };
                    // Idle: si vuelve a hacer falta se recarga del disco
                    if let State::Ready(a) = std::mem::replace(&mut slots[v as usize].state, State::Idle) {
                        *used -= a.bytes;
                    }
                }
            }
            None => slots[id as usize].state = State::Failed,
        }
        drop(g);
        ctx.request_repaint();
    }
}

type Decoded = (ColorImage, u32, u32, u32, u32, Vec<u32>);

/// Decodifica GIF/WebP animado o imagen fija, escala a `h` px de alto, quita frames repetidos y
/// empaca todo en una rejilla. Devuelve (atlas, ancho, alto, columnas, filas, fines en ms).
pub fn decode(b: &[u8], h: u32) -> Option<Decoded> {
    let ms = |d: image::Delay| {
        let (n, den) = d.numer_denom_ms();
        let v = n / den.max(1);
        if v < 20 { 100 } else { v } // igual que los navegadores con delays de 0-10 ms
    };
    let frames: Vec<(RgbaImage, u32)> = if b.starts_with(b"GIF8") {
        let d = image::codecs::gif::GifDecoder::new(Cursor::new(b)).ok()?;
        d.into_frames().take(MAX_FRAMES).filter_map(|f| f.ok()).map(|f| (f.delay(), f.into_buffer())).map(|(d, f)| (f, ms(d))).collect()
    } else if b.len() > 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        let d = image::codecs::webp::WebPDecoder::new(Cursor::new(b)).ok()?;
        if d.has_animation() {
            d.into_frames().take(MAX_FRAMES).filter_map(|f| f.ok()).map(|f| (f.delay(), f.into_buffer())).map(|(d, f)| (f, ms(d))).collect()
        } else {
            vec![(image::load_from_memory(b).ok()?.to_rgba8(), 0)]
        }
    } else {
        vec![(image::load_from_memory(b).ok()?.to_rgba8(), 0)]
    };
    let (sw, sh) = frames.first()?.0.dimensions();
    let (w, fh) = if sh > h { (((sw as u64 * h as u64) / sh as u64).max(1) as u32, h) } else { (sw, sh) };
    // Frames idénticos seguidos se funden en uno solo con la suma de sus duraciones.
    let mut out: Vec<(RgbaImage, u32)> = Vec::with_capacity(frames.len());
    for (f, d) in frames {
        let f = if (w, fh) != (sw, sh) { imageops::resize(&f, w, fh, imageops::FilterType::Triangle) } else { f };
        match out.last_mut() {
            Some(last) if last.0.as_raw() == f.as_raw() => last.1 += d,
            _ => out.push((f, d)),
        }
    }
    let n = out.len() as u32;
    let cols = (n as f32).sqrt().ceil() as u32;
    let rows = n.div_ceil(cols);
    let (cw, ch) = (w + 2, fh + 2);
    let mut atlas = RgbaImage::new(cols * cw, rows * ch);
    let mut ends = Vec::with_capacity(out.len());
    let mut acc = 0;
    for (i, (f, d)) in out.iter().enumerate() {
        let i = i as u32;
        imageops::replace(&mut atlas, f, ((i % cols) * cw + 1) as i64, ((i / cols) * ch + 1) as i64);
        acc += d;
        ends.push(acc);
    }
    let img = ColorImage::from_rgba_unmultiplied([atlas.width() as usize, atlas.height() as usize], atlas.as_raw());
    Some((img, w, fh, cols, rows, ends))
}

// ---------- Emoji (Twemoji) ----------

fn is_emoji(c: char) -> bool {
    matches!(c as u32, 0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2B00..=0x2BFF | 0x2300..=0x23FF | 0x2190..=0x21FF | 0x3030 | 0x303D | 0x3297 | 0x3299 | 0xA9 | 0xAE | 0x203C | 0x2049 | 0x2122 | 0x2139)
}

/// Parte un texto en tramos de texto normal y secuencias emoji (ZWJ, tonos de piel, banderas, keycaps).
pub fn split_emoji(s: &str) -> Vec<(bool, String)> {
    let ch: Vec<char> = s.chars().collect();
    let mut out: Vec<(bool, String)> = Vec::new();
    let mut i = 0;
    while i < ch.len() {
        let c = ch[i];
        let keycap = (c.is_ascii_digit() || c == '#' || c == '*') && matches!(ch.get(i + 1..i + 3), Some(['\u{FE0F}', '\u{20E3}']) | Some(['\u{20E3}', _]));
        if is_emoji(c) || keycap {
            let mut seq = String::from(c);
            let flag = (0x1F1E6..=0x1F1FF).contains(&(c as u32));
            i += 1;
            if flag && ch.get(i).is_some_and(|n| (0x1F1E6..=0x1F1FF).contains(&(*n as u32))) {
                seq.push(ch[i]);
                i += 1;
            }
            while let Some(&n) = ch.get(i) {
                let n32 = n as u32;
                if n == '\u{FE0F}' || n == '\u{20E3}' || (0x1F3FB..=0x1F3FF).contains(&n32) || (0xE0020..=0xE007F).contains(&n32) {
                    seq.push(n);
                    i += 1;
                } else if n == '\u{200D}' && ch.get(i + 1).is_some_and(|x| is_emoji(*x)) {
                    seq.push(n);
                    seq.push(ch[i + 1]);
                    i += 2;
                } else {
                    break;
                }
            }
            out.push((true, seq));
        } else {
            if c != '\u{FE0F}' && c != '\u{FE0E}' {
                match out.last_mut() {
                    Some((false, t)) => t.push(c),
                    _ => out.push((false, c.to_string())),
                }
            }
            i += 1;
        }
    }
    out
}

/// Archivo de Twemoji: code points en hex unidos por '-'; sin FE0F salvo en secuencias ZWJ.
pub fn twemoji_url(seq: &str) -> String {
    let zwj = seq.contains('\u{200D}');
    let hex: Vec<String> = seq.chars().filter(|c| zwj || *c != '\u{FE0F}').map(|c| format!("{:x}", c as u32)).collect();
    format!("https://cdn.jsdelivr.net/gh/jdecked/twemoji@16.0.1/assets/72x72/{}.png", hex.join("-"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sieve_spares_visited() {
        let mut s = Sieve::default();
        for i in 0..3 {
            s.insert(i);
        }
        let mut bits = [true, false, true];
        // 0 visitado se salva (y pierde su bit), 1 no visitado se va
        assert_eq!(s.evict(|i| std::mem::replace(&mut bits[i as usize], false)), Some(1));
        // la mano sigue en 2 (visitado -> se salva), da la vuelta y saca a 0
        assert_eq!(s.evict(|i| std::mem::replace(&mut bits[i as usize], false)), Some(0));
        assert_eq!(s.evict(|_| false), Some(2));
        assert_eq!(s.evict(|_| false), None);
    }

    #[test]
    fn emoji_split_and_url() {
        let p = split_emoji("hola 🎀 x 🏳️‍🌈 👍🏽 🇲🇽");
        let e: Vec<&str> = p.iter().filter(|x| x.0).map(|x| x.1.as_str()).collect();
        assert_eq!(e, ["🎀", "🏳\u{FE0F}\u{200D}🌈", "👍🏽", "🇲🇽"]);
        assert!(twemoji_url("🏳\u{FE0F}\u{200D}🌈").ends_with("/1f3f3-fe0f-200d-1f308.png"));
        assert!(twemoji_url("❤\u{FE0F}").ends_with("/2764.png"));
        assert!(twemoji_url("🇲🇽").ends_with("/1f1f2-1f1fd.png"));
    }

    #[test]
    fn decode_dedupes_and_packs() {
        // GIF de 2 frames idénticos 4x4 -> 1 frame, escalado a 2 px de alto
        let mut buf = Vec::new();
        {
            let mut enc = image::codecs::gif::GifEncoder::new(&mut buf);
            let f = image::Frame::from_parts(RgbaImage::from_pixel(4, 4, image::Rgba([255, 0, 0, 255])), 0, 0, image::Delay::from_numer_denom_ms(50, 1));
            enc.encode_frames([f.clone(), f]).unwrap();
        }
        let (img, w, h, cols, rows, ends) = decode(&buf, 2).unwrap();
        assert_eq!((w, h, cols, rows), (2, 2, 1, 1));
        assert_eq!(ends, vec![100]);
        assert_eq!(img.size, [4, 4]);
    }
}
