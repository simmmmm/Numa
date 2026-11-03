use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::connect_info::ConnectInfo;
use axum::{Extension, Router};
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::server::graceful::GracefulShutdown;
use hyper_util::service::TowerToHyperService;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;

pub const HEAD_TIME: Duration = Duration::from_secs(10);
pub const CONNECTIONS: usize = 512;

pub async fn serve(listener: TcpListener, app: Router, head_time: Duration, connections: usize, stop: impl std::future::Future<Output = ()>) {
    let room = Arc::new(Semaphore::new(connections));
    let graceful = GracefulShutdown::new();
    tokio::pin!(stop);
    loop {
        let permit = tokio::select! {
            permit = room.clone().acquire_owned() => permit.expect("the semaphore is never closed"),
            _ = &mut stop => break,
        };
        let (stream, peer) = tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok(pair) => pair,
                Err(_) => {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    continue;
                }
            },
            _ = &mut stop => break,
        };
        let service = TowerToHyperService::new(app.clone().layer(Extension(ConnectInfo::<SocketAddr>(peer))));
        let mut http = hyper::server::conn::http1::Builder::new();
        http.timer(TokioTimer::new()).header_read_timeout(head_time);
        let connection = graceful.watch(http.serve_connection(TokioIo::new(stream), service));
        tokio::spawn(async move {
            let _ = connection.await;
            drop(permit);
        });
    }
    graceful.shutdown().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn up(head_time: Duration, connections: usize) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new().route("/", axum::routing::get(|ConnectInfo(me): ConnectInfo<SocketAddr>| async move { me.ip().to_string() }));
        tokio::spawn(serve(listener, app, head_time, connections, std::future::pending()));
        addr
    }

    #[tokio::test]
    async fn a_half_sent_request_is_dropped() {
        let addr = up(Duration::from_millis(300), 8).await;
        let mut slow = tokio::net::TcpStream::connect(addr).await.unwrap();
        slow.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n").await.unwrap();
        let mut rest = Vec::new();
        let ended = tokio::time::timeout(Duration::from_secs(5), slow.read_to_end(&mut rest)).await;
        assert!(ended.is_ok(), "the server must close a connection whose head never ends");
        let mut fine = tokio::net::TcpStream::connect(addr).await.unwrap();
        fine.write_all(b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").await.unwrap();
        let mut text = String::new();
        fine.read_to_string(&mut text).await.unwrap();
        assert!(text.starts_with("HTTP/1.1 200") && text.ends_with("127.0.0.1"), "{text}");
    }

    #[tokio::test]
    async fn only_so_many_connections_are_served_at_once() {
        let addr = up(Duration::from_secs(30), 2).await;
        let mut held = Vec::new();
        for _ in 0..2 {
            held.push(tokio::net::TcpStream::connect(addr).await.unwrap());
        }
        let mut third = tokio::net::TcpStream::connect(addr).await.unwrap();
        third.write_all(b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").await.unwrap();
        let mut text = String::new();
        let waited = tokio::time::timeout(Duration::from_millis(400), third.read_to_string(&mut text)).await;
        assert!(waited.is_err(), "the third waits while two are open");
        drop(held);
        tokio::time::timeout(Duration::from_secs(5), third.read_to_string(&mut text)).await.unwrap().unwrap();
        assert!(text.starts_with("HTTP/1.1 200"), "{text}");
    }
}
