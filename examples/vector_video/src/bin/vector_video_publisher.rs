/// Starts the vector video publisher.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    vector_video::run_publisher().await
}
