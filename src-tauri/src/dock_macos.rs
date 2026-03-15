#[cfg(target_os = "macos")]
pub fn set_dock_icon_visible<R: tauri::Runtime>(app: &tauri::AppHandle<R>, visible: bool) {
    let policy = if visible {
        tauri::ActivationPolicy::Regular
    } else {
        tauri::ActivationPolicy::Accessory
    };

    let _ = app.set_activation_policy(policy);
    let _ = app.set_dock_visibility(visible);

    if visible {
        let _ = app.show();
    } else {
        let _ = app.hide();
    }
}

#[cfg(not(target_os = "macos"))]
pub fn set_dock_icon_visible<R: tauri::Runtime>(_app: &tauri::AppHandle<R>, _visible: bool) {}
