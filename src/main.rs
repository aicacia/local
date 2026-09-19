#[tokio::main]
async fn main() -> std::io::Result<()> {
    idp_server::run().await
}
