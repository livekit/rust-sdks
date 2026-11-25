/// Runs the combined vector video example.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    vector_video::run().await
}
