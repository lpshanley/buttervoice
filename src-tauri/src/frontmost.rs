//! The app in front, which is the one a dictation was just typed into.

use buttervoice_history::App;
use objc2_app_kit::NSWorkspace;

pub fn app() -> Option<App> {
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    Some(App {
        name: app.localizedName()?.to_string(),
        bundle_id: app.bundleIdentifier().map(|id| id.to_string()),
    })
}
