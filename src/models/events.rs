//! Ephemeral owner-scoped notifications. Publishers never wait for consumers.
use dashmap::{DashMap, mapref::entry::Entry};
use std::sync::{
  Arc,
  atomic::{AtomicBool, Ordering},
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Debug)]
struct Subscriber {
  sender: mpsc::Sender<Arc<str>>,
  enabled: Arc<AtomicBool>,
  slow: CancellationToken,
}

#[derive(Debug, Default)]
pub struct EventHub {
  users: DashMap<u64, std::collections::HashMap<Uuid, Subscriber>>,
}

pub struct Connection {
  hub: Arc<EventHub>,
  user_id: u64,
  pub id: Uuid,
  pub enabled: Arc<AtomicBool>,
  pub slow: CancellationToken,
  pub receiver: mpsc::Receiver<Arc<str>>,
}

impl EventHub {
  /// Two concurrent sockets per owner, across both WebSocket endpoints.
  pub fn connect(self: &Arc<Self>, user_id: u64, capacity: usize) -> Option<Connection> {
    let mut entries = self.users.entry(user_id).or_default();
    if entries.len() >= 2 {
      return None;
    }
    let id = Uuid::now_v7();
    let (sender, receiver) = mpsc::channel(capacity);
    let enabled = Arc::new(AtomicBool::new(false));
    let slow = CancellationToken::new();
    entries.insert(id, Subscriber { sender, enabled: enabled.clone(), slow: slow.clone() });
    Some(Connection { hub: self.clone(), user_id, id, enabled, slow, receiver })
  }

  pub fn publish(&self, user_id: u64, event: &'static str, source: &'static str) {
    let Some(entries) = self.users.get(&user_id) else {
      return;
    };
    let data: Arc<str> = serde_json::json!({"version":1,"type":"event","event":event,"event_id":Uuid::now_v7().to_string(),"source":source}).to_string().into();
    for entry in entries.values() {
      if entry.enabled.load(Ordering::Relaxed) && entry.sender.try_send(data.clone()).is_err() {
        tracing::warn!("websocket_slow_consumer");
        entry.slow.cancel();
      }
    }
  }
}

impl Drop for Connection {
  fn drop(&mut self) {
    if let Entry::Occupied(mut entry) = self.hub.users.entry(self.user_id) {
      entry.get_mut().remove(&self.id);
      if entry.get().is_empty() {
        entry.remove();
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn isolated_bounded_and_cleaned_up() {
    let hub = Arc::new(EventHub::default());
    let mut a = hub.connect(1, 1).unwrap();
    let mut b = hub.connect(2, 1).unwrap();
    a.enabled.store(true, Ordering::Relaxed);
    b.enabled.store(true, Ordering::Relaxed);
    hub.publish(1, "profile.updated", "application");
    assert!(a.receiver.try_recv().is_ok());
    assert!(b.receiver.try_recv().is_err());
    hub.publish(1, "profile.updated", "application");
    hub.publish(1, "profile.updated", "application");
    assert!(a.slow.is_cancelled());
    let second = hub.connect(1, 1).unwrap();
    assert!(hub.connect(1, 1).is_none());
    drop(a);
    drop(second);
    drop(b);
    assert!(hub.users.is_empty());
  }
}
