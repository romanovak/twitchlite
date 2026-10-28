// Red: IRC del chat, APIs de emotes (7TV/BTTV/FFZ), Helix, login por device code y stream HLS.
// Todo corre en hilos con ureq/rustls bloqueantes y habla con la UI por un canal mpsc.
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

// Client-ID público del reproductor web de twitch.tv. Solo se usa para pedir el token de reproducción
// (lo mismo que hace streamlink); el login usa el Client-ID de tu propia app.
const WEB_CLIENT_ID: &str = "kimne78kx3ncx6brgo4mv6wki5h1ko";
const SCOPES: &str = "chat:read chat:edit user:read:follows user:read:emotes";

pub struct Emote {
    pub url: String,
    pub zw: bool,
}

#[derive(Clone)]
pub struct Auth {
    pub login: String,
    pub id: String,
    pub token: String,
    pub client_id: String,
    /// Sesión de twitch.tv (cliente oficial): habilita GQL con cuenta (puntos, seguidos, tus emotes).
    pub web: bool,
}

#[derive(Clone, Default)]
pub struct Live {
    pub login: String,
    pub name: String,
    pub game: String,
    pub viewers: Option<u64>, // None = desconectado
    pub avatar: String,
    pub title: String,
    pub preview: String,
    pub tags: Vec<String>,
}

fn tags_of(stream: &Value) -> Vec<String> {
    stream["freeformTags"].as_array().into_iter().flatten().map(|t| s(&t["name"])).collect()
}

#[derive(Clone)]
pub struct BadgeOpt {
    pub set: String,
    pub ver: String,
    pub title: String,
    pub url: String,
}

/// "Identidad en el chat" de twitch.tv: color del nombre y emblemas elegidos/disponibles.
#[derive(Default)]
pub struct Identity {
    pub color: String,
    pub chan_id: String,
    pub global_sel: String, // "set/version"
    pub chan_sel: String,
    pub globals: Vec<BadgeOpt>,
    pub chans: Vec<BadgeOpt>,
}

pub struct DropItem {
    pub name: String,
    pub image: String,
    pub date: String,
}

pub struct SubItem {
    pub login: String,
    pub name: String,
    pub avatar: String,
    pub tier: String,
    pub ends: String,
}

pub struct Game {
    pub name: String,
    pub art: String,
    pub viewers: u64,
}

#[derive(Default)]
pub struct UserCard {
    pub login: String,
    pub name: String,
    pub avatar: String,
    pub created: String,
    pub followers: u64,
    pub bio: String,
}

#[derive(Default)]
pub struct ChanInfo {
    pub name: String,
    pub avatar: String,
    pub title: String,
    pub game: String,
    pub viewers: Option<u64>,
    pub started: Option<i64>, // unix segundos
    pub bio: String,
    pub followers: u64,
    pub tags: Vec<String>,
    pub partner: bool,
    pub panels: Vec<Panel>,
}

/// Panel de "Acerca de" del canal (imagen con enlace y/o texto).
#[derive(Clone)]
pub struct Panel {
    pub title: String,
    pub image: String,
    pub link: String,
    pub desc: String,
}

pub struct Reward {
    pub title: String,
    pub cost: u64,
    pub image: String,
    pub color: String,
    pub prompt: String,
}

pub struct Notif {
    pub body: String,
    pub thumb: String,
    pub date: String,
    pub read: bool,
}

pub struct Irc {
    pub cmd: String,
    pub tags: HashMap<String, String>,
    pub nick: String,
    pub text: String,
}

pub enum Ev {
    Line(u64, Irc),
    Status(u64, String),
    Emotes(String, HashMap<String, Emote>),
    Badges(HashMap<String, String>),
    Auth(Option<Auth>),
    Device(String, String),
    Follows(Vec<Live>),
    Top(Vec<Live>),
    Avatar(String),
    MyEmotes(Vec<(String, String)>),
    Stream(String, Result<Vec<(String, String)>, String>),
    Info(String, ChanInfo),
    Points(String, u64),
    Bits(u64),
    Identity(Identity),
    Rewards(String, Vec<Reward>),
    Notifs(Vec<Notif>),
    Drops(Vec<DropItem>),
    Subs(Vec<SubItem>),
    Card(UserCard),
    Games(Vec<Game>),
    Streams(Option<String>, Vec<Live>),
    Search(String, Vec<Live>),
    AdStatus(bool),
    CleanStream(String, Result<Vec<(String, String)>, String>),
}

// ---------- Secretos: Administrador de credenciales de Windows / Llavero de macOS ----------

fn entry(name: &str) -> Option<keyring::Entry> {
    keyring::Entry::new("TwitchLite", name).ok()
}
pub fn secret(name: &str) -> String {
    entry(name).and_then(|e| e.get_password().ok()).unwrap_or_default()
}
pub fn set_secret(name: &str, v: &str) {
    if let Some(e) = entry(name) {
        let _ = if v.is_empty() { e.delete_credential() } else { e.set_password(v) };
    }
}

#[derive(Clone)]
pub struct Net {
    pub tx: Sender<Ev>,
    pub ctx: eframe::egui::Context,
}

impl Net {
    fn send(&self, ev: Ev) {
        let _ = self.tx.send(ev);
        self.ctx.request_repaint();
    }
    fn spawn(&self, f: impl FnOnce(Net) + Send + 'static) {
        let n = self.clone();
        thread::spawn(move || f(n));
    }
}

// ---------- HTTP ----------

/// Un solo agente con keep-alive para todo: reusa conexiones TLS en vez de un handshake por emote.
fn agent() -> &'static ureq::Agent {
    static A: OnceLock<ureq::Agent> = OnceLock::new();
    A.get_or_init(|| ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(15))).build().into())
}

/// Bytes crudos (imágenes). Tope de 8 MB por archivo.
pub fn get_bytes(url: &str) -> Option<Vec<u8>> {
    agent().get(url).call().ok()?.body_mut().with_config().limit(8 << 20).read_to_vec().ok()
}

fn get(url: &str, headers: &[(&str, &str)]) -> Option<Value> {
    let mut req = agent().get(url);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    serde_json::from_str(&req.call().ok()?.body_mut().read_to_string().ok()?).ok()
}

fn post_form(url: &str, form: &[(&str, &str)]) -> Option<Value> {
    let r = agent().post(url).send_form(form.iter().copied()).ok()?.body_mut().read_to_string().ok()?;
    serde_json::from_str(&r).ok()
}

fn gql_q(q: &str) -> Option<Value> {
    gql(&json!({ "query": q }))
}

fn gql(body: &Value) -> Option<Value> {
    gql_as(body, None)
}

/// GQL con tu sesión de twitch.tv. Solo lecturas: lo que Twitch protege con el token anti-bots
/// (reclamar puntos, seguir, paginar) responde "failed integrity check" y no se intenta saltar.
fn gql_as(body: &Value, token: Option<&str>) -> Option<Value> {
    let mut req = agent().post("https://gql.twitch.tv/gql").header("Client-Id", WEB_CLIENT_ID).header("Content-Type", "application/json");
    if let Some(t) = token {
        req = req.header("Authorization", &format!("OAuth {t}"));
    }
    let r = req.send(body.to_string()).ok()?.body_mut().read_to_string().ok()?;
    serde_json::from_str(&r).ok()
}

/// Consulta persistida (Twitch solo acepta algunas por hash). Si Twitch cambia el hash, devuelve None
/// y la función que la usa se degrada sin romper nada.
fn persisted(op: &str, hash: &str, vars: Value, token: &str) -> Option<Value> {
    let body = json!({"operationName": op, "variables": vars, "extensions": {"persistedQuery": {"version": 1, "sha256Hash": hash}}});
    gql_as(&body, Some(token)).filter(|v| v["errors"].is_null())
}

fn helix(path: &str, cid: &str, token: &str) -> Option<Value> {
    get(
        &format!("https://api.twitch.tv/helix/{path}"),
        &[("Client-Id", cid), ("Authorization", &format!("Bearer {token}"))],
    )
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

fn https(u: &str) -> String {
    if u.starts_with("//") { format!("https:{u}") } else { u.to_string() }
}

fn enc(v: &str) -> String {
    v.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn rnd() -> u32 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().subsec_nanos()
}

/// URL de emote nativo de Twitch: `default` devuelve GIF si es animado y PNG si no.
pub fn twitch_emote(id: &str, hi: bool) -> String {
    format!("https://static-cdn.jtvnw.net/emoticons/v2/{id}/default/dark/{}", if hi { "2.0" } else { "1.0" })
}

/// "2026-10-22T01:01:10Z" -> segundos unix (algoritmo days_from_civil de Howard Hinnant).
pub fn parse_iso(t: &str) -> Option<i64> {
    let n = |a: usize, b: usize| t.get(a..b)?.parse::<i64>().ok();
    let (y, m, d) = (n(0, 4)?, n(5, 7)?, n(8, 10)?);
    let (hh, mm, ss) = (n(11, 13)?, n(14, 16)?, n(17, 19)?);
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146_097 + doe - 719_468) * 86_400 + hh * 3600 + mm * 60 + ss)
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn gql_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_default()
}

/// Título, juego, espectadores y avatar del canal (sin login). Se vuelve a pedir cada minuto.
pub fn load_info(net: &Net, chan: String) {
    net.spawn(move |net| {
        let q = format!("query{{user(login:{}){{displayName profileImageURL(width:70) description followers{{totalCount}} roles{{isPartner}} panels{{type ... on DefaultPanel{{title imageURL linkURL description}}}} broadcastSettings{{title game{{displayName}}}} stream{{viewersCount createdAt freeformTags{{name}} game{{displayName}}}}}}}}", gql_str(&chan));
        let Some(v) = gql_q(&q) else { return };
        let u = &v["data"]["user"];
        let live = !u["stream"].is_null();
        let info = ChanInfo {
            name: s(&u["displayName"]),
            avatar: s(&u["profileImageURL"]),
            title: clean(&s(&u["broadcastSettings"]["title"])),
            game: s(if live { &u["stream"]["game"]["displayName"] } else { &u["broadcastSettings"]["game"]["displayName"] }),
            viewers: u["stream"]["viewersCount"].as_u64(),
            started: u["stream"]["createdAt"].as_str().and_then(parse_iso),
            bio: s(&u["description"]),
            followers: u["followers"]["totalCount"].as_u64().unwrap_or(0),
            tags: u["stream"]["freeformTags"].as_array().into_iter().flatten().map(|t| s(&t["name"])).collect(),
            partner: u["roles"]["isPartner"].as_bool().unwrap_or(false),
            // paneles del canal (los de extensión no se pueden mostrar fuera de la web)
            panels: u["panels"].as_array().into_iter().flatten().filter(|p| s(&p["type"]) == "DEFAULT").map(|p| Panel { title: s(&p["title"]), image: s(&p["imageURL"]), link: s(&p["linkURL"]), desc: s(&p["description"]) }).collect(),
        };
        net.send(Ev::Info(chan, info));
    });
}

pub fn load_avatar(net: &Net, login: String) {
    net.spawn(move |net| {
        if let Some(v) = gql_q(&format!("query{{user(login:{}){{profileImageURL(width:50)}}}}", gql_str(&login))) {
            net.send(Ev::Avatar(s(&v["data"]["user"]["profileImageURL"])));
        }
    });
}

const STREAM_FIELDS: &str = "title viewersCount previewImageURL(width:320,height:180) freeformTags{name} game{displayName} broadcaster{login displayName profileImageURL(width:50)}";

fn streams_from(edges: &Value) -> Vec<Live> {
    edges.as_array().into_iter().flatten().map(|e| {
        let n = &e["node"];
        Live {
            login: s(&n["broadcaster"]["login"]),
            name: s(&n["broadcaster"]["displayName"]),
            game: s(&n["game"]["displayName"]),
            viewers: n["viewersCount"].as_u64(),
            avatar: s(&n["broadcaster"]["profileImageURL"]),
            title: clean(&s(&n["title"])),
            preview: s(&n["previewImageURL"]),
            tags: tags_of(n),
        }
    }).collect()
}

/// "Canales en directo": los más vistos ahora mismo (sin login). Sidebar y página principal.
/// sort: "VIEWER_COUNT" (default) o "RECENT".
pub fn load_top(net: &Net, sort: &str) {
    let sort = sort.to_string();
    net.spawn(move |net| {
        let v = gql_q(&format!("query{{streams(first:24,options:{{sort:{sort}}}){{edges{{node{{{STREAM_FIELDS}}}}}}}}}")).unwrap_or_default();
        net.send(Ev::Top(streams_from(&v["data"]["streams"]["edges"])));
    });
}

/// Directos de una categoría.
/// sort: "VIEWER_COUNT" (default) o "RECENT".
pub fn load_streams(net: &Net, game: String, sort: &str) {
    let sort = sort.to_string();
    net.spawn(move |net| {
        let v = gql_q(&format!("query{{game(name:{}){{viewersCount streams(first:30,options:{{sort:{sort}}}){{edges{{node{{{STREAM_FIELDS}}}}}}}}}}}", gql_str(&game))).unwrap_or_default();
        net.send(Ev::Streams(Some(game), streams_from(&v["data"]["game"]["streams"]["edges"])));
    });
}

/// Categorías más vistas (página Explorar).
pub fn load_games(net: &Net) {
    net.spawn(|net| {
        let v = gql_q("query{games(first:40){edges{node{name boxArtURL(width:144,height:192) viewersCount}}}}").unwrap_or_default();
        let list = v["data"]["games"]["edges"].as_array().into_iter().flatten().map(|e| {
            let n = &e["node"];
            Game { name: s(&n["name"]), art: s(&n["boxArtURL"]), viewers: n["viewersCount"].as_u64().unwrap_or(0) }
        });
        net.send(Ev::Games(list.collect()));
    });
}

/// Sugerencias del buscador: canales que coinciden, en vivo primero.
pub fn search(net: &Net, q: String) {
    net.spawn(move |net| {
        let v = gql_q(&format!("query{{searchFor(userQuery:{}, platform:\"web\"){{channels{{items{{login displayName profileImageURL(width:50) stream{{viewersCount game{{displayName}}}}}}}}}}}}", gql_str(&q))).unwrap_or_default();
        let mut list: Vec<Live> = v["data"]["searchFor"]["channels"]["items"].as_array().into_iter().flatten().take(8).map(|n| Live {
            login: s(&n["login"]),
            name: s(&n["displayName"]),
            game: s(&n["stream"]["game"]["displayName"]),
            viewers: n["stream"]["viewersCount"].as_u64(),
            avatar: s(&n["profileImageURL"]),
            ..Default::default()
        }).collect();
        list.sort_by(|a, b| b.viewers.cmp(&a.viewers));
        net.send(Ev::Search(q, list));
    });
}

/// Tarjeta de usuario al dar clic en un nombre del chat (datos públicos).
pub fn load_card(net: &Net, login: String) {
    net.spawn(move |net| {
        let q = format!("query{{user(login:{}){{login displayName profileImageURL(width:70) createdAt description followers{{totalCount}}}}}}", gql_str(&login));
        let Some(v) = gql_q(&q) else { return };
        let u = &v["data"]["user"];
        if u.is_null() {
            return;
        }
        let created = s(&u["createdAt"]).get(..10).map(|d| format!("{}/{}/{}", &d[8..10], &d[5..7], &d[0..4])).unwrap_or_default();
        net.send(Ev::Card(UserCard { login: s(&u["login"]), name: s(&u["displayName"]), avatar: s(&u["profileImageURL"]), created, followers: u["followers"]["totalCount"].as_u64().unwrap_or(0), bio: s(&u["description"]) }));
    });
}

fn date_es(iso: &str) -> String {
    iso.get(..10).map(|d| format!("{}/{}/{}", &d[8..10], &d[5..7], &d[0..4])).unwrap_or_default()
}

fn badge_opts(v: &Value) -> Vec<BadgeOpt> {
    v.as_array().into_iter().flatten().map(|b| BadgeOpt { set: s(&b["setID"]), ver: s(&b["version"]), title: s(&b["title"]), url: s(&b["imageURL"]) }).collect()
}

fn sel(v: &Value) -> String {
    if v.is_null() { String::new() } else { format!("{}/{}", s(&v["setID"]), s(&v["version"])) }
}

/// Identidad en el chat (sesión de twitch.tv): color y emblemas globales y del canal.
pub fn load_identity(net: &Net, token: String, chan: String) {
    net.spawn(move |net| {
        let q = format!(
            "query{{currentUser{{chatColor selectedBadge{{setID version}} availableBadges{{setID version title imageURL(size:DOUBLE)}}}} user(login:{}){{id self{{selectedBadge{{setID version}} availableBadges{{setID version title imageURL(size:DOUBLE)}}}}}}}}",
            gql_str(&chan)
        );
        let Some(v) = gql_as(&json!({ "query": q }), Some(&token)) else { return };
        let me = &v["data"]["currentUser"];
        let u = &v["data"]["user"];
        let globals = badge_opts(&me["availableBadges"]);
        // los del canal = disponibles en el canal que no son globales (sub, bits, fundador…)
        let chans = badge_opts(&u["self"]["availableBadges"]).into_iter().filter(|b| !globals.iter().any(|g| g.set == b.set && g.ver == b.ver)).collect();
        net.send(Ev::Identity(Identity { color: s(&me["chatColor"]), chan_id: s(&u["id"]), global_sel: sel(&me["selectedBadge"]), chan_sel: sel(&u["self"]["selectedBadge"]), globals, chans }));
    });
}

/// Cambia el color del nombre y vuelve a leer la identidad.
pub fn set_color(net: &Net, token: String, chan: String, color: String) {
    net.spawn(move |n| {
        let q = format!("mutation{{updateChatColor(input:{{color:{}}}){{user{{chatColor}}}}}}", gql_str(&color));
        let ok = gql_as(&json!({ "query": q }), Some(&token)).is_some_and(|v| v["errors"].is_null());
        if !ok {
            n.send(Ev::Status(0, "Twitch no aceptó ese color (los colores libres son de Turbo/Prime).".into()));
        }
        load_identity(&n, token, chan);
    });
}

/// Elige emblema global (`chan_id` vacío) o del canal.
pub fn set_badge(net: &Net, token: String, chan: String, chan_id: String, b: BadgeOpt) {
    net.spawn(move |n| {
        let q = if chan_id.is_empty() {
            format!("mutation{{selectGlobalBadge(input:{{badgeSetID:{}, badgeSetVersion:{}}}){{user{{id}}}}}}", gql_str(&b.set), gql_str(&b.ver))
        } else {
            format!("mutation{{selectChannelBadge(input:{{channelID:{}, badgeSetID:{}, badgeSetVersion:{}}}){{user{{id}}}}}}", gql_str(&chan_id), gql_str(&b.set), gql_str(&b.ver))
        };
        gql_as(&json!({ "query": q }), Some(&token));
        load_identity(&n, token, chan);
    });
}

/// Recompensas de puntos del canal (solo lectura: canjear se hace en twitch.tv).
pub fn load_rewards(net: &Net, token: Option<String>, chan: String) {
    net.spawn(move |net| {
        let q = format!("query{{channel(name:{}){{communityPointsSettings{{customRewards{{title cost isEnabled isPaused isInStock prompt backgroundColor image{{url2x}} defaultImage{{url2x}}}}}}}}}}", gql_str(&chan));
        let v = gql_as(&json!({ "query": q }), token.as_deref()).unwrap_or_default();
        let mut list: Vec<Reward> = v["data"]["channel"]["communityPointsSettings"]["customRewards"].as_array().into_iter().flatten()
            .filter(|r| r["isEnabled"].as_bool().unwrap_or(false) && !r["isPaused"].as_bool().unwrap_or(false) && r["isInStock"].as_bool().unwrap_or(true))
            .map(|r| Reward {
                title: s(&r["title"]),
                cost: r["cost"].as_u64().unwrap_or(0),
                image: Some(s(&r["image"]["url2x"])).filter(|u| !u.is_empty()).unwrap_or_else(|| s(&r["defaultImage"]["url2x"])),
                color: s(&r["backgroundColor"]),
                prompt: s(&r["prompt"]),
            })
            .collect();
        list.sort_by_key(|r| r.cost);
        net.send(Ev::Rewards(chan, list));
    });
}

/// Bandeja de notificaciones de Twitch (sesión de twitch.tv).
pub fn load_notifs(net: &Net, token: String) {
    net.spawn(move |net| {
        let v = gql_as(&json!({"query": "query{currentUser{notifications(first:30){edges{node{body thumbnailURL createdAt isRead}}}}}"}), Some(&token)).unwrap_or_default();
        let list = v["data"]["currentUser"]["notifications"]["edges"].as_array().into_iter().flatten().map(|e| {
            let n = &e["node"];
            Notif { body: clean(&s(&n["body"])), thumb: s(&n["thumbnailURL"]), date: date_es(&s(&n["createdAt"])), read: n["isRead"].as_bool().unwrap_or(true) }
        });
        net.send(Ev::Notifs(list.collect()));
    });
}

/// Inventario de drops (sesión de twitch.tv).
pub fn load_drops(net: &Net, token: String) {
    net.spawn(move |net| {
        let v = gql_as(&json!({"query": "query{currentUser{inventory{gameEventDrops{name imageURL lastAwardedAt}}}}"}), Some(&token)).unwrap_or_default();
        let mut list: Vec<DropItem> = v["data"]["currentUser"]["inventory"]["gameEventDrops"].as_array().into_iter().flatten().map(|d| DropItem { name: s(&d["name"]), image: s(&d["imageURL"]), date: s(&d["lastAwardedAt"]) }).collect();
        list.sort_by(|a, b| b.date.cmp(&a.date));
        for d in &mut list {
            d.date = date_es(&d.date);
        }
        net.send(Ev::Drops(list));
    });
}

/// Tus suscripciones (sesión de twitch.tv).
pub fn load_subs(net: &Net, token: String) {
    net.spawn(move |net| {
        let q = "query{currentUser{subscriptionBenefits(criteria:{filter:ALL}, first:50){edges{node{tier endsAt renewsAt user{login displayName profileImageURL(width:70)}}}}}}";
        let v = gql_as(&json!({ "query": q }), Some(&token)).unwrap_or_default();
        let list = v["data"]["currentUser"]["subscriptionBenefits"]["edges"].as_array().into_iter().flatten().map(|e| {
            let n = &e["node"];
            let renews = s(&n["renewsAt"]);
            SubItem {
                login: s(&n["user"]["login"]),
                name: s(&n["user"]["displayName"]),
                avatar: s(&n["user"]["profileImageURL"]),
                tier: s(&n["tier"]).get(..1).map(|t| format!("Nivel {t}")).unwrap_or_default(),
                ends: if renews.is_empty() { format!("Termina el {}", date_es(&s(&n["endsAt"]))) } else { format!("Se renueva el {}", date_es(&renews)) },
            }
        });
        net.send(Ev::Subs(list.collect()));
    });
}

/// Saldo de Bits (solo con sesión de twitch.tv).
pub fn load_bits(net: &Net, token: String) {
    net.spawn(move |net| {
        if let Some(b) = gql_as(&json!({"query": "query{currentUser{bitsBalance}}"}), Some(&token)).and_then(|v| v["data"]["currentUser"]["bitsBalance"].as_u64()) {
            net.send(Ev::Bits(b));
        }
    });
}

/// Saldo de puntos de canal (solo con sesión de twitch.tv).
/// Con `claim` = true intenta reclamar el bonus automáticamente si hay uno disponible.
pub fn load_points(net: &Net, token: String, chan: String, claim: bool) {
    net.spawn(move |net| {
        let q = json!({"query": format!("query{{channel(name:{}){{id self{{communityPoints{{balance availableClaim{{id}}}}}}}}}}", gql_str(&chan))});
        let Some(v) = gql_as(&q, Some(&token)) else { return };
        let ch = &v["data"]["channel"];
        let cp = &ch["self"]["communityPoints"];
        let chan_id = s(&ch["id"]);
        if let Some(claim_id) = cp["availableClaim"]["id"].as_str().filter(|_| claim && !chan_id.is_empty()) {
            let m = json!({"query": format!(
                "mutation{{claimCommunityPoints(input:{{channelID:{},claimID:{}}}){{claim{{pointsEarnedTotal}} error{{code}}}}}}",
                gql_str(&chan_id), gql_str(claim_id)
            )});
            if let Some(r) = gql_as(&m, Some(&token)) {
                if let Some(earned) = r["data"]["claimCommunityPoints"]["claim"]["pointsEarnedTotal"].as_u64() {
                    net.send(Ev::Status(0, format!("+{earned} puntos de canal reclamados")));
                }
            }
        }
        if let Some(b) = cp["balance"].as_u64() {
            net.send(Ev::Points(chan, b));
        }
    });
}

// ---------- Emotes de terceros ----------

pub fn load_emotes(net: &Net, room: String, hi: bool) {
    net.spawn(move |net| {
        let (x, ffz) = if hi { ("2x", "2") } else { ("1x", "1") };
        let r = room.as_str();
        let [fg, fc, bg, bc, sg, sc] = thread::scope(|t| {
            let a = t.spawn(|| get("https://api.frankerfacez.com/v1/set/global", &[]));
            let b = t.spawn(|| get(&format!("https://api.frankerfacez.com/v1/room/id/{r}"), &[]));
            let c = t.spawn(|| get("https://api.betterttv.net/3/cached/emotes/global", &[]));
            let d = t.spawn(|| get(&format!("https://api.betterttv.net/3/cached/users/twitch/{r}"), &[]));
            let e = t.spawn(|| get("https://7tv.io/v3/emote-sets/global", &[]));
            let f = t.spawn(|| get(&format!("https://7tv.io/v3/users/twitch/{r}"), &[]));
            [a, b, c, d, e, f].map(|h| h.join().ok().flatten().unwrap_or(Value::Null))
        });
        let mut m = HashMap::new();
        let mut add = |n: String, url: String, zw: bool| {
            m.insert(n, Emote { url, zw });
        };
        // Orden = prioridad: lo último pisa (7TV del canal gana, como en las extensiones).
        let mut ffz_sets: Vec<&Value> = fg["default_sets"].as_array().into_iter().flatten().map(|i| &fg["sets"][i.to_string()]).collect();
        ffz_sets.extend(fc["sets"].as_object().into_iter().flat_map(|o| o.values()));
        for set in ffz_sets {
            for e in set["emoticons"].as_array().into_iter().flatten() {
                let u = e["urls"][ffz].as_str().or(e["urls"]["1"].as_str()).unwrap_or_default();
                add(s(&e["name"]), https(u), false);
            }
        }
        const BTTV_ZW: [&str; 8] = ["SoSnowy", "IceCold", "SantaHat", "TopHat", "ReinDeer", "CandyCane", "cvMask", "cvHazmat"];
        for e in bg.as_array().into_iter().flatten().chain(bc["channelEmotes"].as_array().into_iter().flatten()).chain(bc["sharedEmotes"].as_array().into_iter().flatten()) {
            let code = s(&e["code"]);
            let zw = BTTV_ZW.contains(&code.as_str());
            add(code, format!("https://cdn.betterttv.net/emote/{}/{x}.webp", s(&e["id"])), zw);
        }
        for e in sg["emotes"].as_array().into_iter().flatten().chain(sc["emote_set"]["emotes"].as_array().into_iter().flatten()) {
            let zw = e["flags"].as_u64().unwrap_or(0) & 1 != 0 || e["data"]["flags"].as_u64().unwrap_or(0) & 256 != 0;
            add(s(&e["name"]), format!("{}/{x}.webp", https(e["data"]["host"]["url"].as_str().unwrap_or_default())), zw);
        }
        net.send(Ev::Emotes(room, m));
    });
}

// ---------- Helix (requiere login) ----------

/// Badges globales + del canal por GQL: funcionan sin login, igual que en twitch.tv.
pub fn load_badges(net: &Net, chan: String, hi: bool) {
    net.spawn(move |net| {
        let size = if hi { "DOUBLE" } else { "NORMAL" };
        let q = format!("query{{badges{{setID version imageURL(size:{size})}} user(login:{}){{broadcastBadges{{setID version imageURL(size:{size})}}}}}}", gql_str(&chan));
        let Some(v) = gql_q(&q) else { return };
        let mut m = HashMap::new();
        // los del canal van después: pisan a los globales (p. ej. meses de sub)
        for b in v["data"]["badges"].as_array().into_iter().flatten().chain(v["data"]["user"]["broadcastBadges"].as_array().into_iter().flatten()) {
            m.insert(format!("{}/{}", s(&b["setID"]), s(&b["version"])), s(&b["imageURL"]));
        }
        net.send(Ev::Badges(m));
    });
}

/// Canales que sigues: en vivo primero (por espectadores), luego desconectados.
/// Con sesión de twitch.tv: consulta persistida `ChannelFollows` (100 más recientes; la página 2 pide
/// token anti-bots y no se intenta). Con app propia: Helix. El estado en vivo sale de GQL público.
pub fn load_follows(net: &Net, a: Auth) {
    net.spawn(move |net| {
        let logins: Vec<String> = if a.web {
            let v = persisted("ChannelFollows", "eecf815273d3d949e5cf0085cc5084cd8a1b5b7b6f7990cf43cb0beadf546907", json!({"limit": 100, "order": "DESC"}), &a.token).unwrap_or_default();
            v["data"]["user"]["follows"]["edges"].as_array().into_iter().flatten().map(|e| s(&e["node"]["login"])).collect()
        } else {
            let all = helix(&format!("channels/followed?first=100&user_id={}", a.id), &a.client_id, &a.token).unwrap_or_default();
            all["data"].as_array().into_iter().flatten().map(|x| s(&x["broadcaster_login"])).collect()
        };
        if logins.is_empty() {
            return;
        }
        let q = format!(
            "query{{users(logins:[{}]){{login displayName profileImageURL(width:50) stream{{title viewersCount previewImageURL(width:640,height:360) freeformTags{{name}} game{{displayName}}}}}}}}",
            logins.iter().map(|l| gql_str(l)).collect::<Vec<_>>().join(",")
        );
        let v = gql_q(&q).unwrap_or_default();
        let mut list: Vec<Live> = v["data"]["users"].as_array().into_iter().flatten().filter(|u| !u.is_null()).map(|u| Live {
            login: s(&u["login"]),
            name: s(&u["displayName"]),
            game: s(&u["stream"]["game"]["displayName"]),
            viewers: u["stream"]["viewersCount"].as_u64(),
            avatar: s(&u["profileImageURL"]),
            title: clean(&s(&u["stream"]["title"])),
            preview: s(&u["stream"]["previewImageURL"]),
            tags: tags_of(&u["stream"]),
        }).collect();
        list.sort_by(|a, b| b.viewers.cmp(&a.viewers));
        net.send(Ev::Follows(list));
    });
}

pub fn load_my_emotes(net: &Net, a: Auth, hi: bool) {
    net.spawn(move |net| {
        let mut out = Vec::new();
        if a.web {
            let v = gql_as(&json!({"query": "query{currentUser{emoteSets{emotes{id token}}}}"}), Some(&a.token)).unwrap_or_default();
            for set in v["data"]["currentUser"]["emoteSets"].as_array().into_iter().flatten() {
                for e in set["emotes"].as_array().into_iter().flatten() {
                    out.push((s(&e["token"]), twitch_emote(&s(&e["id"]), hi)));
                }
            }
        } else {
            let mut after = String::new();
            for _ in 0..10 {
                let Some(v) = helix(&format!("chat/emotes/user?user_id={}&after={after}", a.id), &a.client_id, &a.token) else { break };
                for e in v["data"].as_array().into_iter().flatten() {
                    out.push((s(&e["name"]), twitch_emote(&s(&e["id"]), hi)));
                }
                after = s(&v["pagination"]["cursor"]);
                if after.is_empty() {
                    break;
                }
            }
        }
        // los emotes "globales" de texto (":)", "<3") no sirven en el selector
        out.retain(|e| e.0.chars().any(|c| c.is_alphanumeric()));
        out.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
        out.dedup_by(|a, b| a.0 == b.0);
        net.send(Ev::MyEmotes(out));
    });
}

// ---------- Login ----------
// Dos caminos: (1) tu sesión de twitch.tv (cookie `auth-token`): sin registrar nada, natural.
// (2) Device code con el Client ID de tu propia app. Los tokens viven en el Administrador de credenciales.

/// (login, user_id, client_id, segundos restantes; 0 = no expira)
fn validate(token: &str) -> Option<(String, String, String, u64)> {
    let v = get("https://id.twitch.tv/oauth2/validate", &[("Authorization", &format!("OAuth {token}"))])?;
    Some((s(&v["login"]), s(&v["user_id"]), s(&v["client_id"]), v["expires_in"].as_u64().unwrap_or(3600)))
}

fn refresh(cid: &str, rt: &str) -> Option<(String, String)> {
    let v = post_form("https://id.twitch.tv/oauth2/token", &[("client_id", cid), ("grant_type", "refresh_token"), ("refresh_token", rt)])?;
    Some((s(&v["access_token"]), s(&v["refresh_token"])))
}

/// Valida el token (o lo renueva) y lo mantiene vivo en segundo plano. Guarda cada cambio en el llavero.
fn keep_alive(net: Net, cid: String, mut token: String, mut rt: String) {
    loop {
        let ok = match validate(&token) {
            Some(x) => Some(x),
            None if !rt.is_empty() => refresh(&cid, &rt).and_then(|(t, r)| {
                (token, rt) = (t, r);
                validate(&token)
            }),
            None => None,
        };
        let Some((login, id, client_id, exp)) = ok else {
            set_secret("token", "");
            set_secret("refresh", "");
            return net.send(Ev::Auth(None));
        };
        set_secret("token", &token);
        set_secret("refresh", &rt);
        let web = client_id == WEB_CLIENT_ID;
        net.send(Ev::Auth(Some(Auth { login, id, token: token.clone(), client_id, web })));
        // la sesión web no expira (exp = 0): se revalida cada hora por si la cerraste en el navegador
        thread::sleep(Duration::from_secs(if exp == 0 { 3600 } else { exp.saturating_sub(300).max(60) }));
        if exp != 0 {
            match refresh(&cid, &rt) {
                Some((t, r)) => (token, rt) = (t, r),
                None => {
                    set_secret("token", "");
                    return net.send(Ev::Auth(None));
                }
            }
        }
    }
}

/// Retoma la sesión guardada en el llavero, si hay.
pub fn resume(net: &Net, cid: String) {
    let token = secret("token");
    if token.is_empty() {
        return;
    }
    let rt = secret("refresh");
    net.spawn(move |net| keep_alive(net, cid, token, rt));
}

/// Saca el token de lo que pegues: URL de redirect OAuth, línea de cookies, o el token directo.
pub fn extract_token(raw: &str) -> String {
    if let Some(i) = raw.find("access_token=") {
        return raw[i + 13..].split(['&', ' ', '\t', '\r', '\n', '#']).next().unwrap_or_default().to_string();
    }
    match raw.find("auth-token=") {
        Some(i) => raw[i + 11..].split([';', ' ', '\t', '\r', '\n']).next().unwrap_or_default().to_string(),
        None => raw.trim().trim_start_matches("oauth:").to_string(),
    }
}

/// Login con tu sesión de twitch.tv.
pub fn import_web(net: &Net, raw: String) {
    let token = extract_token(&raw);
    net.spawn(move |net| {
        if validate(&token).is_none() {
            return net.send(Ev::Status(0, "Esa sesión no es válida. Copia de nuevo la cookie auth-token de twitch.tv.".into()));
        }
        keep_alive(net, String::new(), token, String::new());
    });
}

pub fn logout() {
    set_secret("token", "");
    set_secret("refresh", "");
}

pub fn device_login(net: &Net, cid: String) {
    net.spawn(move |net| {
        let Some(d) = post_form("https://id.twitch.tv/oauth2/device", &[("client_id", &cid), ("scopes", SCOPES)]) else {
            return net.send(Ev::Status(0, "No se pudo iniciar el login. ¿El Client ID es de una app tipo Public?".into()));
        };
        net.send(Ev::Device(s(&d["user_code"]), s(&d["verification_uri"])));
        let code = s(&d["device_code"]);
        let wait = d["interval"].as_u64().unwrap_or(5);
        for _ in 0..d["expires_in"].as_u64().unwrap_or(1800) / wait {
            thread::sleep(Duration::from_secs(wait));
            let grant = "urn:ietf:params:oauth:grant-type:device_code";
            // Mientras no autorices, Twitch responde 400 (authorization_pending) y ureq lo da como error.
            if let Some(v) = post_form("https://id.twitch.tv/oauth2/token", &[("client_id", &cid), ("scopes", SCOPES), ("device_code", &code), ("grant_type", grant)]) {
                return keep_alive(net, cid, s(&v["access_token"]), s(&v["refresh_token"]));
            }
        }
        net.send(Ev::Status(0, "El código de login expiró.".into()));
    });
}

// ---------- Video: token de reproducción + playlist maestra HLS ----------

/// Con tu sesión de twitch.tv el token de reproducción sale a tu nombre: si estás suscrito al canal
/// o tienes Turbo, Twitch no mete anuncios (igual que en la web).
pub fn load_stream(net: &Net, chan: String, session: Option<String>) {
    net.spawn(move |net| {
        let q = "query PlaybackAccessToken_Template($login: String!, $isLive: Boolean!, $vodID: ID!, $isVod: Boolean!, $playerType: String!) { streamPlaybackAccessToken(channelName: $login, params: {platform: \"web\", playerBackend: \"mediaplayer\", playerType: $playerType}) @include(if: $isLive) { value signature __typename } videoPlaybackAccessToken(id: $vodID, params: {platform: \"web\", playerBackend: \"mediaplayer\", playerType: $playerType}) @include(if: $isVod) { value signature __typename } }";
        let body = json!({"operationName": "PlaybackAccessToken_Template", "query": q,
            "variables": {"isLive": true, "login": chan, "isVod": false, "vodID": "", "playerType": "site"}});
        let res = (|| {
            let v = gql_as(&body, session.as_deref()).ok_or("Sin respuesta de Twitch")?;
            let t = &v["data"]["streamPlaybackAccessToken"];
            if t.is_null() {
                return Err("Canal no encontrado".to_string());
            }
            let url = format!(
                "https://usher.ttvnw.net/api/channel/hls/{chan}.m3u8?allow_source=true&allow_audio_only=true&fast_bread=true&p={}&sig={}&token={}",
                rnd() % 999_999, s(&t["signature"]), enc(&s(&t["value"]))
            );
            let m3u8 = agent().get(&url).call().map_err(|_| "Offline".to_string())?.body_mut().read_to_string().map_err(|e| e.to_string())?;
            Ok(parse_master(&m3u8))
        })();
        net.send(Ev::Stream(chan, res));
    });
}

/// Busca líneas `#EXT-X-DATERANGE` con CLASS que contenga "stitched-ad" — solo esas líneas
/// indican un segmento de anuncio activo, no URLs u otros metadatos del playlist.
pub fn start_ad_monitor(net: &Net, variant_url: String) {
    net.spawn(move |net| {
        let mut was_ad = false;
        loop {
            thread::sleep(Duration::from_secs(3));
            let body = match agent().get(&variant_url).call().and_then(|mut r| Ok(r.body_mut().read_to_string()?)) {
                Ok(b) => b,
                Err(e) => {
                    crate::tlog(&format!("AdMonitor: URL error ({e}), was_ad={was_ad}"));
                    if was_ad { net.send(Ev::AdStatus(false)); }
                    return;
                }
            };
            let is_ad = body.lines().any(|l| l.starts_with("#EXT-X-DATERANGE") && l.contains("stitched-ad"));
            if is_ad != was_ad {
                crate::tlog(&format!("AdMonitor: {was_ad} → {is_ad}"));
                was_ad = is_ad;
                net.send(Ev::AdStatus(is_ad));
            }
        }
    });
}

pub fn load_stream_clean(net: &Net, chan: String) {
    net.spawn(move |net| {
        let q = "query PlaybackAccessToken_Template($login: String!, $isLive: Boolean!, $vodID: ID!, $isVod: Boolean!, $playerType: String!) { streamPlaybackAccessToken(channelName: $login, params: {platform: \"web\", playerBackend: \"mediaplayer\", playerType: $playerType}) @include(if: $isLive) { value signature __typename } videoPlaybackAccessToken(id: $vodID, params: {platform: \"web\", playerBackend: \"mediaplayer\", playerType: $playerType}) @include(if: $isVod) { value signature __typename } }";
        let body = json!({"operationName": "PlaybackAccessToken_Template", "query": q,
            "variables": {"isLive": true, "login": chan, "isVod": false, "vodID": "", "playerType": "embed"}});
        let res = (|| {
            let v = gql_as(&body, None).ok_or("Sin respuesta de Twitch")?;
            let t = &v["data"]["streamPlaybackAccessToken"];
            if t.is_null() { return Err("Canal no encontrado".to_string()); }
            let url = format!(
                "https://usher.ttvnw.net/api/channel/hls/{chan}.m3u8?allow_source=true&allow_audio_only=true&fast_bread=true&p={}&sig={}&token={}",
                rnd() % 999_999, s(&t["signature"]), enc(&s(&t["value"]))
            );
            let m3u8 = agent().get(&url).call().map_err(|_| "Offline".to_string())?.body_mut().read_to_string().map_err(|e| e.to_string())?;
            Ok(parse_master(&m3u8))
        })();
        net.send(Ev::CleanStream(chan, res));
    });
}

/// (nombre de calidad, url de la variante) en el orden de la playlist (fuente primero).
fn parse_master(m: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut name = String::new();
    for l in m.lines() {
        if l.starts_with("#EXT-X-MEDIA") {
            name = l.split("NAME=\"").nth(1).and_then(|x| x.split('"').next()).unwrap_or_default().to_string();
        } else if !l.starts_with('#') && !l.trim().is_empty() {
            out.push((std::mem::take(&mut name), l.trim().to_string()));
        }
    }
    out
}

// ---------- IRC sobre TLS ----------

pub fn parse_irc(mut l: &str) -> Irc {
    let mut tags = HashMap::new();
    if let Some(rest) = l.strip_prefix('@') {
        let (t, r) = rest.split_once(' ').unwrap_or((rest, ""));
        for kv in t.split(';') {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            tags.insert(k.to_string(), v.replace("\\s", " ").replace("\\:", ";").replace("\\\\", "\\"));
        }
        l = r;
    }
    let mut nick = "";
    if let Some(rest) = l.strip_prefix(':') {
        let (p, r) = rest.split_once(' ').unwrap_or((rest, ""));
        nick = p.split('!').next().unwrap_or_default();
        l = r;
    }
    let (head, text) = l.split_once(" :").unwrap_or((l, ""));
    Irc { cmd: head.split(' ').next().unwrap_or_default().to_string(), tags, nick: nick.to_string(), text: text.to_string() }
}

/// Quita invisibles que egui pinta como cuadros: U+E0000 (7TV/Chatterino lo agregan para saltarse
/// el filtro de repetidos) y espacios de ancho cero. Los selectores de variación se quedan: el
/// tokenizador los necesita para armar el nombre del archivo de Twemoji.
pub fn clean(s: &str) -> String {
    s.chars().filter(|c| !matches!(c, '\u{E0000}' | '\u{200B}')).collect()
}

fn tls() -> Arc<rustls::ClientConfig> {
    static CFG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    CFG.get_or_init(|| {
        let roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
        Arc::new(
            rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .expect("tls")
                .with_root_certificates(roots)
                .with_no_client_auth(),
        )
    })
    .clone()
}

/// Conecta al chat y devuelve el emisor para mandar líneas crudas. Al soltar el emisor, el hilo termina.
pub fn irc(net: &Net, generation: u64, chan: String, auth: Option<Auth>) -> Sender<String> {
    let (out, rx) = channel::<String>();
    net.spawn(move |net| loop {
        match session(&net, generation, &chan, auth.as_ref(), &rx) {
            Ok(true) => return,
            Ok(false) => {}
            Err(e) => net.send(Ev::Status(generation, format!("Chat desconectado ({e}), reconectando…"))),
        }
        thread::sleep(Duration::from_secs(2));
    });
    out
}

fn session(net: &Net, generation: u64, chan: &str, auth: Option<&Auth>, rx: &Receiver<String>) -> io::Result<bool> {
    let tcp = TcpStream::connect("irc.chat.twitch.tv:6697")?;
    tcp.set_read_timeout(Some(Duration::from_millis(150)))?;
    let conn = rustls::ClientConnection::new(tls(), "irc.chat.twitch.tv".try_into().expect("host")).map_err(io::Error::other)?;
    let mut r = BufReader::new(rustls::StreamOwned::new(conn, tcp));
    let (pass, nick) = match auth {
        Some(a) => (format!("oauth:{}", a.token), a.login.clone()),
        None => ("x".into(), format!("justinfan{}", rnd() % 99_999)),
    };
    let w = r.get_mut();
    write!(w, "CAP REQ :twitch.tv/tags twitch.tv/commands\r\nPASS {pass}\r\nNICK {nick}\r\nJOIN #{chan}\r\n")?;
    w.flush()?;
    let mut buf = Vec::new();
    loop {
        loop {
            match rx.try_recv() {
                Ok(line) => {
                    let w = r.get_mut();
                    w.write_all(line.as_bytes())?;
                    w.write_all(b"\r\n")?;
                    w.flush()?;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(true),
            }
        }
        match r.read_until(b'\n', &mut buf) {
            Ok(0) => return Ok(false),
            Ok(_) if buf.ends_with(b"\n") => {
                let line = String::from_utf8_lossy(&buf).trim_end().to_string();
                buf.clear();
                if let Some(rest) = line.strip_prefix("PING") {
                    let w = r.get_mut();
                    write!(w, "PONG{rest}\r\n")?;
                    w.flush()?;
                    continue;
                }
                let m = parse_irc(&line);
                if m.cmd == "RECONNECT" {
                    return Ok(false);
                }
                net.send(Ev::Line(generation, m));
            }
            Ok(_) => {}
            Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => {}
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn irc_and_master() {
        let m = parse_irc("@color=#FF0000;display-name=Foo;emotes=25:0-4 :foo!foo@foo.tmi.twitch.tv PRIVMSG #bar :Kappa hola mundo");
        assert_eq!((m.cmd.as_str(), m.nick.as_str(), m.text.as_str()), ("PRIVMSG", "foo", "Kappa hola mundo"));
        assert_eq!(m.tags["emotes"], "25:0-4");
        assert_eq!(parse_irc("@system-msg=a\\sb :tmi.twitch.tv USERNOTICE #x").tags["system-msg"], "a b");
        let q = parse_master("#EXTM3U\n#EXT-X-MEDIA:TYPE=VIDEO,GROUP-ID=\"chunked\",NAME=\"1080p60 (source)\"\n#EXT-X-STREAM-INF:BANDWIDTH=1\nhttps://a/1.m3u8\n#EXT-X-MEDIA:NAME=\"audio_only\"\n#EXT-X-STREAM-INF:BANDWIDTH=2\nhttps://a/2.m3u8\n");
        assert_eq!(q, vec![("1080p60 (source)".into(), "https://a/1.m3u8".into()), ("audio_only".into(), "https://a/2.m3u8".into())]);
        assert_eq!(extract_token("last_login=x;auth-token=abc123;api_token=zz"), "abc123");
        assert_eq!(extract_token("  oauth:abc123 "), "abc123");
        assert_eq!(extract_token("https://www.twitch.tv/#access_token=tok999&scope=chat:read&token_type=bearer"), "tok999");
        assert_eq!(parse_iso("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso("2026-10-22T01:01:10Z"), Some(1_792_630_870));
    }
}
