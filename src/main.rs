#[tokio::main]
async fn main() -> std::io::Result<()> {
    idp_unified::run().await
}
