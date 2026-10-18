//! Serves the library's watch pages and media without the app: `cargo run --example serve`.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let port = app_lib::server::start().await?;
    for v in app_lib::library::list() {
        println!("http://127.0.0.1:{port}/v/{}  {}", v.share_id, v.title);
    }
    tokio::signal::ctrl_c().await?;
    Ok(())
}
