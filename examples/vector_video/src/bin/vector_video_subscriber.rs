/// Starts the vector video subscriber.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    vector_video::run_subscriber().await
}
