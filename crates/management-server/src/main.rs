#[tokio::main]
async fn main() -> std::io::Result<()> {
    management_server::run().await
}
