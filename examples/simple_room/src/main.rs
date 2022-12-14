use tracing_subscriber::prelude::*;
mod app;
mod events;
mod video_grid;
mod video_renderer;

fn main() {
    let fmt_layer = tracing_subscriber::fmt::Layer::default();

    tracing_subscriber::registry()
        .with(fmt_layer)
        .init();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    app::run(rt);
}
