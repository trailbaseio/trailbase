use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, hash_map::Entry};
use std::sync::atomic::Ordering;
use std::sync::{Arc, LazyLock};
use trailbase_qs::ValueOrComposite;
use trailbase_reactive::Reactive;

use crate::auth::User;
use crate::records::RecordApi;
use crate::records::RecordError;
use crate::records::subscribe::event::{EventPayload, JsonEventPayload};
use crate::records::subscribe::state::{
  AutoCleanupEventStream, EventCandidate, PerConnectionState, PerConnectionStateInternal,
  Subscription,
};

/// Internal, shareable state of the cloneable SubscriptionManager.
struct ManagerState {
  /// Record API configurations.
  record_apis: Reactive<Vec<RecordApi>>,

  /// Manages subscriptions for different connections based on `conn.id()`.
  connections: RwLock<HashMap</* conn id= */ usize, Arc<PerConnectionState>>>,
}

#[derive(Clone)]
pub struct SubscriptionManager {
  state: Arc<ManagerState>,
}

impl SubscriptionManager {
  pub fn new(record_apis: Reactive<HashMap<String, RecordApi>>) -> Self {
    let record_apis = record_apis.derive_unchecked(|apis| apis.values().cloned().collect());
    let state = Arc::new(ManagerState {
      record_apis: record_apis.clone(),
      connections: RwLock::new(HashMap::new()),
    });

    {
      let state = state.clone();
      record_apis.add_observer(move |record_apis| {
        let mut lock = state.connections.write();

        let mut old: HashMap<usize, Arc<PerConnectionState>> = std::mem::take(&mut lock);

        for api in record_apis.iter() {
          if !api.enable_subscriptions() {
            continue;
          }

          let id = api.conn().id();

          // TODO: Skip/cleanup subscriptions from existing entries for tables that not longer have
          // a corresponding API.
          if let Some(existing) = old.remove(&id) {
            let apis = filter_record_apis(id, record_apis);
            let Some(first) = apis.values().nth(0) else {
              continue;
            };

            {
              let mut state = existing.state.lock();
              // Update metadata and add back.
              state.connection_metadata = first.connection_metadata().clone();
              state.record_apis = apis;
            }

            lock.insert(id, existing);
          }
        }
      });
    }

    return Self { state };
  }

  pub async fn add_sse_table_subscription(
    &self,
    api: RecordApi,
    user: Option<User>,
    filter: Option<ValueOrComposite>,
  ) -> Result<(AutoCleanupEventStream, Arc<Subscription>), RecordError> {
    let (sender, receiver) = async_channel::bounded::<EventCandidate>(64);
    let state = self.get_per_connection_state(&api);

    let subscription = state
      .clone()
      .add_table_subscription(api, user, filter, sender.clone())
      .await?;

    // Send an immediate comment to flush SSE headers and establish the connection
    if sender
      .send(EventCandidate {
        record: None,
        payload: ESTABLISHED_EVENT.clone(),
        seq: subscription.candidate_seq.fetch_add(1, Ordering::SeqCst),
      })
      .await
      .is_err()
    {
      return Err(RecordError::BadRequest("channel already closed"));
    }

    return Ok((
      AutoCleanupEventStream::new(receiver, state, subscription.id.clone()),
      subscription,
    ));
  }

  pub async fn add_sse_record_subscription(
    &self,
    api: RecordApi,
    record: trailbase_sqlite::Value,
    user: Option<User>,
  ) -> Result<(AutoCleanupEventStream, Arc<Subscription>), RecordError> {
    let (sender, receiver) = async_channel::bounded::<EventCandidate>(64);
    let state = self.get_per_connection_state(&api);

    let subscription = state
      .clone()
      .add_record_subscription(api, record, user, sender.clone())
      .await?;

    // Send an immediate comment to flush SSE headers and establish the connection
    if sender
      .send(EventCandidate {
        record: None,
        payload: ESTABLISHED_EVENT.clone(),
        seq: subscription.candidate_seq.fetch_add(1, Ordering::SeqCst),
      })
      .await
      .is_err()
    {
      return Err(RecordError::BadRequest("channel already closed"));
    }

    return Ok((
      AutoCleanupEventStream::new(receiver, state, subscription.id.clone()),
      subscription,
    ));
  }

  pub fn get_per_connection_state(&self, api: &RecordApi) -> Arc<PerConnectionState> {
    let id: usize = api.conn().id();
    let mut lock = self.state.connections.upgradable_read();
    if let Some(state) = lock.get(&id) {
      return state.clone();
    }

    return lock.with_upgraded(|m| {
      return match m.entry(id) {
        Entry::Occupied(v) => v.get().clone(),
        Entry::Vacant(v) => {
          let state = Arc::new(PerConnectionState {
            state: Mutex::new(PerConnectionStateInternal {
              connection_metadata: api.connection_metadata().clone(),
              record_apis: filter_record_apis(id, &self.state.record_apis.value()),
              conn: api.conn().clone(),
              subscriptions: Default::default(),
            }),
          });
          v.insert(state).clone()
        }
      };
    });
  }

  #[cfg(test)]
  pub fn num_record_subscriptions(&self) -> usize {
    let mut count: usize = 0;
    for state in self.state.connections.read().values() {
      for (_table_name, subs) in state.state.lock().subscriptions.iter() {
        for record in subs.record.values() {
          count += record.len();
        }
      }
    }
    return count;
  }

  #[cfg(test)]
  pub fn num_table_subscriptions(&self) -> usize {
    let mut count: usize = 0;
    for state in self.state.connections.read().values() {
      for (_table_name, subs) in state.state.lock().subscriptions.iter() {
        count += subs.table.len();
      }
    }
    return count;
  }
}

fn filter_record_apis(conn_id: usize, record_apis: &[RecordApi]) -> HashMap<String, RecordApi> {
  return record_apis
    .iter()
    .flat_map(|api| {
      if !api.enable_subscriptions() {
        return None;
      }
      if api.conn().id() == conn_id {
        return Some((api.api_name().to_string(), api.clone()));
      }

      return None;
    })
    .collect();
}

static ESTABLISHED_EVENT: LazyLock<Arc<EventPayload>> =
  LazyLock::new(|| Arc::new(EventPayload::from(&JsonEventPayload::Ping)));

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn static_establish_event_test() {
    let _y: Arc<EventPayload> = (*ESTABLISHED_EVENT).clone();
  }
}
