pub const NAME: &str = "Dropflip";

pub fn window_icon() -> eframe::egui::IconData {
    let image = image::load_from_memory(include_bytes!("../assets/dropflip.png"))
        .expect("Bundled application icon is valid")
        .into_rgba8();
    eframe::egui::IconData {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    }
}

pub fn tray_icon() -> anyhow::Result<tray_icon::Icon> {
    let image =
        image::load_from_memory(include_bytes!("../assets/dropflip-tray.png"))?.into_rgba8();
    let (width, height) = image.dimensions();
    Ok(tray_icon::Icon::from_rgba(image.into_raw(), width, height)?)
}
