mod app;
mod events;
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
