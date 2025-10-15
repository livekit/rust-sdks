use eframe::Renderer;
use egui::ViewportBuilder;
use parking_lot::deadlock;
use std::thread;
use std::time::Duration;

mod app;
mod logo_track;
mod service;
mod sine_track;
mod video_grid;
mod video_renderer;

fn main() {
    env_logger::init();

    #[cfg(feature = "tracing")]
    console_subscriber::init();

    // Create a background thread which checks for deadlocks every 10s
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(10));
        let deadlocks = deadlock::check_deadlock();
        if deadlocks.is_empty() {
            continue;
        }

        log::error!("{} deadlocks detected", deadlocks.len());
        for (i, threads) in deadlocks.iter().enumerate() {
            log::error!("Deadlock #{}", i);
            for t in threads {
                log::error!("Thread Id {:#?}: \n{:#?}", t.thread_id(), t.backtrace());
            }
        }
    });

    eframe::run_native(
        "LiveKit - Rust App",
        eframe::NativeOptions { centered: true, renderer: Renderer::Wgpu, ..Default::default() },
        Box::new(|cc| Ok(Box::new(app::LkApp::new(cc)))),
    )
    .unwrap();
}
