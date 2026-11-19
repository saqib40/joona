use objc2_app_kit::NSWorkspace;

use super::super::ForegroundApp;

pub fn foreground() -> Option<ForegroundApp> {
    let workspace = NSWorkspace::sharedWorkspace();
    let app = workspace.frontmostApplication()?;

    let pid = app.processIdentifier() as u32;
    let own = pid == std::process::id();

    let localized_name = app.localizedName().map(|s| s.to_string());
    let bundle_id = app.bundleIdentifier().map(|s| s.to_string());

    let title = localized_name
        .clone()
        .or_else(|| bundle_id.clone())
        .unwrap_or_default();

    let exe = app
        .executableURL()
        .and_then(|url| url.lastPathComponent().map(|s| s.to_string()))
        .or_else(|| bundle_id.clone())
        .unwrap_or_else(|| title.clone())
        .to_lowercase();

    let is_shell = matches!(
        bundle_id.as_deref(),
        Some("com.apple.dock" | "com.apple.finder")
    );
    let class = if is_shell {
        "Progman".into()
    } else {
        "macOS".into()
    };

    let hung = !app.isFinishedLaunching();

    Some(ForegroundApp {
        exe,
        title,
        pid,
        elevated: false,
        own,
        class,
        hung,
    })
}
