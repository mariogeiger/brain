//! The brain viewer's server: it serves the built page, `client/dist`, with
//! gzip. It serves the listening socket systemd passes it (socket
//! activation), else `127.0.0.1:4006`.

use axum::Router;
use std::net::SocketAddr;
use std::os::fd::{FromRawFd, OwnedFd};
use std::os::unix::net::UnixListener;
use tower_http::compression::CompressionLayer;
use tower_http::services::ServeDir;

const PORT: u16 = 4006;

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Gzip-compress on the wire: the .nii scan squashes well. The layer
    // comes after the service, as it wraps only what the router holds.
    let app = Router::new()
        .fallback_service(ServeDir::new("client/dist"))
        .layer(CompressionLayer::new().gzip(true));

    match inherited_listener() {
        Some(socket) => {
            socket.set_nonblocking(true).expect("the socket is usable");
            let listener =
                tokio::net::UnixListener::from_std(socket).expect("the socket is usable");
            log::info!("brain viewer on the socket systemd passed");
            axum::serve(listener, app)
                .with_graceful_shutdown(stopped())
                .await
        }
        None => {
            let addr = SocketAddr::from(([127, 0, 0, 1], PORT));
            let listener = tokio::net::TcpListener::bind(addr)
                .await
                .expect("the port is free");
            log::info!("brain viewer listening on http://{addr}");
            axum::serve(listener, app)
                .with_graceful_shutdown(stopped())
                .await
        }
    }
    .expect("the server runs");
}

/// The one listening socket systemd passes this process, by the protocol of
/// `sd_listen_fds`: `LISTEN_PID` names it, `LISTEN_FDS` counts the sockets,
/// which begin at descriptor 3. It comes back as a duplicate that closes on
/// exec, and descriptor 3 is closed, so no child could inherit the listener.
fn inherited_listener() -> Option<UnixListener> {
    let ours = std::env::var("LISTEN_PID").ok()?.parse::<u32>().ok()? == std::process::id();
    let one = std::env::var("LISTEN_FDS").ok()? == "1";
    if !(ours && one) {
        return None;
    }
    // SAFETY: by the protocol, descriptor 3 is a listening socket this process owns.
    let passed = unsafe { OwnedFd::from_raw_fd(3) };
    Some(UnixListener::from(
        passed.try_clone().expect("the socket can be duplicated"),
    ))
}

/// Resolves when the process is asked to stop: SIGINT, or SIGTERM, which
/// systemd and Podman send. In a container the server is PID 1, for which
/// the kernel drops a SIGTERM it does not handle.
async fn stopped() {
    use tokio::signal::unix::{SignalKind, signal};
    let mut term = signal(SignalKind::terminate()).expect("SIGTERM can be awaited");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
}
