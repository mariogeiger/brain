use axum::Router;
use std::net::SocketAddr;
use tower_http::compression::CompressionLayer;
use tower_http::services::ServeDir;

const PORT: u16 = 4006;

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let app = Router::new()
        // Gzip-compress on the wire — the 49 MB .nii squashes to ~15 MB.
        .layer(CompressionLayer::new().gzip(true))
        .fallback_service(ServeDir::new("client/dist"));

    let addr = SocketAddr::from(([127, 0, 0, 1], PORT));
    log::info!("brain viewer listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
