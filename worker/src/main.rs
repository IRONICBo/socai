#[tokio::main]
async fn main() -> anyhow::Result<()> {
    socai_worker::run_stdio().await
}
