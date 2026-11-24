use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::body::{Body, BodyDataStream, Bytes};
use axum::extract::connect_info::ConnectInfo;
use axum::extract::Request;
use axum::http::Method;
use axum::{BoxError, Extension, Router};
use futures_util::Stream;
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::server::graceful::GracefulShutdown;
use hyper_util::service::TowerToHyperService;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;

pub const HEAD_TIME: Duration = Duration::from_secs(10);
pub const CONNECTIONS: usize = 512;

const BODY_TIME: Duration = Duration::from_millis(if cfg!(test) { 300 } else { 30_000 });
const BODY_PACE: u64 = 16 * 1024;

const WRITE_TIME: Duration = Duration::from_millis(if cfg!(test) { 300 } else { 60_000 });

struct Paced {
    body: BodyDataStream,
    started: tokio::time::Instant,
    got: u64,
    late: Pin<Box<tokio::time::Sleep>>,
}

impl Stream for Paced {
    type Item = Result<Bytes, BoxError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.body).poll_next(cx) {
            Poll::Ready(Some(Ok(piece))) => {
                self.got += piece.len() as u64;
                let until = self.started + BODY_TIME + Duration::from_millis(self.got * 1000 / BODY_PACE);
                self.late.as_mut().reset(until);
                Poll::Ready(Some(Ok(piece)))
            }
            Poll::Ready(Some(Err(err))) => Poll::Ready(Some(Err(err.into()))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => match self.late.as_mut().poll(cx) {
                Poll::Ready(()) => Poll::Ready(Some(Err("the request's body came too slowly".into()))),
                Poll::Pending => Poll::Pending,
            },
        }
    }
}

async fn paced(request: Request) -> Request {
    if matches!(*request.method(), Method::GET | Method::HEAD) {
        return request;
    }
    request.map(|body| {
        let started = tokio::time::Instant::now();
        Body::from_stream(Paced { body: body.into_data_stream(), started, got: 0, late: Box::pin(tokio::time::sleep_until(started + BODY_TIME)) })
    })
}

struct Unstalled {
    socket: tokio::net::TcpStream,
    waiting: Option<Pin<Box<tokio::time::Sleep>>>,
}

impl Unstalled {
    fn timed<T>(&mut self, cx: &mut Context<'_>, polled: Poll<std::io::Result<T>>) -> Poll<std::io::Result<T>> {
        if polled.is_ready() {
            self.waiting = None;
            return polled;
        }
        let waiting = self.waiting.get_or_insert_with(|| Box::pin(tokio::time::sleep(WRITE_TIME)));
        match waiting.as_mut().poll(cx) {
            Poll::Ready(()) => Poll::Ready(Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "the answer was not read"))),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl tokio::io::AsyncRead for Unstalled {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut tokio::io::ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.socket).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for Unstalled {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        let polled = Pin::new(&mut self.socket).poll_write(cx, buf);
        self.timed(cx, polled)
    }

    fn poll_write_vectored(mut self: Pin<&mut Self>, cx: &mut Context<'_>, bufs: &[std::io::IoSlice<'_>]) -> Poll<std::io::Result<usize>> {
        let polled = Pin::new(&mut self.socket).poll_write_vectored(cx, bufs);
        self.timed(cx, polled)
    }

    fn is_write_vectored(&self) -> bool {
        self.socket.is_write_vectored()
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let polled = Pin::new(&mut self.socket).poll_flush(cx);
        self.timed(cx, polled)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.socket).poll_shutdown(cx)
    }
}

pub async fn serve(listener: TcpListener, app: Router, head_time: Duration, connections: usize, stop: impl std::future::Future<Output = ()>) {
    let app = app.layer(axum::middleware::map_request(paced));
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
        let connection = graceful.watch(http.serve_connection(TokioIo::new(Unstalled { socket: stream, waiting: None }), service));
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
        let app = Router::new()
            .route("/", axum::routing::get(|ConnectInfo(me): ConnectInfo<SocketAddr>| async move { me.ip().to_string() }).post(|body: Bytes| async move { body.len().to_string() }))
            .route("/big", axum::routing::get(|| async { vec![7u8; 64 << 20] }));
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

    #[tokio::test]
    async fn a_body_that_never_comes_is_dropped() {
        let addr = up(Duration::from_secs(30), 8).await;
        let mut slow = tokio::net::TcpStream::connect(addr).await.unwrap();
        slow.write_all(b"POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 10\r\n\r\n").await.unwrap();
        let mut rest = String::new();
        let ended = tokio::time::timeout(Duration::from_secs(5), slow.read_to_string(&mut rest)).await;
        assert!(ended.is_ok(), "the server must end a request whose body never comes");
        assert!(rest.starts_with("HTTP/1.1 4"), "{rest}");

        let mut steady = tokio::net::TcpStream::connect(addr).await.unwrap();
        steady.write_all(b"POST / HTTP/1.1\r\nHost: x\r\nConnection: close\r\nContent-Length: 40000\r\n\r\n").await.unwrap();
        for _ in 0..4 {
            steady.write_all(&[b'a'; 10_000]).await.unwrap();
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let mut text = String::new();
        steady.read_to_string(&mut text).await.unwrap();
        assert!(text.starts_with("HTTP/1.1 200") && text.ends_with("40000"), "a body that keeps coming has its time: {text}");
    }

    #[tokio::test]
    async fn an_answer_that_is_not_read_is_dropped() {
        let addr = up(Duration::from_secs(30), 8).await;
        let mut deaf = tokio::net::TcpStream::connect(addr).await.unwrap();
        deaf.write_all(b"GET /big HTTP/1.1\r\nHost: x\r\n\r\n").await.unwrap();
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let mut all = Vec::new();
        let ended = tokio::time::timeout(Duration::from_secs(10), deaf.read_to_end(&mut all)).await;
        assert!(ended.is_ok(), "the server must close a connection whose answer is not read");
        assert!(all.len() < 64 << 20, "it was cut short: {} bytes", all.len());
    }
}
