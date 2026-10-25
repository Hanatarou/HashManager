// A window program on Windows: no console.
#![cfg_attr(windows, windows_subsystem = "windows")]

use eframe::egui;
use hashcore::consts::APP_NAME;
use hashcore::ui::App;

fn main() -> eframe::Result {
    hashcore::applog::init();
    // A panic is written to the log first. The released program (profile `small`) then stops, so nothing half-done goes on; the
    // other profiles unwind, and a job that panicked ends with an error message.
    std::panic::set_hook(Box::new(|info| hashcore::applog::error(&format!("Panic: {info}"))));
    let s = hashcore::settings::load();
    hashcore::applog::startup_banner(&format!("Settings: {}", hashcore::settings::serialize(&s).lines().collect::<Vec<_>>().join(" ")));
    let icon = image::load_from_memory(include_bytes!("../../resources/icon.png")).ok().map(|i| {
        let rgba = i.to_rgba8();
        egui::IconData { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() }
    });
    // Where the window opens: as it was, when that fits on the monitor; otherwise 60% of the work area, centered.
    // (Windows knows the monitor now; elsewhere the first run is corrected once the window exists.)
    let saved = s.window.filter(|g| g.sane());
    let saved_rect = saved.map(|g| (g.x, g.y, g.w as i32, g.h as i32));
    let fitted = hashcore::winfit::work_area(saved_rect).map(|work| hashcore::winfit::fit_window(saved_rect, work, 16));
    let mut vp = egui::ViewportBuilder::default().with_inner_size([900.0, 640.0]).with_min_inner_size([640.0, 420.0]).with_drag_and_drop(true).with_title(APP_NAME);
    if let Some((x, y, w, h)) = fitted.or(saved_rect) {
        vp = vp.with_inner_size([w as f32, h as f32]).with_position([x as f32, y as f32]);
    }
    if saved.is_some_and(|g| g.maximized) { vp = vp.with_maximized(true); }
    if s.on_top { vp = vp.with_always_on_top(); }
    if let Some(i) = icon { vp = vp.with_icon(std::sync::Arc::new(i)); }
    eframe::run_native(APP_NAME, eframe::NativeOptions { viewport: vp, centered: saved.is_none() && fitted.is_none(), ..Default::default() }, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
