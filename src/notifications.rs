#[cfg(windows)]
const APP_ID: &str = "Romanovak.TwitchLite";

/// Identidad propia en el Centro de notificaciones; nunca usa el AUMID de PowerShell.
#[cfg(windows)]
fn identity() -> Result<std::path::PathBuf, String> {
    use std::sync::OnceLock;
    static ICON: OnceLock<Result<std::path::PathBuf, String>> = OnceLock::new();
    ICON.get_or_init(|| {
        let dir = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from)
            .ok_or("No se encontró LOCALAPPDATA")?.join("TwitchLite");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join("notification-icon.png");
        if !path.exists() {
            // Glifo de chat blanco sobre el morado de Twitch, legible al tamaño de un toast.
            let icon = image::RgbaImage::from_fn(96, 96, |x, y| {
                let (x, y) = (x as i32, y as i32);
                let outer = (20..=76).contains(&x) && (16..=62).contains(&y)
                    && (x <= 62 || y <= 76 - x + 48);
                let tail = (26..=43).contains(&x) && (62..=77).contains(&y) && y <= 103 - x;
                let inner = (29..=67).contains(&x) && (24..=54).contains(&y)
                    && (x <= 57 || y <= 91 - x);
                let eyes = ((40..=46).contains(&x) || (55..=61).contains(&x)) && (32..=46).contains(&y);
                image::Rgba(if ((outer || tail) && !inner) || eyes { [255, 255, 255, 255] } else { [145, 71, 255, 255] })
            });
            icon.save(&path).map_err(|e| e.to_string())?;
        }
        let (key, _) = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
            .create_subkey(format!(r"Software\Classes\AppUserModelId\{APP_ID}"))
            .map_err(|e| e.to_string())?;
        key.set_value("DisplayName", &"Twitch Lite").map_err(|e| e.to_string())?;
        key.set_value("IconUri", &path.to_string_lossy().as_ref()).map_err(|e| e.to_string())?;
        key.set_value("IconBackgroundColor", &"FF9147FF").map_err(|e| e.to_string())?;
        Ok(path)
    }).clone()
}

pub fn init() {
    #[cfg(windows)]
    {
        #[link(name = "shell32")]
        unsafe extern "system" { fn SetCurrentProcessExplicitAppUserModelID(id: *const u16) -> i32; }
        let id: Vec<u16> = APP_ID.encode_utf16().chain(Some(0)).collect();
        unsafe { SetCurrentProcessExplicitAppUserModelID(id.as_ptr()); }
        if let Err(e) = identity() { crate::tlog(&format!("Notification identity: {e}")); }
    }
}

pub fn show(title: &str, body: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        let icon = identity()?;
        winrt_notification::Toast::new(APP_ID).title(title).text1(body)
            .icon(&icon, winrt_notification::IconCrop::Square, "Twitch Lite")
            .duration(winrt_notification::Duration::Short)
            .show().map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        notify_rust::Notification::new().appname("Twitch Lite").summary(title).body(body)
            .show().map(|_| ()).map_err(|e| e.to_string())
    }
}
