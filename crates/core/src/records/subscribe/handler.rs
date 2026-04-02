use axum::extract::{Path, RawQuery, Request, State};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;
use futures_util::stream;
use serde::Deserialize;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, LazyLock};
use trailbase_qs::ValueOrComposite;
use ts_rs::TS;

use crate::app_state::AppState;
use crate::auth::User;
use crate::records::RecordApi;
use crate::records::filter::{Filter, apply_filter_recursively_to_record};
use crate::records::subscribe::event::{
  EventError, EventErrorStatus, EventPayload, JsonEventPayload,
};
use crate::records::subscribe::state::{EventCandidate, Subscription};
use crate::records::{Permission, RecordError};

#[derive(Clone, Default, Debug, PartialEq, Deserialize)]
pub struct SubscriptionQuery {
  /// Map from filter params to filter value. It's a vector in cases like:
  ///   `col0[$gte]=2&col0[$lte]=10`.
  pub filter: Option<ValueOrComposite>,

  /// Whether to use WebSocket instead of default SSE.
  pub ws: Option<bool>,
}

impl SubscriptionQuery {
  pub fn parse(query: &str) -> Result<SubscriptionQuery, RecordError> {
    // NOTE: We rely on form-encoding to properly parse ampersands, e.g.:
    // `filter[col0]=a&b%filter[col1]=c`.
    let qs = serde_qs::Config::new().max_depth(9).use_form_encoding(true);
    return qs
      .deserialize_bytes::<SubscriptionQuery>(query.as_bytes())
      .map_err(|_err| RecordError::BadRequest("Invalid query"));
  }
}

/// Read record.
#[utoipa::path(
  get,
  path = "/{name}/subscribe/{record}",
  tag = "records",
  // TODO: Document the params. Requires utoipa support in trailbase_qs or external impl.
  // params(SubscriptionParams),
  responses(
    (status = 200, description = "Starts streaming changes to matching records via SSE/WebSocket")
  )
)]
pub async fn add_subscription_sse_and_ws_handler(
  State(state): State<AppState>,
  Path((api_name, record)): Path<(String, String)>,
  user: Option<User>,
  RawQuery(raw_url_query): RawQuery,
  _request: Request,
) -> Result<Response, RecordError> {
  let Some(api) = state.lookup_record_api(&api_name) else {
    return Err(RecordError::ApiNotFound);
  };

  if !api.enable_subscriptions() {
    return Err(RecordError::Forbidden);
  }

  let SubscriptionQuery { filter, ws } = raw_url_query
    .as_ref()
    .map_or_else(
      || Ok(SubscriptionQuery::default()),
      |query| SubscriptionQuery::parse(query),
    )
    .map_err(|_err| {
      return RecordError::BadRequest("Invalid query");
    })?;

  return if ws.unwrap_or(false) {
    #[cfg(feature = "ws")]
    {
      subscribe_ws(state, api, record, filter, user, _request).await
    }

    #[cfg(not(feature = "ws"))]
    {
      Err(RecordError::BadRequest("ws unsupported"))
    }
  } else {
    subscribe_sse(state, api, record, filter, user).await
  };
}

struct ValidateEventArgs {
  state: AppState,
  // FIXME: We could probably do with a subset of information from the `Subscription` and keep it
  // internal.
  subscription: Arc<Subscription>,
  expected_candidate_seq: AtomicI64,
}

async fn validate_event(
  args: Arc<ValidateEventArgs>,
  ev: EventCandidate,
) -> Result<Option<Arc<EventPayload>>, RecordError> {
  if ev.seq != args.expected_candidate_seq.fetch_add(1, Ordering::SeqCst) {
    args.expected_candidate_seq.store(ev.seq, Ordering::SeqCst);
    return Ok(Some(EVENT_LOSS_EVENT.clone()));
  }

  let Some(ref record) = ev.record else {
    // Established events.
    return Ok(Some(ev.payload));
  };

  let sub: &Subscription = &args.subscription;
  if let Filter::Record(ref filter) = sub.filter
    && !apply_filter_recursively_to_record(filter, record)
  {
    return Ok(None);
  }

  // We don't memoize and eagerly look up the APIs to make sure we get an up-to-date
  // version.
  let Some(api) = args.state.lookup_record_api(&sub.record_api_name) else {
    return Ok(None);
  };

  api
    .check_record_level_read_access_for_subscriptions(api.conn(), record, sub.user.as_ref())
    .await?;

  return Ok(Some(ev.payload));
}

pub async fn subscribe_sse(
  state: AppState,
  api: RecordApi,
  record: String,
  filter: Option<ValueOrComposite>,
  user: Option<User>,
) -> Result<Response, RecordError> {
  let seq = Arc::new(AtomicI64::default());

  return match record.as_str() {
    "*" => {
      api.check_table_level_access(Permission::Read, user.as_ref())?;

      let (receiver, subscription) = state
        .subscription_manager()
        .add_sse_table_subscription(api, user, filter)
        .await?;

      let args = Arc::new(ValidateEventArgs {
        state,
        subscription,
        expected_candidate_seq: AtomicI64::default(),
      });

      Ok(
        Sse::new(receiver.filter_map(move |ev: EventCandidate| {
          let seq = seq.clone();
          let args = args.clone();

          return async move {
            validate_event(args.clone(), ev)
              .await
              .unwrap_or_default()
              .map(|ev| ev.into_sse_event(Some(seq.fetch_add(1, Ordering::SeqCst))))
          };
        }))
        .keep_alive(KeepAlive::default())
        .into_response(),
      )
    }
    _ => {
      let record_id = api.primary_key_to_value(record)?;
      api
        .check_record_level_access(Permission::Read, Some(&record_id), None, user.as_ref())
        .await?;

      let (receiver, subscription) = state
        .subscription_manager()
        .add_sse_record_subscription(api, record_id, user)
        .await?;

      let args = Arc::new(ValidateEventArgs {
        state,
        subscription,
        expected_candidate_seq: AtomicI64::default(),
      });

      Ok(
        Sse::new(
          receiver
            .then(move |ev: EventCandidate| {
              let seq = seq.clone();
              let args = args.clone();

              return async move {
                match validate_event(args.clone(), ev).await {
                  Ok(None) => stream::empty().boxed(),
                  Ok(Some(ev)) => stream::once(std::future::ready(
                    ev.into_sse_event(Some(seq.fetch_add(1, Ordering::SeqCst))),
                  ))
                  .boxed(),
                  Err(_) => {
                    // Death sentence for record subscriptions to not have access
                    stream::iter(vec![
                      // First send an error event to the user.
                      ACCESS_DENIED_EVENT
                        .clone()
                        .into_sse_event(Some(seq.fetch_add(1, Ordering::SeqCst))),
                      // Then terminate the stream via the `take_while` below.
                      Err(RecordError::Forbidden),
                    ])
                    .boxed()
                  }
                }
              };
            })
            .flatten()
            .take_while(|event: &Result<SseEvent, RecordError>| std::future::ready(event.is_ok())),
        )
        .keep_alive(KeepAlive::default())
        .into_response(),
      )
    }
  };
}

#[allow(unused)]
#[derive(Clone, Debug, Deserialize, TS)]
#[ts(export)]
enum WsProtocol {
  Init { auth_token: Option<String> },
}

#[cfg(feature = "ws")]
pub async fn subscribe_ws(
  state: AppState,
  api: RecordApi,
  record: String,
  filter: Option<ValueOrComposite>,
  mut user: Option<User>,
  request: Request,
) -> Result<Response, RecordError> {
  use axum::extract::FromRequestParts;
  use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
  use futures_util::SinkExt;
  use std::sync::Arc;

  use crate::records::subscribe::event::EventPayload;
  use crate::records::subscribe::state::AutoCleanupEventStream;
  use crate::records::subscribe::state::EventCandidate;

  let (mut parts, _body) = request.into_parts();
  let ws = match WebSocketUpgrade::from_request_parts(&mut parts, &state).await {
    Ok(ws) => ws,
    Err(err) => {
      return Ok(err.into_response());
    }
  };

  // https://www.rfc-editor.org/rfc/rfc6455.html#section-7.4.1
  //
  // "1011 indicates that a server is terminating the connection because it encountered an
  // unexpected condition that prevented it from fulfilling the request."
  //
  // "1008 indicates that an endpoint is terminating the connection because it has received a
  // message that violates its policy.  This is a generic status code that can be
  // returned when there is no other more suitable status code (e.g., 1003 or 1009) or
  // if there is a need to hide specific details about the policy."
  #[repr(u16)]
  enum Code {
    Policy = 1008,
    Unexpected = 1011,
  }

  async fn abort<S: SinkExt<Message> + std::marker::Unpin>(
    sender: &mut S,
    code: Code,
    reason: &str,
  ) {
    let _ = sender
      .send(Message::Close(Some(CloseFrame {
        code: code as u16,
        reason: reason.into(),
      })))
      .await;

    let _ = sender.close().await;
  }

  async fn broker<S: SinkExt<Message> + std::marker::Unpin>(
    state: AppState,
    subscription: Arc<Subscription>,
    // Receive events from SQLite
    receiver: AutoCleanupEventStream,
    // Send messages via WebSocket.
    sender: &mut S,
    is_record_subscription: bool,
  ) {
    let args = Arc::new(ValidateEventArgs {
      state,
      subscription,
      expected_candidate_seq: AtomicI64::default(),
    });

    let mut pinned_receiver = std::pin::pin!(receiver);
    while let Some(ev) = pinned_receiver.next().await {
      let payload = match validate_event(args.clone(), ev).await {
        Ok(Some(payload)) => payload,
        Ok(None) => {
          continue;
        }
        Err(_) => {
          if is_record_subscription {
            // Death sentence for record subscriptions to not have access
            let _ = ACCESS_DENIED_EVENT
              .clone()
              .into_ws_event()
              .map(|ev| sender.send(ev));
            return;
          } else {
            continue;
          }
        }
      };

      match payload.into_ws_event() {
        Ok(msg) => {
          if let Err(_value) = sender.send(msg).await {
            log::debug!("Sending WS event to client failed");

            abort(sender, Code::Unexpected, "Failed to send event").await;
            return;
          }
        }
        Err(err) => {
          debug_assert!(false, "into_ws_event failed: {err}");
        }
      };
    }
  }

  let init = async move |state: &AppState, socket: WebSocket, user: &mut Option<User>| {
    let (mut ws_sender, mut ws_receiver) = socket.split();

    if user.is_some()
      || parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .is_some()
    {
      return Some(ws_sender);
    }

    match tokio::time::timeout(tokio::time::Duration::from_secs(10), ws_receiver.next()).await {
      Ok(Some(Ok(Message::Text(json)))) => {
        let Ok(msg) = serde_json::from_str::<WsProtocol>(&json) else {
          abort(&mut ws_sender, Code::Policy, "unauthorized").await;
          return None;
        };

        match msg {
          WsProtocol::Init { auth_token } => {
            if let Some(auth_token) = auth_token {
              let Ok(claims) =
                crate::auth::AuthTokenClaims::from_auth_token(state.jwt(), &auth_token)
              else {
                abort(&mut ws_sender, Code::Policy, "unauthorized").await;
                return None;
              };

              if let Ok(u) = User::from_token_claims(claims) {
                let _ = user.insert(u);
              }
            }

            return Some(ws_sender);
          }
        }
      }
      _ => {
        abort(&mut ws_sender, Code::Unexpected, "unexpected message").await;
      }
    };

    return None;
  };

  // WebSocket uses the HTTP `UPGRADE` mechanism to switch over to dedicated, non-HTTP `ws://`
  // protocol.
  return match record.as_str() {
    "*" => {
      Ok(ws.on_upgrade(async move |socket: WebSocket| {
        use crate::records::subscribe::state::EventCandidate;

        let Some(mut ws_sender) = init(&state, socket, &mut user).await else {
          return;
        };

        // NOTE: Access checking can only happen post upgrade, since browsers & Node.js don't allow
        // setting custom headers for the UPGRADE HTTP request. We could maybe use cookies in some
        // places but instead expect an explicit authorization.
        if let Err(_err) = api.check_table_level_access(Permission::Read, user.as_ref()) {
          abort(&mut ws_sender, Code::Policy, "unauthorized").await;
          return;
        }

        let (sender, receiver) = async_channel::bounded::<EventCandidate>(64);
        let conn_state = state.subscription_manager().get_per_connection_state(&api);

        let Ok(subscription) = conn_state
          .clone()
          .add_table_subscription(api, user, filter, sender)
          .await
        else {
          abort(&mut ws_sender, Code::Unexpected, "subscription failed").await;
          return;
        };

        let receiver = AutoCleanupEventStream::new(receiver, conn_state, subscription.id.clone());

        broker(state, subscription, receiver, &mut ws_sender, false).await
      }))
    }
    _ => {
      let record_id = api.primary_key_to_value(record)?;

      Ok(ws.on_upgrade(async move |socket: WebSocket| {
        use crate::records::subscribe::state::EventCandidate;

        let Some(mut ws_sender) = init(&state, socket, &mut user).await else {
          return;
        };

        // NOTE: Access checking can only happen post upgrade, since browsers & Node.js don't allow
        // setting custom headers for the UPGRADE HTTP request. We could maybe use cookies in some
        // places but instead expect an explicit authorization.
        if let Err(_) = api
          .check_record_level_access(Permission::Read, Some(&record_id), None, user.as_ref())
          .await
        {
          abort(&mut ws_sender, Code::Policy, "unauthorized").await;
          return;
        }

        let (sender, receiver) = async_channel::bounded::<EventCandidate>(64);
        let conn_state = state.subscription_manager().get_per_connection_state(&api);

        let Ok(subscription) = conn_state
          .clone()
          .add_record_subscription(api, record_id, user, sender)
          .await
        else {
          abort(&mut ws_sender, Code::Unexpected, "subscription failed").await;
          return;
        };

        let receiver = AutoCleanupEventStream::new(receiver, conn_state, subscription.id.clone());

        broker(state, subscription, receiver, &mut ws_sender, true).await;
      }))
    }
  };
}

static ACCESS_DENIED_EVENT: LazyLock<Arc<EventPayload>> = LazyLock::new(|| {
  Arc::new(EventPayload::from(&JsonEventPayload::Error {
    value: EventError {
      status: EventErrorStatus::Forbidden,
      message: Some("Access denied".into()),
    },
  }))
});
static EVENT_LOSS_EVENT: LazyLock<Arc<EventPayload>> = LazyLock::new(|| {
  Arc::new(EventPayload::from(&JsonEventPayload::Error {
    value: EventError {
      status: EventErrorStatus::Loss,
      message: None,
    },
  }))
});

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn static_sse_event_test() {
    let _x: Arc<EventPayload> = (*ACCESS_DENIED_EVENT).clone();
  }
}
