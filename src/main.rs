#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod img;
mod mpv;
mod twitch;
mod ui;

use eframe::egui::{self, Color32};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};
use twitch::{Auth, ChanInfo, DropItem, Emote, Ev, Game, Identity, Live, Net, Notif, Reward, SubItem, UserCard};

pub fn tlog(msg: &str) {
    use std::io::Write;
    let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("twitchlite.log") else { return };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let _ = writeln!(f, "[{now}] {msg}");
}

const MAX_MSGS: usize = 150;
const MAX_CHATTERS: usize = 300;
const IMG_BUDGET: usize = 32 << 20;

// ---------- Config persistente: %APPDATA%\TwitchLite\config.json (sin secretos: esos van al llavero) ----------

fn cfg_path() -> std::path::PathBuf {
    let base = std::env::var_os(if cfg!(windows) { "APPDATA" } else { "HOME" }).unwrap_or_default();
    let dir = std::path::PathBuf::from(base).join(if cfg!(windows) { "TwitchLite" } else { "Library/Application Support/TwitchLite" });
    let _ = std::fs::create_dir_all(&dir);
    dir.join("config.json")
}

struct Cfg {
    channel: String,
    on_top: bool,
    video: bool,
    volume: f32,
    quality: String,
    client_id: String,
    sidebar: bool,
    animate: bool,
    pause_unfocused: bool,
    timestamps: bool,
    chat_font: f32,
    chat_overlay: bool,
    chat_left: bool,
    low_latency: bool,
    notify: bool,
    pause_mode: u8, // Pausar chat (como twitch.tv): 0 solo al desplazar, 1 con el cursor encima, 2 con Alt, 3 cursor o Alt
    hide_links: bool,
    deleted_mode: u8, // Mensajes eliminados: 0 quitar, 1 "<mensaje eliminado>", 2 tachado (detallado)
    blocked: String,  // palabras bloqueadas, separadas por coma
    readable: bool,   // colores de nombre legibles (aclara los oscuros)
    replies: bool,    // mostrar a quién responde cada mensaje
    filter: bool,     // filtros de chat de Twitch (tag `flags`)
    filter_cats: u8,  // bits: 1 discriminación, 2 sexual, 4 hostilidad/acoso, 8 lenguaje soez
    auto_claim: bool, // reclamar bonus de puntos de canal automáticamente
    chat_popout: bool,
}

impl Cfg {
    fn load() -> Self {
        let v: Value = std::fs::read_to_string(cfg_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let s = |k: &str| v[k].as_str().unwrap_or_default().to_string();
        let b = |k: &str, d: bool| v[k].as_bool().unwrap_or(d);
        // migración: versiones anteriores guardaban el token en este archivo en texto plano
        if !s("token").is_empty() {
            twitch::set_secret("token", &s("token"));
            twitch::set_secret("refresh", &s("refresh"));
        }
        let cfg = Cfg {
            channel: s("channel"),
            on_top: b("on_top", false),
            video: b("video", true),
            volume: v["volume"].as_f64().unwrap_or(70.0) as f32,
            quality: Some(s("quality")).filter(|q| !q.is_empty()).unwrap_or_else(|| "Auto".into()),
            client_id: s("client_id"),
            sidebar: b("sidebar", true),
            animate: b("animate", true),
            pause_unfocused: b("pause_unfocused", true),
            timestamps: b("timestamps", false),
            chat_font: v["chat_font"].as_f64().unwrap_or(13.5) as f32,
            chat_overlay: b("chat_overlay", false),
            chat_left: b("chat_left", false),
            low_latency: b("low_latency", true),
            notify: b("notify", true),
            pause_mode: v["pause_mode"].as_u64().map(|x| x as u8).unwrap_or(if b("pause_hover", false) { 1 } else { 0 }),
            hide_links: b("hide_links", false),
            deleted_mode: v["deleted_mode"].as_u64().map(|x| x as u8).unwrap_or(if b("hide_deleted", false) { 0 } else { 1 }),
            blocked: s("blocked"),
            readable: b("readable", true),
            replies: b("replies", true),
            filter: b("filter", false),
            filter_cats: v["filter_cats"].as_u64().unwrap_or(15) as u8,
            auto_claim: b("auto_claim", true),
            chat_popout: false, // la ventana aparte no se reabre sola al iniciar
        };
        cfg.save();
        cfg
    }
    fn save(&self) {
        let v = json!({"channel": self.channel, "on_top": self.on_top, "video": self.video, "volume": self.volume,
            "quality": self.quality, "client_id": self.client_id, "sidebar": self.sidebar, "animate": self.animate,
            "pause_unfocused": self.pause_unfocused, "timestamps": self.timestamps, "chat_font": self.chat_font, "chat_overlay": self.chat_overlay, "chat_left": self.chat_left, "low_latency": self.low_latency, "notify": self.notify,
            "pause_mode": self.pause_mode, "hide_links": self.hide_links, "deleted_mode": self.deleted_mode, "blocked": self.blocked,
            "readable": self.readable, "replies": self.replies, "filter": self.filter, "filter_cats": self.filter_cats, "auto_claim": self.auto_claim});
        let _ = std::fs::write(cfg_path(), serde_json::to_string_pretty(&v).unwrap_or_default());
    }
}

/// Segundos a sumar a UTC para la hora local (para la hora de cada mensaje).
#[cfg(windows)]
fn tz_offset() -> i64 {
    // TIME_ZONE_INFORMATION: Bias(i32) StandardName[32]u16 StandardDate(16B) StandardBias(i32) DaylightName[32]u16 DaylightDate(16B) DaylightBias(i32)
    #[repr(C)]
    struct Tzi {
        bias: i32,
        _a: [u16; 32],
        _b: [u8; 16],
        std_bias: i32,
        _c: [u16; 32],
        _d: [u8; 16],
        dst_bias: i32,
    }
    unsafe extern "system" {
        fn GetTimeZoneInformation(t: *mut Tzi) -> u32;
    }
    let mut t = Tzi { bias: 0, _a: [0; 32], _b: [0; 16], std_bias: 0, _c: [0; 32], _d: [0; 16], dst_bias: 0 };
    let r = unsafe { GetTimeZoneInformation(&mut t) };
    let extra = if r == 2 { t.dst_bias } else { t.std_bias };
    -((t.bias + extra) as i64) * 60
}
#[cfg(not(windows))]
fn tz_offset() -> i64 {
    0
}

// ---------- Mensajes ----------

enum Part {
    Text(String),
    Emote(Vec<(u32, String)>), // capas (id de imagen, nombre): la primera ocupa espacio, las zero-width van encima
    Emoji(u32, String),
    Link(String),
}

struct Msg {
    id: String,
    uid: String,
    login: String,
    name: String,
    color: Color32,
    badges: Vec<u32>,
    parts: Vec<Part>,
    body: String, // texto plano, para citar al responder
    reply: String,
    time: String,
    system: bool,
    action: bool,
    mention: bool,
    deleted: bool,
    head: String,    // rótulo arriba del mensaje: "Primer mensaje", "Mensaje destacado", "Anuncio"...
    accent: Color32, // franja a la izquierda (avisos de subs/raids/anuncios), transparente si no
    h: f32, // alto medido la última vez que se pintó; sirve para saltar los que no se ven
    born: Instant, // llegada: el chat flotante lo usa para la entrada que sube
}

/// Altos en pixeles físicos a los que se decodifica cada tipo de imagen.
#[derive(Clone, Copy)]
struct Px {
    emote: u32,
    emoji: u32,
    badge: u32,
}

#[derive(Clone, PartialEq)]
enum Page {
    Channel,
    Following,
    Explore,
    Category(String),
    Drops,
    Subs,
}

pub fn sort_str(i: u8) -> &'static str { if i == 1 { "RECENT" } else { "VIEWER_COUNT" } }

/// Texto -> partes. Emotes nativos por índice (tag `emotes`), los de 7TV/BTTV/FFZ por palabra,
/// emoji Unicode a Twemoji y links aparte.
fn tokenize(text: &str, tw: &str, emotes: &HashMap<String, Emote>, store: &img::Store, px: Px, hi: bool) -> Vec<Part> {
    let ch: Vec<char> = text.chars().collect();
    // tag `emotes` de Twitch: "25:0-4,12-16/1902:6-10", índices en code points
    let mut at: HashMap<usize, (String, usize)> = HashMap::new();
    for p in tw.split('/').filter(|p| !p.is_empty()) {
        let Some((id, ranges)) = p.split_once(':') else { continue };
        for r in ranges.split(',') {
            if let Some((a, b)) = r.split_once('-').and_then(|(a, b)| Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()?))) {
                if a <= b && b < ch.len() {
                    at.insert(a, (id.to_string(), b));
                }
            }
        }
    }
    let mut parts = Vec::new();
    let mut buf = String::new();
    let flush = |parts: &mut Vec<Part>, buf: &mut String| {
        if !buf.is_empty() {
            parts.push(Part::Text(std::mem::take(buf)));
        }
    };
    let emote = |parts: &mut Vec<Part>, buf: &mut String, id: u32, name: String, zw: bool| {
        if zw && buf.trim().is_empty() {
            if let Some(Part::Emote(layers)) = parts.last_mut() {
                buf.clear();
                return layers.push((id, name));
            }
        }
        flush(parts, buf);
        parts.push(Part::Emote(vec![(id, name)]));
    };
    let mut i = 0;
    while i < ch.len() {
        if let Some((id, b)) = at.get(&i) {
            emote(&mut parts, &mut buf, store.id(&twitch::twitch_emote(id, hi), px.emote), ch[i..=*b].iter().collect(), false);
            i = b + 1;
            continue;
        }
        if ch[i] == ' ' {
            buf.push(' ');
            i += 1;
            continue;
        }
        let mut j = i;
        while j < ch.len() && ch[j] != ' ' && !at.contains_key(&j) {
            j += 1;
        }
        let w: String = ch[i..j].iter().collect();
        i = j;
        if let Some(e) = emotes.get(&w) {
            emote(&mut parts, &mut buf, store.id(&e.url, px.emote), w, e.zw);
        } else if w.starts_with("http://") || w.starts_with("https://") {
            flush(&mut parts, &mut buf);
            parts.push(Part::Link(w));
        } else if w.is_ascii() {
            buf.push_str(&w);
        } else {
            for (is_emoji, s) in img::split_emoji(&w) {
                if is_emoji {
                    flush(&mut parts, &mut buf);
                    parts.push(Part::Emoji(store.id(&img::twemoji_url(&s), px.emoji), s));
                } else {
                    buf.push_str(&s);
                }
            }
        }
    }
    flush(&mut parts, &mut buf);
    parts
}

/// Alto de una variante HLS por su nombre ("1080p60 (source)" -> 1080). `None` para audio_only.
fn variant_h(name: &str) -> Option<u32> {
    name.split('p').next()?.parse().ok()
}

/// Calidad "Auto": la variante más chica que cubre el alto del player en pixeles físicos.
/// Decodificar 1080p para mostrarlo en 690 px gasta ~2x RAM/GPU y ancho de banda sin verse mejor.
fn auto_pick(q: &[(String, String)], want_h: u32) -> Option<&(String, String)> {
    let mut v: Vec<(u32, &(String, String))> = q.iter().filter_map(|x| Some((variant_h(&x.0)?, x))).collect();
    v.sort_by_key(|x| x.0);
    v.iter().find(|x| x.0 >= want_h).or(v.last()).map(|x| x.1)
}

/// Color del nombre; con "colores legibles" aclara los muy oscuros para que se lean sobre fondo negro.
fn name_color(hex: &str, name: &str, readable: bool) -> Color32 {
    let c = u32::from_str_radix(hex.trim_start_matches('#'), 16).ok().filter(|_| hex.len() == 7).map(|v| [(v >> 16) as u8, (v >> 8) as u8, v as u8]).unwrap_or_else(|| {
        const P: [[u8; 3]; 6] = [[255, 105, 97], [119, 221, 119], [100, 181, 246], [255, 179, 71], [203, 153, 255], [77, 208, 225]];
        P[name.bytes().map(usize::from).sum::<usize>() % P.len()]
    });
    let lum = (0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32) / 255.0;
    let k = if readable && lum < 0.45 { 0.45 } else { 0.0 };
    let f = |x: u8| (x as f32 + (255.0 - x as f32) * k) as u8;
    Color32::from_rgb(f(c[0]), f(c[1]), f(c[2]))
}

/// Filtros de chat de twitch.tv: el tag `flags` ("0-4:A.6/P.6,25-28:S.3") marca rangos (en code points)
/// por categoría (I discriminación, S sexual, A hostilidad, P lenguaje soez). Se tapan con '*' del
/// mismo largo para no correr los índices de los emotes.
fn mask_flags(text: &str, flags: &str, cats: u8) -> String {
    let mut ch: Vec<char> = text.chars().collect();
    for f in flags.split(',') {
        let Some((range, kinds)) = f.split_once(':') else { continue };
        let hit = kinds.split('/').any(|k| match k.chars().next() {
            Some('I') => cats & 1 != 0,
            Some('S') => cats & 2 != 0,
            Some('A') => cats & 4 != 0,
            Some('P') => cats & 8 != 0,
            _ => false,
        });
        let Some((a, b)) = range.split_once('-').and_then(|(a, b)| Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()?))) else { continue };
        if hit && a <= b && b < ch.len() {
            ch[a..=b].iter_mut().filter(|c| !c.is_whitespace()).for_each(|c| *c = '*');
        }
    }
    ch.into_iter().collect()
}

/// Estilo nativo de Windows 11 para la ventana sin marco (egui ya le pide a winit la sombra y el
/// marco invisible): modo oscuro, esquinas redondeadas y borde/barra del color del fondo para que
/// no se vea la línea clara de arriba. No usar SetWindowRgn: apaga el dibujo moderno y aparece el
/// marco clásico al redimensionar.
#[cfg(windows)]
fn dwm_style(hwnd: isize) {
    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmSetWindowAttribute(h: isize, attr: u32, v: *const u32, size: u32) -> i32;
    }
    if hwnd == 0 {
        return;
    }
    const DARK: u32 = 0x0010_0E0E; // COLORREF 0x00BBGGRR = (14,14,16), el fondo de la app
    // 20 modo oscuro, 33 esquinas (2 = redondeadas), 34 color del borde, 35 color de la barra
    for (attr, v) in [(20u32, 1u32), (33, 2), (34, DARK), (35, DARK)] {
        unsafe { DwmSetWindowAttribute(hwnd, attr, &v, 4) };
    }
}
#[cfg(not(windows))]
fn dwm_style(_: isize) {}

fn hwnd_of(cc: &eframe::CreationContext) -> isize {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match cc.window_handle().map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(w)) => w.hwnd.get(),
        _ => 0,
    }
}

/// Notificación nativa (toast de Windows / Centro de notificaciones de macOS). En un hilo para no
/// trabar la UI mientras el sistema la muestra.
fn toast(title: String, body: String) {
    std::thread::spawn(move || {
        let _ = notify_rust::Notification::new().appname("Twitch Lite").summary(&title).body(&body).show();
    });
}

/// "HH:MM" local a partir de milisegundos unix.
fn hhmm(ms: i64, tz: i64) -> String {
    let s = (ms / 1000 + tz).rem_euclid(86_400);
    format!("{}:{:02}", s / 3600, s % 3600 / 60)
}

// ---------- App ----------

struct App {
    net: Net,
    rx: Receiver<Ev>,
    cfg: Cfg,
    store: img::Store,
    px: Px,
    hi: bool,
    tz: i64,
    page: Page,
    generation: u64,
    chan: String,
    search: String,
    search_res: Vec<Live>,
    search_t: Option<Instant>,
    irc: Option<Sender<String>>,
    room: String,
    modes: HashMap<String, String>,
    emotes: HashMap<String, Emote>,
    chan_emotes: Vec<(String, String)>,
    badges: HashMap<String, String>,
    msgs: VecDeque<Msg>,
    chatters: VecDeque<String>,
    input: String,
    tab: Option<(String, usize, usize)>, // (prefijo, índice, inicio de palabra) del autocompletado con Tab
    reply: Option<(String, String, String)>, // (id del mensaje, nombre, texto) al que respondes
    me: Option<Auth>,
    my_avatar: String,
    my_color: Color32,
    my_badges: String,
    points: Option<u64>,
    device: Option<(String, String)>,
    login_open: bool,
    login_paste: String,
    login_err: String,
    menu_open: bool,
    quality_open: bool,
    chat_menu: bool,
    card: Option<UserCard>,
    card_pos: egui::Pos2,
    follows: Vec<Live>,
    follows_all: bool,
    top: Vec<Live>,
    games: Vec<Game>,
    cat_streams: (String, Vec<Live>),
    related: (String, Vec<Live>), // "Más en {juego}" del sidebar: directos de la categoría que estás viendo
    follow_tab: u8,               // Siguiendo: 0 en directo, 1 canales
    explore_tab: u8,              // Explorar: 0 categorías, 1 canales en directo
    explore_sort: u8,             // 0 = espectadores, 1 = recientes
    cat_sort: u8,
    cat_tag: String,              // tag seleccionado para filtrar en categoría
    my_emotes: Vec<(String, String)>,
    picker: bool,
    picker_tab: u8,
    picker_q: String,
    chat_open: bool,
    chat_stuck: bool,
    focused: bool,
    stats_open: bool,
    buffer: f64,            // segundos de video por delante (distancia al directo)
    catching_up: bool,
    rewound: bool, // regresaste con la barra de progreso
    shot: Option<(String, Instant, bool)>, // pruebas: TWITCHLITE_SHOT=ruta.png guarda un cuadro de la app a los 25 s
    lat_t: Instant,
    copied_t: Option<Instant>,
    users_open: bool,
    hwnd: isize,
    identity: Option<Identity>,
    drops: Vec<DropItem>,
    subs: Vec<SubItem>,
    chat_page: u8, // ajustes del chat: 0 principal, 1 identidad, 2 filtros, 3 pausar, 4 apariencia
    hex_input: String,
    rewards: (String, Vec<Reward>), // recompensas de puntos del canal (solo para ver; canjear es en twitch.tv)
    rewards_open: bool,
    notifs: Vec<Notif>,
    menu_page: u8, // menú de usuario: 0 principal, 1 identidad, 2 notificaciones, 3 cartera, 4 ajustes, 5 atajos
    bits: Option<u64>,
    hero: usize,     // índice del canal destacado en el carrusel de Siguiendo
    hero_t: Instant, // cuándo cambió (para la transición)
    hero_dir: f32,   // 1 avanza, -1 retrocede
    follow_more: bool, // "Mostrar más" en la rejilla de Siguiendo
    chat_content_h: f32,
    about_open: bool,
    jump_bottom: bool,
    theater: bool,
    get_proc: Option<mpv::GetProc>,
    player: Option<mpv::Player>,
    paused: bool,
    muted: bool,
    vid_off: bool,
    controls_t: Instant,
    status: String,
    info: ChanInfo,
    info_t: Instant,
    side_t: Instant,
    qualities: Vec<(String, String)>,
    playing: String, // variante que está sonando (la elige Auto o el usuario)
    want_h: u32,     // alto del player en px físicos, lo actualiza la UI
    auto_t: Option<Instant>,
    ad_detected: bool,
    ad_player: Option<mpv::Player>, // segundo player para el anuncio en PiP
    ad_muted: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext, cfg: Cfg) -> Self {
        let (tx, rx) = channel();
        let net = Net { tx, ctx: cc.egui_ctx.clone() };
        twitch::resume(&net, cfg.client_id.clone());
        // Para pruebas: TWITCHLITE_IMPORT=ruta importa la línea de cookies de ese archivo al llavero.
        if let Some(raw) = std::env::var_os("TWITCHLITE_IMPORT").and_then(|p| std::fs::read_to_string(p).ok()) {
            twitch::import_web(&net, raw);
        }
        twitch::load_top(&net, "VIEWER_COUNT");
        let ppp = cc.egui_ctx.pixels_per_point();
        let px = |pt: f32| (pt * ppp).round() as u32;
        let mut store = img::Store::new(cc.egui_ctx.clone(), IMG_BUDGET);
        store.animate = cfg.animate;
        let mut app = App {
            net,
            rx,
            store,
            px: Px { emote: px(ui::EMOTE_H), emoji: px(ui::EMOJI_H), badge: px(ui::BADGE_H) },
            // fuentes 2x solo si la pantalla escala >125%; igual se decodifican al tamaño exacto en pantalla
            hi: ppp > 1.25,
            tz: tz_offset(),
            page: Page::Following,
            generation: 0,
            chan: String::new(),
            search: String::new(),
            search_res: Vec::new(),
            search_t: None,
            irc: None,
            room: String::new(),
            modes: HashMap::new(),
            emotes: HashMap::new(),
            chan_emotes: Vec::new(),
            badges: HashMap::new(),
            msgs: VecDeque::new(),
            chatters: VecDeque::new(),
            input: String::new(),
            tab: None,
            reply: None,
            me: None,
            my_avatar: String::new(),
            my_color: ui::PURPLE_L,
            my_badges: String::new(),
            points: None,
            device: None,
            login_open: false,
            login_paste: String::new(),
            login_err: String::new(),
            menu_open: false,
            quality_open: false,
            chat_menu: false,
            card: None,
            card_pos: egui::Pos2::ZERO,
            follows: Vec::new(),
            follows_all: false,
            top: Vec::new(),
            games: Vec::new(),
            cat_streams: (String::new(), Vec::new()),
            related: (String::new(), Vec::new()),
            follow_tab: 0,
            explore_tab: 0,
            explore_sort: 0,
            cat_sort: 0,
            cat_tag: String::new(),
            my_emotes: Vec::new(),
            picker: false,
            picker_tab: 0,
            picker_q: String::new(),
            chat_open: true,
            chat_stuck: true,
            focused: true,
            stats_open: std::env::var_os("TWITCHLITE_STATS").is_some(), // para pruebas
            buffer: 0.0,
            catching_up: false,
            rewound: false,
            shot: std::env::var("TWITCHLITE_SHOT").ok().map(|p| (p, Instant::now(), false)),
            lat_t: Instant::now(),
            copied_t: None,
            users_open: false,
            hwnd: hwnd_of(cc),
            identity: None,
            drops: Vec::new(),
            subs: Vec::new(),
            chat_page: 0,
            hex_input: String::new(),
            rewards: (String::new(), Vec::new()),
            rewards_open: false,
            notifs: Vec::new(),
            menu_page: 0,
            bits: None,
            hero: 0,
            hero_t: Instant::now(),
            hero_dir: 1.0,
            follow_more: false,
            chat_content_h: 0.0,
            about_open: false,
            jump_bottom: false,
            theater: false,
            get_proc: cc.get_proc_address.clone(),
            player: None,
            paused: false,
            muted: false,
            vid_off: false,
            controls_t: Instant::now(),
            status: String::new(),
            info: ChanInfo::default(),
            info_t: Instant::now(),
            side_t: Instant::now(),
            qualities: Vec::new(),
            playing: String::new(),
            want_h: 720,
            auto_t: None,
            ad_detected: false,
            ad_player: None,
            ad_muted: true,
            cfg,
        };
        dwm_style(app.hwnd);
        let c = app.cfg.channel.clone();
        app.join(&c);
        // Para pruebas: TWITCHLITE_PAGE=explore abre directo en Explorar.
        if std::env::var("TWITCHLITE_PAGE").as_deref() == Ok("explore") {
            app.go(Page::Explore);
        }
        app
    }

    fn join(&mut self, c: &str) {
        let c: String = c.trim().trim_start_matches('#').to_lowercase().chars().filter(|x| x.is_ascii_alphanumeric() || *x == '_').collect();
        if c.is_empty() {
            return;
        }
        tlog(&format!("JOIN: {c}"));
        self.page = Page::Channel;
        self.generation += 1;
        self.irc = Some(twitch::irc(&self.net, self.generation, c.clone(), self.me.clone()));
        if c != self.chan {
            self.msgs.clear();
            self.chatters.clear();
            self.room.clear();
            self.modes.clear();
            self.emotes.clear();
            self.chan_emotes.clear();
            self.info = ChanInfo::default();
            self.ad_detected = false;
            self.ad_player = None;
            self.ad_muted = true;
            self.qualities.clear();
            self.playing.clear();
            self.status.clear();
            self.points = None;
            self.reply = None;
            self.card = None;
            self.chat_stuck = true;
            if let Some(p) = &self.player {
                p.stop();
            }
            if self.cfg.video {
                twitch::load_stream(&self.net, c.clone(), self.session());
            }
            twitch::load_info(&self.net, c.clone());
            twitch::load_badges(&self.net, c.clone(), self.hi);
            self.info_t = Instant::now();
            self.sys(format!("¡Te damos la bienvenida a la sala de chat de {c}!"));
        }
        if let Some(a) = self.me.as_ref().filter(|a| a.web) {
            twitch::load_points(&self.net, a.token.clone(), c.clone(), self.cfg.auto_claim);
        }
        self.search.clear();
        self.search_res.clear();
        self.chan = c.clone();
        self.cfg.channel = c;
        self.cfg.save();
    }

    /// Ir a una página sin canal: corta el video para no gastar mientras navegas.
    fn go(&mut self, page: Page) {
        match &page {
            Page::Explore if self.games.is_empty() => twitch::load_games(&self.net),
            Page::Category(g) => {
                self.cat_streams = (g.clone(), Vec::new());
                self.cat_tag.clear();
                twitch::load_streams(&self.net, g.clone(), sort_str(self.cat_sort));
            }
            Page::Following => {
                if let Some(a) = &self.me {
                    twitch::load_follows(&self.net, a.clone());
                }
            }
            Page::Drops => {
                if let Some(t) = self.session() {
                    twitch::load_drops(&self.net, t);
                }
            }
            Page::Subs => {
                if let Some(t) = self.session() {
                    twitch::load_subs(&self.net, t);
                }
            }
            _ => {}
        }
        if page != Page::Channel {
            if let Some(p) = &self.player {
                p.stop();
            }
            self.playing.clear();
            self.chan.clear();
            self.irc = None; // suelta el hilo del chat
        }
        self.page = page;
    }

    /// Token de la sesión de twitch.tv, si entraste con ella (las de app propia no sirven en GQL).
    fn session(&self) -> Option<String> {
        self.me.as_ref().filter(|a| a.web).map(|a| a.token.clone())
    }

    fn push(&mut self, m: Msg) {
        self.msgs.push_back(m);
        // en pausa (estás leyendo arriba) se guardan más para que lo que lees no se mueva ni desaparezca
        let max = if self.chat_stuck { MAX_MSGS } else { MAX_MSGS * 3 };
        while self.msgs.len() > max {
            self.msgs.pop_front();
        }
    }

    fn sys(&mut self, text: String) {
        self.push(Msg {
            id: String::new(),
            uid: String::new(),
            login: String::new(),
            name: String::new(),
            color: ui::DIM,
            badges: vec![],
            parts: vec![Part::Text(text)],
            body: String::new(),
            reply: String::new(),
            time: String::new(),
            system: true,
            action: false,
            mention: false,
            deleted: false,
            head: String::new(),
            accent: Color32::TRANSPARENT,
            h: 24.0,
            born: Instant::now(),
        });
    }

    /// Avisos de USERNOTICE (subs, regalos, raids, anuncios) con la franja de color como en twitch.tv.
    fn notice(&mut self, tags: &HashMap<String, String>, nick: &str, text: &str) {
        let t = |k: &str| tags.get(k).cloned().unwrap_or_default();
        let kind = t("msg-id");
        let (head, accent) = match kind.as_str() {
            "announcement" => {
                let c = match t("msg-param-color").as_str() {
                    "BLUE" => Color32::from_rgb(0, 214, 214),
                    "GREEN" => Color32::from_rgb(0, 219, 132),
                    "ORANGE" => Color32::from_rgb(255, 179, 31),
                    _ => ui::PURPLE,
                };
                ("Anuncio".to_string(), c)
            }
            "raid" => (String::new(), Color32::from_rgb(255, 179, 31)),
            _ => (String::new(), ui::PURPLE),
        };
        if kind != "announcement" {
            let sys = t("system-msg");
            if !sys.is_empty() {
                self.sys(sys);
                if let Some(m) = self.msgs.back_mut() {
                    m.accent = accent;
                    m.color = ui::TEXT;
                }
            }
        }
        if !text.is_empty() {
            self.chat_msg(tags, nick, text);
            if let Some(m) = self.msgs.back_mut() {
                m.accent = accent;
                if !head.is_empty() {
                    m.head = head;
                }
            }
        }
    }

    fn chat_msg(&mut self, tags: &HashMap<String, String>, nick: &str, text: &str) {
        let t = |k: &str| tags.get(k).cloned().unwrap_or_default();
        let name = Some(t("display-name")).filter(|n| !n.is_empty()).unwrap_or_else(|| nick.to_string());
        let (text, action) = match text.strip_prefix("\x01ACTION ") {
            Some(x) => (x.trim_end_matches('\x01'), true),
            None => (text, false),
        };
        // Filtros de chat de Twitch: se tapa antes de recortar nada, los rangos son sobre el texto completo
        let masked;
        let text = if self.cfg.filter && tags.contains_key("flags") {
            masked = mask_flags(text, &t("flags"), self.cfg.filter_cats);
            masked.as_str()
        } else {
            text
        };
        let reply_to = t("reply-parent-display-name");
        // Twitch antepone "@usuario " a las respuestas; se quita y se muestra arriba.
        let prefix = format!("@{reply_to} ");
        let tw = t("emotes");
        let body = if reply_to.is_empty() || !tw.is_empty() { text } else { text.strip_prefix(&prefix).unwrap_or(text) };
        let reply = if reply_to.is_empty() { String::new() } else { format!("Respuesta a @{reply_to}: {}", t("reply-parent-msg-body")) };
        let badges = t("badges").split(',').filter_map(|b| self.badges.get(b)).map(|u| self.store.id(u, self.px.badge)).collect();
        // te mencionan con tu nombre/@ o te responden un mensaje
        let mention = self.me.as_ref().is_some_and(|m| text.to_lowercase().contains(&m.login) || t("reply-parent-user-login") == m.login);
        let own = self.me.as_ref().is_some_and(|m| m.login == nick.to_lowercase());
        if mention && !own && self.cfg.notify && !self.focused {
            let what = if reply_to.is_empty() { "te mencionó" } else { "te respondió" };
            toast(format!("{name} {what} en #{}", self.chan), twitch::clean(text));
        }
        // con emotes nativos no se limpia: quitar chars correría sus índices
        let body = if tw.is_empty() { twitch::clean(body) } else { body.to_string() };
        // Filtros de chat (locales): links y palabras bloqueadas. Tus propios mensajes no se filtran.
        if !own {
            let low = body.to_lowercase();
            let blocked = self.cfg.blocked.split(',').map(|w| w.trim().to_lowercase()).filter(|w| !w.is_empty()).any(|w| low.contains(&w));
            if blocked || (self.cfg.hide_links && (low.contains("http://") || low.contains("https://"))) {
                return;
            }
        }
        let parts = tokenize(&body, &tw, &self.emotes, &self.store, self.px, self.hi);
        let ms = t("tmi-sent-ts").parse::<i64>().unwrap_or_else(|_| twitch::now_unix() * 1000);
        self.chatters.retain(|c| c != &name);
        self.chatters.push_back(name.clone());
        if self.chatters.len() > MAX_CHATTERS {
            self.chatters.pop_front();
        }
        let login = Some(t("login")).filter(|l| !l.is_empty()).unwrap_or_else(|| nick.to_string());
        self.push(Msg {
            id: t("id"),
            uid: t("user-id"),
            login,
            color: name_color(&t("color"), &name, self.cfg.readable),
            name,
            badges,
            parts,
            body,
            reply,
            time: hhmm(ms, self.tz),
            system: false,
            action,
            mention,
            deleted: false,
            // rótulos como los de twitch.tv
            head: if t("first-msg") == "1" {
                "Primer mensaje en el chat".into()
            } else if t("msg-id") == "highlighted-message" {
                "Mensaje destacado con puntos".into()
            } else if !t("bits").is_empty() {
                format!("{} Bits", t("bits"))
            } else {
                String::new()
            },
            accent: if t("msg-id") == "highlighted-message" || !t("bits").is_empty() { ui::PURPLE } else { Color32::TRANSPARENT },
            h: 24.0,
            born: Instant::now(),
        });
    }

    /// ROOMSTATE llega completo al entrar y luego solo con lo que cambia.
    fn room_modes(&mut self, tags: &HashMap<String, String>) {
        for k in ["slow", "subs-only", "emote-only", "followers-only", "r9k"] {
            if let Some(v) = tags.get(k) {
                self.modes.insert(k.into(), v.clone());
            }
        }
    }

    /// Texto tipo Twitch para los modos activos ("Modo lento (30 s)").
    fn modes_line(&self) -> String {
        let on = |k: &str| self.modes.get(k).is_some_and(|v| v != "0" && v != "-1");
        let mut out = Vec::new();
        if on("slow") {
            out.push(format!("Modo lento ({} s)", self.modes["slow"]));
        }
        if on("followers-only") {
            let m = &self.modes["followers-only"];
            out.push(if m == "0" { "Solo seguidores".into() } else { format!("Solo seguidores ({m} min)") });
        } else if self.modes.get("followers-only").is_some_and(|v| v == "0") {
            out.push("Solo seguidores".into());
        }
        if on("subs-only") {
            out.push("Solo suscriptores".into());
        }
        if on("emote-only") {
            out.push("Solo emotes".into());
        }
        if on("r9k") {
            out.push("Mensajes únicos".into());
        }
        out.join(" · ")
    }

    fn handle(&mut self, ev: Ev) {
        match ev {
            Ev::Line(g, _) | Ev::Status(g, _) if g != 0 && g != self.generation => {}
            Ev::Status(0, s) if self.login_open => self.login_err = s,
            Ev::Status(_, s) => self.sys(s),
            Ev::Line(_, m) => match m.cmd.as_str() {
                "PRIVMSG" => self.chat_msg(&m.tags, &m.nick, &m.text),
                "USERNOTICE" => self.notice(&m.tags, &m.nick, &m.text),
                "NOTICE" => self.sys(m.text),
                "WHISPER" => {
                    let name = m.tags.get("display-name").cloned().filter(|n| !n.is_empty()).unwrap_or(m.nick);
                    if self.cfg.notify {
                        toast(format!("Susurro de {name}"), m.text.clone());
                    }
                    self.sys(format!("Susurro de {name}: {}", m.text));
                }
                "ROOMSTATE" => {
                    self.room_modes(&m.tags);
                    let id = m.tags.get("room-id").cloned().unwrap_or_default();
                    if !id.is_empty() && id != self.room {
                        self.room = id.clone();
                        twitch::load_emotes(&self.net, id, self.hi);
                    }
                }
                "CLEARCHAT" => match m.tags.get("target-user-id") {
                    Some(u) if self.cfg.deleted_mode == 0 => self.msgs.retain(|x| &x.uid != u),
                    Some(u) => self.msgs.iter_mut().filter(|x| &x.uid == u).for_each(|x| x.deleted = true),
                    None => self.sys("Un moderador limpió el chat".into()),
                },
                "CLEARMSG" => {
                    let id = m.tags.get("target-msg-id").cloned().unwrap_or_default();
                    if self.cfg.deleted_mode == 0 {
                        self.msgs.retain(|x| x.id != id);
                    } else {
                        self.msgs.iter_mut().filter(|x| x.id == id).for_each(|x| x.deleted = true);
                    }
                }
                "USERSTATE" | "GLOBALUSERSTATE" => {
                    if let Some(c) = m.tags.get("color").filter(|c| !c.is_empty()) {
                        self.my_color = name_color(c, "", self.cfg.readable);
                    }
                    self.my_badges = m.tags.get("badges").cloned().unwrap_or_default();
                }
                _ => {}
            },
            Ev::Emotes(room, e) => {
                if room == self.room {
                    self.chan_emotes = e.iter().map(|(k, v)| (k.clone(), v.url.clone())).collect();
                    self.chan_emotes.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
                    self.emotes = e;
                }
            }
            Ev::Badges(b) => self.badges = b,
            Ev::Auth(None) => {
                if self.me.take().is_some() {
                    self.sys("La sesión expiró, vuelve a iniciar sesión.".into());
                }
            }
            Ev::Auth(Some(a)) => {
                let first = self.me.as_ref().map(|m| &m.login) != Some(&a.login);
                self.me = Some(a.clone());
                self.device = None;
                self.login_open = false;
                self.login_paste.clear();
                self.login_err.clear();
                if first {
                    twitch::load_follows(&self.net, a.clone());
                    twitch::load_my_emotes(&self.net, a.clone(), self.hi);
                    twitch::load_avatar(&self.net, a.login.clone());
                    if a.web {
                        twitch::load_bits(&self.net, a.token.clone());
                        twitch::load_notifs(&self.net, a.token.clone());
                    }
                    if !self.chan.is_empty() {
                        let c = self.chan.clone();
                        self.join(&c); // reconecta el chat con tu cuenta
                        if self.cfg.video && a.web {
                            twitch::load_stream(&self.net, c, self.session()); // video a tu nombre (sin anuncios si eres sub/Turbo)
                        }
                    }
                }
            }
            Ev::Avatar(url) => self.my_avatar = url,
            Ev::Device(code, uri) => {
                self.net.ctx.open_url(egui::OpenUrl::new_tab(&uri));
                self.device = Some((code, uri));
            }
            Ev::Follows(f) => {
                // Aviso de "entró en vivo": solo los que antes estaban desconectados (la primera carga no avisa)
                if self.cfg.notify && !self.follows.is_empty() {
                    for l in f.iter().filter(|l| l.viewers.is_some()) {
                        let was_live = self.follows.iter().any(|o| o.login == l.login && o.viewers.is_some());
                        if !was_live && l.login != self.chan {
                            toast(format!("{} está en directo", l.name), format!("{} · {}", l.game, l.title));
                        }
                    }
                }
                self.follows = f;
            }
            Ev::Top(t) => self.top = t,
            Ev::Games(g) => self.games = g,
            Ev::Streams(Some(g), s) => {
                if self.related.0 == g {
                    self.related.1 = s.iter().filter(|l| l.login != self.chan).take(6).cloned().collect();
                }
                if self.cat_streams.0 == g {
                    self.cat_streams.1 = s;
                }
            }
            Ev::Streams(None, _) => {}
            Ev::Search(q, r) => {
                if q == self.search.trim() {
                    self.search_res = r;
                }
            }
            Ev::Card(c) => self.card = Some(c),
            Ev::Bits(b) => self.bits = Some(b),
            Ev::Identity(i) => {
                if !i.color.is_empty() {
                    self.my_color = name_color(&i.color, "", self.cfg.readable); // tus mensajes salen ya con el color nuevo
                }
                self.identity = Some(i);
            }
            Ev::Rewards(c, r) => {
                if c == self.chan {
                    self.rewards = (c, r);
                }
            }
            Ev::Notifs(n) => self.notifs = n,
            Ev::Drops(d) => self.drops = d,
            Ev::Subs(s) => self.subs = s,
            Ev::Points(c, b) => {
                if c == self.chan {
                    self.points = Some(b);
                }
            }
            Ev::MyEmotes(e) => self.my_emotes = e,
            Ev::Info(c, info) => {
                if c == self.chan {
                    // categoría nueva: pide sus directos para "Más en {juego}" del sidebar
                    if !info.game.is_empty() && info.game != self.related.0 {
                        self.related = (info.game.clone(), Vec::new());
                        twitch::load_streams(&self.net, info.game.clone(), "VIEWER_COUNT");
                    }
                    self.info = info;
                }
            }
            Ev::Stream(c, _) if c != self.chan || !self.cfg.video => {}
            Ev::Stream(_, Err(e)) => {
                tlog(&format!("Stream error: {e}"));
                self.status = e;
                self.qualities.clear();
            }
            Ev::Stream(_, Ok(q)) => {
                tlog(&format!("Stream OK: {} qualities", q.len()));
                self.status.clear();
                self.qualities = q;
                if self.player.is_none() {
                    match self.get_proc.clone().map(|gp| mpv::Player::new(gp, self.net.ctx.clone(), self.cfg.volume)) {
                        Some(Ok(p)) => self.player = Some(p),
                        Some(Err(e)) => self.status = e,
                        None => self.status = "Sin contexto OpenGL".into(),
                    }
                }
                let manual = self.qualities.iter().find(|q| q.0 == self.cfg.quality);
                let pick = manual.or_else(|| auto_pick(&self.qualities, self.want_h)).or(self.qualities.first()).cloned();
                if manual.is_none() {
                    self.cfg.quality = "Auto".into();
                }
                if let (Some(p), Some(q)) = (&self.player, pick) {
                    p.play(&q.1);
                    p.set("speed", "1");
                    self.catching_up = false;
                    if self.muted {
                        p.set("mute", "yes");
                    }
                    self.playing = q.0;
                    self.paused = false;
                }
                if let Some(url) = self.qualities.last().map(|q| q.1.clone()) {
                    twitch::start_ad_monitor(&self.net, url);
                }
            }
            Ev::AdStatus(is_ad) => {
                tlog(&format!("AdStatus: {is_ad} (was {})", self.ad_detected));
                if is_ad && !self.ad_detected {
                    tlog("Requesting clean stream");
                    twitch::load_stream_clean(&self.net, self.chan.clone());
                }
                self.ad_detected = is_ad;
            }
            Ev::CleanStream(c, _) if c != self.chan => {}
            Ev::CleanStream(_, Err(e)) => {
                tlog(&format!("CleanStream error: {e}"));
                self.status = format!("Ad skip: {e}");
            }
            Ev::CleanStream(_, Ok(q)) => {
                let pick = q.iter().find(|x| x.0 == self.playing).or(q.first());
                tlog(&format!("CleanStream OK: {} qualities, picking {:?}", q.len(), pick.map(|p| &p.0)));
                if let Some(p) = &self.player {
                    if let Some(clean) = pick {
                        p.play(&clean.1);
                    }
                }
            }
        }
    }

    fn send(&mut self) {
        let text = self.input.trim().to_string();
        let (Some(irc), Some(me)) = (&self.irc, &self.me) else { return };
        if text.is_empty() {
            return;
        }
        // /color: Twitch quitó el comando del IRC; se hace con la misma llamada que usa twitch.tv
        if let Some(c) = text.strip_prefix("/color ") {
            const NAMES: [(&str, &str); 15] = [
                ("red", "#FF0000"), ("blue", "#0000FF"), ("green", "#00FF00"), ("firebrick", "#B22222"), ("coral", "#FF7F50"),
                ("yellowgreen", "#9ACD32"), ("orangered", "#FF4500"), ("seagreen", "#2E8B57"), ("goldenrod", "#DAA520"), ("chocolate", "#D2691E"),
                ("cadetblue", "#5F9EA0"), ("dodgerblue", "#1E90FF"), ("hotpink", "#FF69B4"), ("blueviolet", "#8A2BE2"), ("springgreen", "#00FF7F"),
            ];
            let c = c.trim().to_lowercase();
            let hex = NAMES.iter().find(|n| n.0 == c).map(|n| n.1.to_string()).unwrap_or_else(|| c.to_uppercase());
            match self.session() {
                Some(t) => twitch::set_color(&self.net, t, self.chan.clone(), hex),
                None => self.sys("Para cambiar el color entra con tu sesión de twitch.tv.".into()),
            }
            self.input.clear();
            return;
        }
        let wire = match text.strip_prefix("/me ") {
            Some(x) => format!("\x01ACTION {x}\x01"),
            None => text.clone(),
        };
        let reply_tag = self.reply.as_ref().map(|r| format!("@reply-parent-msg-id={} ", r.0)).unwrap_or_default();
        let _ = irc.send(format!("{reply_tag}PRIVMSG #{} :{wire}", self.chan));
        if !text.starts_with('/') || text.starts_with("/me ") {
            // Twitch no te devuelve tu propio mensaje: se pinta local
            let mut tags = HashMap::from([("display-name".to_string(), me.login.clone()), ("badges".to_string(), self.my_badges.clone())]);
            if let Some((_, name, body)) = &self.reply {
                tags.insert("reply-parent-display-name".into(), name.clone());
                tags.insert("reply-parent-msg-body".into(), body.clone());
            }
            let name = me.login.clone();
            self.chat_msg(&tags, &name, &wire);
            if let Some(m) = self.msgs.back_mut() {
                m.color = self.my_color;
                m.mention = false;
            }
        }
        self.input.clear();
        self.tab = None;
        self.reply = None;
        self.jump_bottom = true;
    }

    /// Autocompletado con Tab: emotes del canal, tus emotes y gente que habló. Tab repetido cicla.
    /// Sugerencias mientras escribes, como twitch.tv: "@ab" -> usuarios del chat, ":ab" -> emotes.
    /// Devuelve (texto a insertar, url del emote si es emote), máximo 8.
    fn suggestions(&self) -> Vec<(String, Option<String>)> {
        let word = self.input.rsplit(' ').next().unwrap_or_default();
        if let Some(q) = word.strip_prefix('@').filter(|q| !q.is_empty()) {
            let q = q.to_lowercase();
            return self.chatters.iter().rev().filter(|n| n.to_lowercase().starts_with(&q)).take(8).map(|n| (format!("@{n}"), None)).collect();
        }
        if let Some(q) = word.strip_prefix(':').filter(|q| q.len() >= 2) {
            let q = q.to_lowercase();
            let mut out: Vec<(String, Option<String>)> = Vec::new();
            for (name, url) in self.chan_emotes.iter().chain(&self.my_emotes) {
                if name.to_lowercase().contains(&q) && !out.iter().any(|o| &o.0 == name) {
                    out.push((name.clone(), Some(url.clone())));
                    if out.len() == 8 {
                        break;
                    }
                }
            }
            return out;
        }
        Vec::new()
    }

    /// Cambia la última palabra por la sugerencia elegida.
    fn apply_suggestion(&mut self, s: &str) {
        let start = self.input.rfind(' ').map_or(0, |i| i + 1);
        self.input.truncate(start);
        self.input.push_str(s);
        self.input.push(' ');
        self.tab = None;
    }

    fn complete(&mut self) -> bool {
        let start = self.input.rfind(' ').map_or(0, |i| i + 1);
        let (prefix, idx) = match &self.tab {
            Some((p, i, s)) if *s == start => (p.clone(), i + 1),
            _ => (self.input[start..].trim_start_matches('@').to_lowercase(), 0),
        };
        if prefix.is_empty() {
            return false;
        }
        let mut cands: Vec<&String> = self.chan_emotes.iter().chain(self.my_emotes.iter()).map(|e| &e.0).chain(self.chatters.iter().rev()).filter(|n| n.to_lowercase().starts_with(&prefix)).collect();
        cands.dedup();
        let Some(pick) = cands.get(idx % cands.len().max(1)).map(|s| s.to_string()) else { return false };
        let at = if self.input[start..].starts_with('@') { "@" } else { "" };
        self.input.truncate(start);
        self.input.push_str(at);
        self.input.push_str(&pick);
        self.tab = Some((prefix, idx, start));
        true
    }

    fn set_video(&mut self, on: bool, gl: Option<&eframe::glow::Context>) {
        self.cfg.video = on;
        self.cfg.save();
        if on {
            if !self.chan.is_empty() {
                twitch::load_stream(&self.net, self.chan.clone(), self.session());
            }
        } else if let Some(gl) = gl {
            if let Some(ap) = self.ad_player.take() { ap.destroy(gl); }
            if let Some(p) = self.player.take() { p.destroy(gl); }
            self.qualities.clear();
            self.playing.clear();
        }
    }

    /// En Auto, cambia de variante si el tamaño del player lo pide por 3 s seguidos (evita rebotes al redimensionar).
    fn auto_quality(&mut self, ctx: &egui::Context) {
        if self.cfg.quality != "Auto" || self.player.is_none() || self.chan.is_empty() {
            return;
        }
        let Some(best) = auto_pick(&self.qualities, self.want_h).cloned() else { return };
        if best.0 == self.playing {
            self.auto_t = None;
            return;
        }
        let t = *self.auto_t.get_or_insert_with(Instant::now);
        if self.playing.is_empty() || t.elapsed() >= Duration::from_secs(3) {
            if let Some(p) = &self.player {
                p.play(&best.1);
            }
            self.playing = best.0;
            self.paused = false;
            self.auto_t = None;
        } else {
            ctx.request_repaint_after(Duration::from_secs(3));
        }
    }

    /// Baja latencia como la de Twitch: si el búfer crece más de 3 s (te quedaste atrás del directo),
    /// reproduce al 1.08x hasta bajar a 1.5 s. El audio se corrige de tono, no se nota.
    /// Se revisa una vez por segundo.
    fn latency(&mut self) {
        if self.lat_t.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.lat_t = Instant::now();
        let Some(p) = &self.player else { return };
        self.buffer = p.num("demuxer-cache-duration").unwrap_or(0.0);
        // si tú regresaste con la barra, no se acelera solo hacia el directo
        let want = self.cfg.low_latency && !self.paused && !self.rewound && self.buffer > if self.catching_up { 1.5 } else { 3.0 };
        if want != self.catching_up {
            self.catching_up = want;
            p.set("speed", if want { "1.08" } else { "1" });
        }
    }

    /// Rango que se puede recorrer con la barra: (inicio, posición, fin) en segundos de mpv.
    /// El inicio se estima: desde que entraste (time-pos arranca en 0 al cargar), pero no más de lo
    /// que caben 16 MiB de caché hacia atrás al bitrate actual (mpv no expone el rango por propiedad simple).
    fn rewind_range(&self) -> Option<(f64, f64, f64)> {
        let p = self.player.as_ref()?;
        let (pos, end) = (p.num("time-pos")?, p.num("demuxer-cache-time")?);
        let rate = p.num("video-bitrate").unwrap_or(0.0) + p.num("audio-bitrate").unwrap_or(0.0);
        let back = (16.0 * 1024.0 * 1024.0 * 8.0) / rate.max(500_000.0) * 0.9;
        let start = (end - back).max(0.0).min(pos);
        Some((start, pos, end))
    }

    /// "Ir al directo": recarga la variante actual, que arranca en el borde del directo.
    fn go_live(&mut self) {
        let url = self.qualities.iter().find(|q| q.0 == self.playing).map(|q| q.1.clone());
        if let (Some(p), Some(u)) = (&self.player, url) {
            p.play(&u);
            self.paused = false;
            self.rewound = false;
        }
    }

    fn pick_quality(&mut self, q: Option<(String, String)>) {
        match q {
            Some(q) => {
                if let Some(p) = &self.player {
                    p.play(&q.1);
                }
                self.playing = q.0.clone();
                self.cfg.quality = q.0;
            }
            None => {
                self.cfg.quality = "Auto".into();
                self.playing.clear(); // auto_quality la vuelve a elegir
            }
        }
        self.paused = false;
        self.cfg.save();
    }

    /// Atajos como en twitch.tv (solo si no estás escribiendo).
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || self.page != Page::Channel {
            return;
        }
        let (f, m, t, k) = ctx.input(|i| (i.key_pressed(egui::Key::F), i.key_pressed(egui::Key::M), i.key_pressed(egui::Key::T), i.key_pressed(egui::Key::K) || i.key_pressed(egui::Key::Space)));
        if ctx.input(|i| i.key_pressed(egui::Key::C)) {
            self.cfg.chat_overlay = !self.cfg.chat_overlay;
            self.chat_open = true;
            self.cfg.save();
        }
        if f {
            let fs = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fs));
        }
        if m {
            self.muted = !self.muted;
            if let Some(p) = &self.player {
                p.set("mute", if self.muted { "yes" } else { "no" });
            }
        }
        if t {
            self.theater = !self.theater;
        }
        if k {
            if let Some(p) = &self.player {
                p.toggle_pause();
                self.paused = !self.paused;
            }
        }
    }
}

impl eframe::App for App {
    /// Corre en cada pasada y también con la ventana minimizada (si algo pidió repintado).
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(ev) = self.rx.try_recv() {
            self.handle(ev);
        }
        // Minimizada: mpv deja de decodificar video (el audio sigue). Ahorra la mayor parte del CPU/GPU.
        let minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
        if minimized != self.vid_off {
            self.vid_off = minimized;
            if let Some(p) = &self.player {
                p.set("vid", if minimized { "no" } else { "auto" });
            }
        }
        // Buscador: espera 300 ms sin teclear antes de pedir sugerencias
        if let Some(t) = self.search_t {
            if t.elapsed() >= Duration::from_millis(300) {
                self.search_t = None;
                if self.search.trim().len() >= 2 {
                    twitch::search(&self.net, self.search.trim().to_string());
                }
            } else {
                ctx.request_repaint_after(Duration::from_millis(320));
            }
        }
        if !self.chan.is_empty() && self.info_t.elapsed() >= Duration::from_secs(60) {
            self.info_t = Instant::now();
            twitch::load_info(&self.net, self.chan.clone());
            if let Some(a) = self.me.as_ref().filter(|a| a.web) {
                twitch::load_points(&self.net, a.token.clone(), self.chan.clone(), self.cfg.auto_claim);
            }
        }
        self.focused = ctx.input(|i| i.focused);
        // cada minuto: sidebar y aviso de canales que entran en vivo
        if self.side_t.elapsed() >= Duration::from_secs(60) {
            self.side_t = Instant::now();
            twitch::load_top(&self.net, "VIEWER_COUNT");
            if let Some(a) = &self.me {
                twitch::load_follows(&self.net, a.clone());
            }
            if let Some(t) = self.session() {
                twitch::load_notifs(&self.net, t); // punto rojo de la campana
            }
        }
        self.shortcuts(ctx);
        ctx.request_repaint_after(Duration::from_secs(60));
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.draw(ui, frame);
        // Captura de prueba: egui entrega el cuadro ya renderizado de esta ventana (no de la pantalla)
        if let Some((path, t0, asked)) = &mut self.shot {
            let wait = std::env::var("TWITCHLITE_SHOT_WAIT").ok().and_then(|s| s.parse().ok()).unwrap_or(25);
            if !*asked && t0.elapsed() >= Duration::from_secs(wait) {
                *asked = true;
                let path = path.clone();
                ui.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |im: std::sync::Arc<egui::ColorImage>| {
                    let [w, h] = im.size;
                    let bytes: Vec<u8> = im.pixels.iter().flat_map(|c| c.to_array()).collect();
                    let _ = image::save_buffer(&path, &bytes, w as u32, h as u32, image::ExtendedColorType::Rgba8);
                })));
            } else if !*asked {
                ui.ctx().request_repaint_after(Duration::from_millis(500));
            }
        }
        self.auto_quality(ui.ctx());
        self.latency();
        if self.stats_open {
            ui.ctx().request_repaint_after(Duration::from_secs(1));
        }
        // Un solo repintado para el próximo cambio de frame de cualquier emote visible, con tope de 30 fps.
        // Sin foco (p. ej. jugando con la app encima) las animaciones se congelan si así está configurado.
        let focused = ui.input(|i| i.focused);
        if let Some(w) = self.store.take_wake() {
            if focused || !self.cfg.pause_unfocused {
                ui.ctx().request_repaint_after(w.max(Duration::from_millis(33)));
            }
        }
    }

    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        if let Some(gl) = gl {
            if let Some(ap) = self.ad_player.take() { ap.destroy(gl); }
            if let Some(p) = self.player.take() { p.destroy(gl); }
        }
    }

    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.055, 0.055, 0.063, 1.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_filters_mask() {
        // "puto" (P) y "x" (S): solo se tapa lo de las categorías activas, mismo largo
        assert_eq!(mask_flags("hola puto x", "5-8:P.6,10-10:S.3", 8), "hola **** x");
        assert_eq!(mask_flags("hola puto x", "5-8:P.6,10-10:S.3", 15), "hola **** *");
        assert_eq!(mask_flags("hola", "", 15), "hola");
        assert_eq!(mask_flags("hola", "2-99:A.7", 15), "hola"); // rango fuera del texto: se ignora
    }

    #[test]
    fn auto_quality_pick() {
        let q: Vec<(String, String)> = ["1080p60 (source)", "720p60", "480p30", "160p30", "audio_only"].iter().map(|n| (n.to_string(), String::new())).collect();
        assert_eq!(auto_pick(&q, 690).unwrap().0, "720p60");
        assert_eq!(auto_pick(&q, 721).unwrap().0, "1080p60 (source)");
        assert_eq!(auto_pick(&q, 2160).unwrap().0, "1080p60 (source)"); // nada alcanza: la más alta
        assert_eq!(auto_pick(&q, 100).unwrap().0, "160p30");
    }

    #[test]
    fn tokenize_emotes_and_emoji() {
        let store = img::Store::new(egui::Context::default(), 1 << 20);
        let mut e = HashMap::new();
        e.insert("OMEGALUL".to_string(), Emote { url: "u1".into(), zw: false });
        e.insert("RainTime".to_string(), Emote { url: "u2".into(), zw: true });
        let px = Px { emote: 28, emoji: 20, badge: 18 };
        let p = tokenize("Kappa hola OMEGALUL RainTime 🎀fin", "25:0-4", &e, &store, px, false);
        assert!(matches!(&p[0], Part::Emote(l) if l[0].1 == "Kappa"));
        assert!(matches!(&p[1], Part::Text(t) if t == " hola "));
        assert!(matches!(&p[2], Part::Emote(l) if l.len() == 2)); // zero-width encima
        assert!(matches!(&p[4], Part::Emoji(_, s) if s == "🎀"));
        assert!(matches!(&p[5], Part::Text(t) if t == "fin"));
        assert_eq!(tokenize("hi", "25:0-40", &e, &store, px, false).len(), 1); // índices fuera de rango
    }

    /// Manual: `cargo test toast_shows -- --ignored` should show a native toast on Windows.
    #[test]
    #[ignore]
    fn toast_shows() {
        notify_rust::Notification::new().appname("Twitch Lite").summary("Someone mentioned you in #testchannel").body("Notification test for Twitch Lite").show().unwrap();
    }

    #[test]
    fn local_time() {
        assert_eq!(hhmm(0, 0), "0:00");
        assert_eq!(hhmm(1_792_630_870_000, -6 * 3600), "19:01"); // 01:01 UTC en CDMX
    }
}

/// Si libmpv-2.dll no está junto al .exe, intenta descargarlo del release de GitHub.
fn ensure_libmpv() {
    let dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())).unwrap_or_default();
    let dll = dir.join("libmpv-2.dll");
    if dll.exists() { return; }
    eprintln!("libmpv-2.dll no encontrado, descargando…");
    let url = "https://github.com/romanovak/twitchlite/releases/latest/download/libmpv-2.dll";
    let Ok(mut resp) = ureq::get(url).call() else {
        eprintln!("No se pudo descargar libmpv-2.dll — el video no estará disponible.");
        return;
    };
    if let Ok(buf) = resp.body_mut().with_config().limit(80 << 20).read_to_vec() {
        let _ = std::fs::write(&dll, &buf);
        eprintln!("libmpv-2.dll descargado ({:.1} MB)", buf.len() as f64 / 1e6);
    }
}

fn main() -> eframe::Result {
    std::panic::set_hook(Box::new(|info| {
        tlog(&format!("PANIC: {info}"));
    }));
    tlog("=== START ===");
    ensure_libmpv();
    let cfg = Cfg::load();
    // sin marco de Windows: la barra superior de la app hace de barra de título (ver ui::top_bar)
    let mut vp = egui::ViewportBuilder::default().with_title("Twitch Lite").with_inner_size([1280.0, 720.0]).with_min_inner_size([360.0, 240.0]).with_decorations(false);
    if cfg.on_top {
        vp = vp.with_always_on_top();
    }
    let opts = eframe::NativeOptions { viewport: vp, renderer: eframe::Renderer::Glow, ..Default::default() };
    eframe::run_native(
        "Twitch Lite",
        opts,
        Box::new(|cc| {
            ui::style(&cc.egui_ctx);
            Ok(Box::new(App::new(cc, cfg)))
        }),
    )
}
