mod app;
mod events;
mod logo_track;
mod sine_track;
mod video_grid;
mod video_renderer;

fn main() {
    tracing_subscriber::fmt::init();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    app::run(rt);
}
