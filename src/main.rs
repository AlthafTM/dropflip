#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app;
mod branding;
mod engine;
mod platform;
mod settings;
mod sheets;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let engines = engine::Engines::discover();
    if args.first().is_some_and(|v| v == "--engines") {
        println!("{}", serde_json::to_string_pretty(&engines)?);
        return Ok(());
    }
    if args.first().is_some_and(|v| v == "--inspect") {
        let file = args
            .get(1)
            .ok_or_else(|| anyhow::anyhow!("Expected an input path"))?;
        println!(
            "{}",
            serde_json::to_string_pretty(&engine::inspect(std::path::Path::new(file), &engines)?)?
        );
        return Ok(());
    }
    if args.first().is_some_and(|v| v == "--convert") {
        return engine::cli(&args[1..], &engines);
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(branding::NAME)
            .with_icon(branding::window_icon())
            .with_inner_size([180.0, 110.0])
            .with_mouse_passthrough(true)
            .with_active(false)
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_taskbar(false)
            .with_drag_and_drop(true),
        renderer: eframe::Renderer::Glow,
        vsync: true,
        multisampling: 0,
        ..Default::default()
    };
    eframe::run_native(
        branding::NAME,
        options,
        Box::new(move |cc| Ok(Box::new(app::Dropflip::new(cc, engines, args)))),
    )
    .map_err(|e| anyhow::anyhow!("Could not start the desktop window: {e}"))
}
