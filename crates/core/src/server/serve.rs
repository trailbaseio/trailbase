//! Serve services.
//!
//! Ripped straight from axum::serve to add rustls support.

use axum::{body::Body, extract::Request, response::Response};
use futures_util::{FutureExt, pin_mut};
use hyper::body::Incoming;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::{server::conn::auto::Builder, service::TowerToHyperService};
use log::*;
use std::{
  convert::Infallible,
  fmt::Debug,
  future::{Future, IntoFuture, poll_fn},
  io,
  marker::PhantomData,
  net::SocketAddr,
  sync::Arc,
  task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio_rustls::TlsAcceptor;
use tower::ServiceExt as _;
use tower_service::Service;

/// Types that can listen for connections.
pub trait Listener: Send + 'static {
  /// The listener's IO type.
  type Io: AsyncRead + AsyncWrite + Unpin + Send + 'static;

  /// The listener's address type.
  type Addr: Send;

  /// Accept a new incoming connection to this listener.
  ///
  /// If the underlying accept call can return an error, this function must
  /// take care of logging and retrying.
  fn accept(
    &mut self,
  ) -> impl std::future::Future<Output = io::Result<(Self::Io, Self::Addr)>> + Send;

  /// Returns the local address that this listener is bound to.
  fn local_addr(&self) -> io::Result<Self::Addr>;
}

impl Listener for TcpListener {
  type Io = TcpStream;
  type Addr = std::net::SocketAddr;

  async fn accept(&mut self) -> io::Result<(Self::Io, Self::Addr)> {
    loop {
      match Self::accept(self).await {
        Ok(tup) => return Ok(tup),
        Err(e) => handle_accept_error(e).await,
      }
    }
  }

  #[inline]
  fn local_addr(&self) -> io::Result<Self::Addr> {
    Self::local_addr(self)
  }
}

async fn handle_accept_error(e: io::Error) {
  if is_connection_error(&e) {
    return;
  }

  // [From `hyper::Server` in 0.14](https://github.com/hyperium/hyper/blob/v0.14.27/src/server/tcp.rs#L186)
  //
  // > A possible scenario is that the process has hit the max open files
  // > allowed, and so trying to accept a new connection will fail with
  // > `EMFILE`. In some cases, it's preferable to just wait for some time, if
  // > the application will likely close some files (or connections), and try
  // > to accept the connection again. If this option is `true`, the error
  // > will be logged at the `error` level, since it is still a big deal,
  // > and then the listener will sleep for 1 second.
  //
  // hyper allowed customizing this but axum does not.
  warn!("accept error: {e}");
  tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
}

fn is_connection_error(e: &io::Error) -> bool {
  matches!(
    e.kind(),
    io::ErrorKind::ConnectionRefused
      | io::ErrorKind::ConnectionAborted
      | io::ErrorKind::ConnectionReset
  )
}

pub(crate) struct TlsListener {
  pub(crate) acceptor: TlsAcceptor,
  pub(crate) listener: TcpListener,
}

impl Listener for TlsListener {
  type Io = tokio_rustls::server::TlsStream<TcpStream>;
  type Addr = std::net::SocketAddr;

  async fn accept(&mut self) -> io::Result<(Self::Io, Self::Addr)> {
    loop {
      match self.listener.accept().await {
        Ok((stream, remote_addr)) => {
          return Ok((self.acceptor.accept(stream).await?, remote_addr));
        }
        Err(e) => handle_accept_error(e).await,
      }
    }
  }

  #[inline]
  fn local_addr(&self) -> io::Result<Self::Addr> {
    self.listener.local_addr()
  }
}

/// Serve the service with the supplied listener.
///
/// This method of running a service is intentionally simple and doesn't support any configuration.
/// Use hyper or hyper-util if you need configuration.
///
/// It supports both HTTP/1 as well as HTTP/2.
///
/// # Examples
///
/// Serving a [`Router`]:
///
/// ```
/// use axum::{Router, routing::get};
///
/// # async {
/// let router = Router::new().route("/", get(|| async { "Hello, World!" }));
///
/// let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
/// axum::serve(listener, router).await.unwrap();
/// # };
/// ```
///
/// See also [`Router::into_make_service_with_connect_info`].
///
/// Serving a [`MethodRouter`]:
///
/// ```
/// use axum::routing::get;
///
/// # async {
/// let router = get(|| async { "Hello, World!" });
///
/// let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
/// axum::serve(listener, router).await.unwrap();
/// # };
/// ```
///
/// See also [`MethodRouter::into_make_service_with_connect_info`].
///
/// Serving a [`Handler`]:
///
/// ```
/// use axum::handler::HandlerWithoutStateExt;
///
/// # async {
/// async fn handler() -> &'static str {
///     "Hello, World!"
/// }
///
/// let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
/// axum::serve(listener, handler.into_make_service()).await.unwrap();
/// # };
/// ```
///
/// See also [`HandlerWithoutStateExt::into_make_service_with_connect_info`] and
/// [`HandlerService::into_make_service_with_connect_info`].
///
/// [`Router`]: crate::Router
/// [`Router::into_make_service_with_connect_info`]: crate::Router::into_make_service_with_connect_info
/// [`MethodRouter`]: crate::routing::MethodRouter
/// [`MethodRouter::into_make_service_with_connect_info`]: crate::routing::MethodRouter::into_make_service_with_connect_info
/// [`Handler`]: crate::handler::Handler
/// [`HandlerWithoutStateExt::into_make_service_with_connect_info`]: crate::handler::HandlerWithoutStateExt::into_make_service_with_connect_info
/// [`HandlerService::into_make_service_with_connect_info`]: crate::handler::HandlerService::into_make_service_with_connect_info
pub fn serve<L, M, S>(listener: L, make_service: M) -> Serve<L, M, S>
where
  L: Listener,
  M: for<'a> Service<IncomingStream<'a, L>, Error = Infallible, Response = S>,
  S: Service<Request, Response = Response, Error = Infallible> + Clone + Send + 'static,
  S::Future: Send,
{
  Serve {
    listener,
    make_service,
    _marker: PhantomData,
  }
}

/// Future returned by [`serve`].
#[must_use = "futures must be awaited or polled"]
pub struct Serve<L, M, S> {
  listener: L,
  make_service: M,
  _marker: PhantomData<S>,
}

impl<L, M, S> Serve<L, M, S>
where
  L: Listener,
{
  /// Prepares a server to handle graceful shutdown when the provided future completes.
  ///
  /// # Example
  ///
  /// ```
  /// use axum::{Router, routing::get};
  ///
  /// # async {
  /// let router = Router::new().route("/", get(|| async { "Hello, World!" }));
  ///
  /// let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
  /// axum::serve(listener, router)
  ///     .with_graceful_shutdown(shutdown_signal())
  ///     .await
  ///     .unwrap();
  /// # };
  ///
  /// async fn shutdown_signal() {
  ///     // ...
  /// }
  /// ```
  pub fn with_graceful_shutdown<F>(self, signal: F) -> WithGracefulShutdown<L, M, S, F>
  where
    F: Future<Output = ()> + Send + 'static,
  {
    WithGracefulShutdown {
      listener: self.listener,
      make_service: self.make_service,
      signal,
      _marker: PhantomData,
    }
  }

  /// Returns the local address this server is bound to.
  #[allow(unused)]
  pub fn local_addr(&self) -> io::Result<L::Addr> {
    self.listener.local_addr()
  }
}

impl<L, M, S> Debug for Serve<L, M, S>
where
  L: Debug + 'static,
  M: Debug,
{
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let Self {
      listener,
      make_service,
      _marker: _,
    } = self;

    let mut s = f.debug_struct("Serve");
    s.field("listener", listener)
      .field("make_service", make_service);

    s.finish()
  }
}

impl<L, M, S> IntoFuture for Serve<L, M, S>
where
  L: Listener,
  L::Addr: Debug,
  M: for<'a> Service<IncomingStream<'a, L>, Error = Infallible, Response = S> + Send + 'static,
  for<'a> <M as Service<IncomingStream<'a, L>>>::Future: Send,
  S: Service<Request, Response = Response, Error = Infallible> + Clone + Send + 'static,
  S::Future: Send,
{
  type Output = io::Result<()>;
  type IntoFuture = private::ServeFuture;

  fn into_future(self) -> Self::IntoFuture {
    self
      .with_graceful_shutdown(std::future::pending())
      .into_future()
  }
}

impl<L> Service<IncomingStream<'_, L>> for axum::Router<()>
where
  L: Listener,
{
  type Response = Self;
  type Error = Infallible;
  type Future = std::future::Ready<Result<Self::Response, Self::Error>>;

  fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
    Poll::Ready(Ok(()))
  }

  fn call(&mut self, _req: IncomingStream<'_, L>) -> Self::Future {
    // call `Router::with_state` such that everything is turned into `Route` eagerly
    // rather than doing that per request
    std::future::ready(Ok(self.clone().with_state(())))
  }
}

impl axum::extract::connect_info::Connected<IncomingStream<'_, TcpListener>> for SocketAddr {
  fn connect_info(stream: IncomingStream<'_, TcpListener>) -> Self {
    *stream.remote_addr()
  }
}

/// Serve future with graceful shutdown enabled.
#[must_use = "futures must be awaited or polled"]
pub struct WithGracefulShutdown<L, M, S, F> {
  listener: L,
  make_service: M,
  signal: F,
  _marker: PhantomData<S>,
}

impl<L, M, S, F> WithGracefulShutdown<L, M, S, F>
where
  L: Listener,
{
  /// Returns the local address this server is bound to.
  #[allow(unused)]
  pub fn local_addr(&self) -> io::Result<L::Addr> {
    self.listener.local_addr()
  }
}

impl<L, M, S, F> Debug for WithGracefulShutdown<L, M, S, F>
where
  L: Debug + 'static,
  M: Debug,
  S: Debug,
  F: Debug,
{
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let Self {
      listener,
      make_service,
      signal,
      _marker: _,
    } = self;

    f.debug_struct("WithGracefulShutdown")
      .field("listener", listener)
      .field("make_service", make_service)
      .field("signal", signal)
      .finish()
  }
}

impl<L, M, S, F> IntoFuture for WithGracefulShutdown<L, M, S, F>
where
  L: Listener,
  L::Addr: Debug,
  M: for<'a> Service<IncomingStream<'a, L>, Error = Infallible, Response = S> + Send + 'static,
  for<'a> <M as Service<IncomingStream<'a, L>>>::Future: Send,
  S: Service<Request, Response = Response, Error = Infallible> + Clone + Send + 'static,
  S::Future: Send,
  F: Future<Output = ()> + Send + 'static,
{
  type Output = io::Result<()>;
  type IntoFuture = private::ServeFuture;

  fn into_future(self) -> Self::IntoFuture {
    let Self {
      mut listener,
      mut make_service,
      signal,
      _marker: _,
    } = self;

    private::ServeFuture(Box::pin(async move {
      let (signal_tx, signal_rx) = watch::channel(());
      let signal_tx = Arc::new(signal_tx);
      tokio::spawn(async move {
        signal.await;
        trace!("received graceful shutdown signal. Telling tasks to shutdown");
        drop(signal_rx);
      });

      let (close_tx, close_rx) = watch::channel(());

      loop {
        let (io, remote_addr) = tokio::select! {
            tuple_or = listener.accept() => {
            let Ok(tuple) = tuple_or else {
              continue;
            };
            tuple
          },
            _ = signal_tx.closed() => {
                trace!("signal received, not accepting new connections");
                break;
            }
        };

        let io = TokioIo::new(io);

        trace!("connection {remote_addr:?} accepted");

        poll_fn(|cx| make_service.poll_ready(cx))
          .await
          .unwrap_or_else(|err| match err {});

        let tower_service = make_service
          .call(IncomingStream {
            io: &io,
            remote_addr,
          })
          .await
          .unwrap_or_else(|err| match err {})
          .map_request(|req: Request<Incoming>| req.map(Body::new));

        let hyper_service = TowerToHyperService::new(tower_service);

        let signal_tx = Arc::clone(&signal_tx);

        let close_rx = close_rx.clone();

        tokio::spawn(async move {
          #[allow(unused_mut)]
          let mut builder = Builder::new(TokioExecutor::new());
          // CONNECT protocol needed for HTTP/2 websockets
          builder.http2().enable_connect_protocol();
          let conn = builder.serve_connection_with_upgrades(io, hyper_service);
          pin_mut!(conn);

          let signal_closed = signal_tx.closed().fuse();
          pin_mut!(signal_closed);

          loop {
            tokio::select! {
                result = conn.as_mut() => {
                    if let Err(_err) = result {
                        trace!("failed to serve connection: {_err:#}");
                    }
                    break;
                }
                _ = &mut signal_closed => {
                    trace!("signal received in task, starting graceful shutdown");
                    conn.as_mut().graceful_shutdown();
                }
            }
          }

          drop(close_rx);
        });
      }

      drop(close_rx);
      drop(listener);

      trace!(
        "waiting for {} task(s) to finish",
        close_tx.receiver_count()
      );
      close_tx.closed().await;

      Ok(())
    }))
  }
}

/// An incoming stream.
///
/// Used with [`serve`] and [`IntoMakeServiceWithConnectInfo`].
///
/// [`IntoMakeServiceWithConnectInfo`]: crate::extract::connect_info::IntoMakeServiceWithConnectInfo
#[derive(Debug)]
pub struct IncomingStream<'a, L>
where
  L: Listener,
{
  io: &'a TokioIo<L::Io>,
  remote_addr: L::Addr,
}

impl<L> IncomingStream<'_, L>
where
  L: Listener,
{
  /// Get a reference to the inner IO type.
  pub fn io(&self) -> &L::Io {
    self.io.inner()
  }

  /// Returns the remote address that this stream is bound to.
  pub fn remote_addr(&self) -> &L::Addr {
    &self.remote_addr
  }
}

mod private {
  use std::{
    future::Future,
    io,
    pin::Pin,
    task::{Context, Poll},
  };

  pub struct ServeFuture(pub(super) futures_util::future::BoxFuture<'static, io::Result<()>>);

  impl Future for ServeFuture {
    type Output = io::Result<()>;

    #[inline]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
      self.0.as_mut().poll(cx)
    }
  }

  impl std::fmt::Debug for ServeFuture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
      f.debug_struct("ServeFuture").finish_non_exhaustive()
    }
  }
}

#[cfg(test)]
mod tests {
  use std::{
    future::{IntoFuture as _, pending},
    net::{IpAddr, Ipv4Addr},
  };

  use axum::http::StatusCode;
  use axum::{
    Router,
    body::{Body, to_bytes},
    extract::Request,
    routing::get,
  };
  use hyper_util::rt::TokioIo;
  use tokio::{
    io::{self, AsyncRead, AsyncWrite},
    net::TcpListener,
  };

  use super::{Listener, serve};

  #[allow(dead_code, unused_must_use)]
  async fn if_it_compiles_it_works() {
    #[derive(Clone, Debug)]
    struct UdsConnectInfo;

    let router: Router = Router::new();

    let addr = "0.0.0.0:0";

    // router
    serve(TcpListener::bind(addr).await.unwrap(), router.clone());

    serve(
      TcpListener::bind(addr).await.unwrap(),
      router.clone().into_make_service(),
    );

    serve(
      TcpListener::bind(addr).await.unwrap(),
      router
        .clone()
        .into_make_service_with_connect_info::<std::net::SocketAddr>(),
    );
  }

  #[tokio::test]
  async fn test_serve_local_addr() {
    let router: Router = Router::new();
    let addr = "0.0.0.0:0";

    let server = serve(TcpListener::bind(addr).await.unwrap(), router.clone());
    let address = server.local_addr().unwrap();

    assert_eq!(address.ip(), IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)));
    assert_ne!(address.port(), 0);
  }

  #[tokio::test]
  async fn test_with_graceful_shutdown_local_addr() {
    let router: Router = Router::new();
    let addr = "0.0.0.0:0";

    let server = serve(TcpListener::bind(addr).await.unwrap(), router.clone())
      .with_graceful_shutdown(pending());
    let address = server.local_addr().unwrap();

    assert_eq!(address.ip(), IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)));
    assert_ne!(address.port(), 0);
  }

  #[test]
  fn into_future_outside_tokio() {
    let router: Router = Router::new();
    let addr = "0.0.0.0:0";

    let rt = tokio::runtime::Builder::new_multi_thread()
      .enable_all()
      .build()
      .unwrap();

    let listener = rt.block_on(tokio::net::TcpListener::bind(addr)).unwrap();

    // Call Serve::into_future outside of a tokio context. This used to panic.
    _ = serve(listener, router).into_future();
  }

  #[tokio::test]
  async fn serving_on_custom_io_type() {
    struct ReadyListener<T>(Option<T>);

    impl<T> Listener for ReadyListener<T>
    where
      T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
      type Io = T;
      type Addr = ();

      async fn accept(&mut self) -> io::Result<(Self::Io, Self::Addr)> {
        match self.0.take() {
          Some(server) => Ok((server, ())),
          None => std::future::pending().await,
        }
      }

      fn local_addr(&self) -> io::Result<Self::Addr> {
        Ok(())
      }
    }

    let (client, server) = io::duplex(1024);
    let listener = ReadyListener(Some(server));

    let app = Router::new().route("/", get(|| async { "Hello, World!" }));

    tokio::spawn(serve(listener, app).into_future());

    let stream = TokioIo::new(client);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(stream).await.unwrap();
    tokio::spawn(conn);

    let request = Request::builder().body(Body::empty()).unwrap();

    let response = sender.send_request(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = Body::new(response.into_body());
    let body = to_bytes(body, usize::MAX).await.unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert_eq!(body, "Hello, World!");
  }
}
