// NOTE:: To fruther abstract away from reqwest we would need to switch to http::Request/Response
// types. We tried and paused it, since we failed to find an elegant streaming response type.
pub type HttpRequestType = reqwest::Request;
pub type HttpResponseType = reqwest::Response;
pub type HttpErrorType = reqwest::Error;

pub type HttpClient =
  tower::util::BoxCloneSyncService<HttpRequestType, HttpResponseType, HttpErrorType>;

#[derive(Debug, Clone)]
struct ReqwestClientService<S>(S);

impl<S> ReqwestClientService<S> {
  pub const fn new(inner: S) -> Self {
    Self(inner)
  }
}

impl<S> tower::Service<HttpRequestType> for ReqwestClientService<S>
where
  S: tower::Service<reqwest::Request>,
  S::Future: Send + 'static,
  S::Error: 'static,
  HttpResponseType: From<S::Response>,
  HttpErrorType: From<S::Error>,
{
  type Response = HttpResponseType;
  type Error = HttpErrorType;
  type Future = futures::future::BoxFuture<'static, Result<Self::Response, Self::Error>>;

  fn poll_ready(
    &mut self,
    _cx: &mut std::task::Context<'_>,
  ) -> std::task::Poll<Result<(), Self::Error>> {
    std::task::Poll::Ready(Ok(()))
  }

  fn call(&mut self, req: HttpRequestType) -> Self::Future {
    // let reqw = match reqwest::Request::try_from(req) {
    //   Ok(req) => req,
    //   Err(err) => {
    //     return Box::pin(std::future::ready(Err(err)));
    //   }
    // };
    let future = self.0.call(req);

    return Box::pin(async move {
      let respw = future.await?;
      return Ok(respw.into());
    });
  }
}

pub(crate) fn into_http_client(
  client: reqwest::Client,
) -> impl tower::Service<
  HttpRequestType,
  Response = HttpResponseType,
  Error = HttpErrorType,
  Future = impl Send,
> + Send
+ Clone {
  tower::ServiceBuilder::new()
    .layer(tower::layer::layer_fn(|service| {
      ReqwestClientService::new(service)
    }))
    .service(client)
}
