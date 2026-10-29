// Video con libmpv cargada en runtime (libmpv-2.dll junto al .exe). Si no está, la app sigue
// funcionando solo con chat. mpv dibuja en un FBO de OpenGL que egui muestra como textura.
use eframe::egui;
use eframe::glow::{self, HasContext};
use libloading::Library;
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::sync::{Arc, OnceLock};

pub type GetProc = Arc<dyn Fn(&CStr) -> *const c_void + Send + Sync>;

#[repr(C)]
struct Param {
    kind: c_int,
    data: *mut c_void,
}
#[repr(C)]
struct GlInit {
    get_proc_address: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    ctx: *mut c_void,
}
#[repr(C)]
struct Fbo {
    fbo: c_int,
    w: c_int,
    h: c_int,
    internal_format: c_int,
}

type H = *mut c_void;
struct Api {
    create: unsafe extern "C" fn() -> H,
    initialize: unsafe extern "C" fn(H) -> c_int,
    set_option: unsafe extern "C" fn(H, *const c_char, *const c_char) -> c_int,
    set_property: unsafe extern "C" fn(H, *const c_char, *const c_char) -> c_int,
    get_property: unsafe extern "C" fn(H, *const c_char, c_int, *mut c_void) -> c_int,
    free: unsafe extern "C" fn(*mut c_void),
    command: unsafe extern "C" fn(H, *const *const c_char) -> c_int,
    destroy: unsafe extern "C" fn(H),
    rc_create: unsafe extern "C" fn(*mut H, H, *mut Param) -> c_int,
    rc_callback: unsafe extern "C" fn(H, unsafe extern "C" fn(*mut c_void), *mut c_void),
    rc_update: unsafe extern "C" fn(H) -> u64,
    rc_render: unsafe extern "C" fn(H, *mut Param) -> c_int,
    rc_free: unsafe extern "C" fn(H),
    _lib: Library,
}

unsafe extern "C" fn get_proc(ctx: *mut c_void, name: *const c_char) -> *mut c_void {
    unsafe { (*(ctx as *const GetProc))(CStr::from_ptr(name)) as *mut c_void }
}
unsafe extern "C" fn wake(ctx: *mut c_void) {
    unsafe { (*(ctx as *const egui::Context)).request_repaint() }
}

pub struct Player {
    api: Arc<Api>,
    h: H,
    rc: H,
    _gp: Box<GetProc>,
    _ctx: Box<egui::Context>,
    target: Option<(glow::Framebuffer, glow::Texture, egui::TextureId, [i32; 2])>,
    frame_serial: u64,
    ipc_name: Option<String>,
}

fn cs(s: &str) -> CString {
    CString::new(s).unwrap_or_default()
}

fn shared_api() -> Result<Arc<Api>, String> {
    static API: OnceLock<Result<Arc<Api>, String>> = OnceLock::new();
    API.get_or_init(|| {
        let dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())).unwrap_or_default();
        let names = if cfg!(windows) { ["libmpv-2.dll", "mpv-2.dll"] } else { ["libmpv.dylib", "libmpv.2.dylib"] };
        let lib = names.iter().find_map(|n| unsafe { Library::new(dir.join(n)).or_else(|_| Library::new(*n)) }.ok())
            .ok_or("Falta libmpv-2.dll junto al .exe")?;
        unsafe {
            macro_rules! sym {
                ($n:literal) => { *lib.get(concat!($n, "\0").as_bytes()).map_err(|e| e.to_string())? };
            }
            Ok(Arc::new(Api {
                create: sym!("mpv_create"), initialize: sym!("mpv_initialize"),
                set_option: sym!("mpv_set_option_string"), set_property: sym!("mpv_set_property_string"),
                get_property: sym!("mpv_get_property"), free: sym!("mpv_free"), command: sym!("mpv_command"),
                destroy: sym!("mpv_terminate_destroy"), rc_create: sym!("mpv_render_context_create"),
                rc_callback: sym!("mpv_render_context_set_update_callback"), rc_update: sym!("mpv_render_context_update"),
                rc_render: sym!("mpv_render_context_render"), rc_free: sym!("mpv_render_context_free"), _lib: lib,
            }))
        }
    }).clone()
}

impl Player {
    pub fn new(gp: GetProc, ctx: egui::Context, volume: f32) -> Result<Self, String> {
        Self::create(gp, ctx, volume, false)
    }

    pub fn standby(gp: GetProc, ctx: egui::Context, volume: f32) -> Result<Self, String> {
        Self::create(gp, ctx, volume, true)
    }

    fn create(gp: GetProc, ctx: egui::Context, volume: f32, standby: bool) -> Result<Self, String> {
        let api = shared_api()?;
        unsafe {
            let h = (api.create)();
            if h.is_null() {
                return Err("mpv_create falló".into());
            }
            let vol = volume.to_string();
            let live_start = format!("live_start_index=-{}", crate::playback::LIVE_START_SEGMENTS);
            for (k, v) in [
                // escaladores bilineales, sin dither ni texturas intermedias de 16 bits: menos VRAM y GPU
                ("profile", "fast"),
                ("hwdec-extra-frames", "2"),
                ("demuxer-readahead-secs", "6"),
                // HLS: arranca cerca del borde y limita lecturas atascadas tras una discontinuidad.
                ("demuxer-lavf-o", live_start.as_str()),
                ("network-timeout", "10"),
                ("vo", "libmpv"),
                // Decodifica en GPU y copia el frame: sin esto mpv inicializa CUDA/Vulkan para interop (~1 GB comprometido)
                ("hwdec", if cfg!(windows) { "d3d11va-copy" } else { "videotoolbox-copy" }),
                ("gpu-hwdec-interop", "no"),
                ("vd-lavc-threads", "2"),
                ("ytdl", "no"),
                ("cache", "yes"),
                ("demuxer-max-bytes", "8MiB"),
                ("demuxer-max-back-bytes", "16MiB"),
                // HLS en vivo no es "seekable" para mpv; esto deja recorrer lo que ya está en caché
                ("demuxer-seekable-cache", "yes"),
                ("force-seekable", "yes"),
                ("audio-client-name", "Twitch Lite"),
                ("volume", vol.as_str()),
                ("mute", if standby { "yes" } else { "no" }),
            ] {
                (api.set_option)(h, cs(k).as_ptr(), cs(v).as_ptr());
            }
            if let Ok(log) = std::env::var("TWITCHLITE_MPV_LOG") {
                let log = if standby { format!("{log}.standby") } else { log };
                (api.set_option)(h, cs("log-file").as_ptr(), cs(&log).as_ptr());
            }
            // Diagnóstico local opt-in para probar cortes con la DLL real, sin exponer IPC por defecto.
            let ipc_name = std::env::var("TWITCHLITE_MPV_IPC").ok().map(|pipe| {
                static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
                if standby { format!("{pipe}-standby-{}", NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)) } else { pipe }
            });
            if let Some(pipe) = &ipc_name {
                (api.set_option)(h, cs("input-ipc-server").as_ptr(), cs(&pipe).as_ptr());
            }
            if (api.initialize)(h) < 0 {
                (api.destroy)(h);
                return Err("mpv_initialize falló".into());
            }
            let gp = Box::new(gp);
            let ctx = Box::new(ctx);
            let mut init = GlInit { get_proc_address: get_proc, ctx: &*gp as *const GetProc as *mut c_void };
            let api_type = cs("opengl");
            let mut params = [
                Param { kind: 1, data: api_type.as_ptr() as *mut c_void }, // MPV_RENDER_PARAM_API_TYPE
                Param { kind: 2, data: &mut init as *mut GlInit as *mut c_void }, // MPV_RENDER_PARAM_OPENGL_INIT_PARAMS
                Param { kind: 0, data: std::ptr::null_mut() },
            ];
            let mut rc: H = std::ptr::null_mut();
            if (api.rc_create)(&mut rc, h, params.as_mut_ptr()) < 0 {
                (api.destroy)(h);
                return Err("mpv no pudo crear el contexto OpenGL".into());
            }
            (api.rc_callback)(rc, wake, &*ctx as *const egui::Context as *mut c_void);
            Ok(Self { api, h, rc, _gp: gp, _ctx: ctx, target: None, frame_serial: 0, ipc_name })
        }
    }

    fn cmd(&self, args: &[&str]) {
        let c: Vec<CString> = args.iter().map(|a| cs(a)).collect();
        let mut p: Vec<*const c_char> = c.iter().map(|a| a.as_ptr()).collect();
        p.push(std::ptr::null());
        unsafe { (self.api.command)(self.h, p.as_ptr()) };
    }
    pub fn set(&self, k: &str, v: &str) {
        unsafe { (self.api.set_property)(self.h, cs(k).as_ptr(), cs(v).as_ptr()) };
    }
    fn int(&self, k: &str) -> i64 {
        let mut v = 0i64;
        unsafe { (self.api.get_property)(self.h, cs(k).as_ptr(), 4, &mut v as *mut i64 as *mut c_void) }; // MPV_FORMAT_INT64
        v
    }
    /// Propiedad numérica (MPV_FORMAT_DOUBLE). None si mpv aún no la tiene.
    pub fn num(&self, k: &str) -> Option<f64> {
        let mut v = 0f64;
        let r = unsafe { (self.api.get_property)(self.h, cs(k).as_ptr(), 5, &mut v as *mut f64 as *mut c_void) };
        (r >= 0).then_some(v)
    }
    pub fn flag(&self, k: &str) -> bool {
        let mut v: c_int = 0;
        let r = unsafe { (self.api.get_property)(self.h, cs(k).as_ptr(), 3, &mut v as *mut c_int as *mut c_void) };
        r >= 0 && v != 0
    }
    /// Propiedad de texto (MPV_FORMAT_STRING); mpv la reserva y hay que liberarla con mpv_free.
    pub fn text(&self, k: &str) -> String {
        let mut p: *mut c_char = std::ptr::null_mut();
        unsafe {
            if (self.api.get_property)(self.h, cs(k).as_ptr(), 1, &mut p as *mut *mut c_char as *mut c_void) < 0 || p.is_null() {
                return String::new();
            }
            let s = CStr::from_ptr(p).to_string_lossy().into_owned();
            (self.api.free)(p as *mut c_void);
            s
        }
    }
    pub fn play(&self, url: &str) {
        self.cmd(&["loadfile", url]);
        self.set("pause", "no");
    }
    pub fn stop(&self) {
        self.cmd(&["stop"]);
    }
    pub fn toggle_pause(&self) {
        self.cmd(&["cycle", "pause"]);
    }
    pub fn frame_serial(&self) -> Option<f64> {
        (self.frame_serial > 0).then_some(self.frame_serial as f64)
    }
    pub fn ipc_name(&self) -> Option<&str> { self.ipc_name.as_deref() }
    /// Relación de aspecto del video actual (16:9 si aún no hay frame).
    pub fn aspect(&self) -> f32 {
        let (w, h) = (self.int("dwidth"), self.int("dheight"));
        if w > 0 && h > 0 { w as f32 / h as f32 } else { 16.0 / 9.0 }
    }

    /// Dibuja el frame actual en una textura de `px` pixeles y devuelve su id para egui.
    /// Reusa la misma textura GL al cambiar de tamaño, así el id registrado sigue valiendo.
    pub fn texture(&mut self, gl: &glow::Context, frame: &mut eframe::Frame, px: [i32; 2]) -> egui::TextureId {
        unsafe {
            let (fb, tex, id, size) = match self.target {
                Some(t) => t,
                None => {
                    let tex = gl.create_texture().expect("tex");
                    let fb = gl.create_framebuffer().expect("fbo");
                    let id = frame.register_native_glow_texture(tex);
                    (fb, tex, id, [0, 0])
                }
            };
            let resized = size != px;
            if resized {
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, px[0], px[1], 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
                gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
            }
            self.target = Some((fb, tex, id, px));
            let updated = (self.api.rc_update)(self.rc) & 1 != 0;
            if updated { self.frame_serial = self.frame_serial.wrapping_add(1); }
            if resized || updated {
                let mut f = Fbo { fbo: fb.0.get() as c_int, w: px[0], h: px[1], internal_format: 0 };
                let mut flip: c_int = 0;
                let mut params = [
                    Param { kind: 3, data: &mut f as *mut Fbo as *mut c_void }, // MPV_RENDER_PARAM_OPENGL_FBO
                    Param { kind: 4, data: &mut flip as *mut c_int as *mut c_void }, // MPV_RENDER_PARAM_FLIP_Y
                    Param { kind: 0, data: std::ptr::null_mut() },
                ];
                (self.api.rc_render)(self.rc, params.as_mut_ptr());
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            id
        }
    }

    /// Hay que llamarlo con el contexto GL activo (desde la UI o `on_exit`).
    pub fn destroy(mut self, gl: &glow::Context) {
        unsafe {
            (self.api.rc_free)(self.rc);
            if let Some((fb, tex, ..)) = self.target.take() {
                gl.delete_framebuffer(fb);
                gl.delete_texture(tex);
            }
        }
        // El cierre de una conexión HLS puede esperar al timeout de red. No bloquea la UI.
        let (api, handle) = (self.api.clone(), self.h as usize);
        std::thread::spawn(move || unsafe { (api.destroy)(handle as H); });
    }
}
