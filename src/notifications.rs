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
        // La URI cambia con la versión para evitar que Windows conserve el icono anterior.
        let path = dir.join(format!("twitchlite-icon-{}.png", crate::branding::VERSION));
        if std::fs::read(&path).ok().as_deref() != Some(crate::branding::ICON_PNG) {
            std::fs::write(&path, crate::branding::ICON_PNG).map_err(|e| e.to_string())?;
        }
        let (key, _) = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
            .create_subkey(format!(r"Software\Classes\AppUserModelId\{APP_ID}"))
            .map_err(|e| e.to_string())?;
        key.set_value("DisplayName", &crate::branding::NAME).map_err(|e| e.to_string())?;
        key.set_value("IconUri", &path.to_string_lossy().as_ref()).map_err(|e| e.to_string())?;
        key.set_value("IconBackgroundColor", &"FF9146FF").map_err(|e| e.to_string())?;
        Ok(path)
    }).clone()
}

pub fn init() {
    #[cfg(windows)]
    {
        #[link(name = "shell32")]
        unsafe extern "system" { fn SetCurrentProcessExplicitAppUserModelID(id: *const u16) -> i32; }
        let id: Vec<u16> = APP_ID.encode_utf16().chain(Some(0)).collect();
        let result = unsafe { SetCurrentProcessExplicitAppUserModelID(id.as_ptr()) };
        if result < 0 { crate::tlog(&format!("App identity: HRESULT {result:#x}")); }
        if let Err(e) = identity() { crate::tlog(&format!("Notification identity: {e}")); }
    }
}

pub fn show(title: &str, body: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        let icon = identity()?;
        winrt_notification::Toast::new(APP_ID).title(title).text1(body)
            .icon(&icon, winrt_notification::IconCrop::Square, crate::branding::NAME)
            .duration(winrt_notification::Duration::Short)
            .show().map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        notify_rust::Notification::new().appname(crate::branding::NAME).summary(title).body(body)
            .show().map(|_| ()).map_err(|e| e.to_string())
    }
}
