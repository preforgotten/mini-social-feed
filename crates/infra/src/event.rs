use async_trait::async_trait;
use tokio::sync::broadcast;

use mini_social_feed_app::{AppError, EventPort, EventReceiver};
use mini_social_feed_domain::SystemEvent;

/// In-memory event bus backed by a tokio broadcast channel.
///
/// Independent of the store backend — both MemoryStore and SqliteStore
/// deployments share the same EventBus. Events are fire-and-forget; slow
/// consumers miss old events (bounded channel).
#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<SystemEvent>,
}

impl EventBus {
    /// Create a new event bus with the given buffer size.
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(1024)
    }
}

#[async_trait]
impl EventPort for EventBus {
    async fn emit(&self, event: SystemEvent) -> Result<(), AppError> {
        let _ = self.tx.send(event);
        Ok(())
    }

    async fn subscribe(&self) -> Result<EventReceiver, AppError> {
        let rx = self.tx.subscribe();
        Ok(EventReceiver::new(rx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn emit_and_receive() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe().await.unwrap();

        let evt = SystemEvent::UserRegistered {
            user_id: "u1".into(),
            username: "alice".into(),
        };
        bus.emit(evt.clone()).await.unwrap();

        let received = rx.recv().await.unwrap();
        match received {
            SystemEvent::UserRegistered { user_id, username } => {
                assert_eq!(user_id, "u1");
                assert_eq!(username, "alice");
            }
            _ => panic!("wrong event type"),
        }
    }

    #[tokio::test]
    async fn multiple_subscribers_get_same_event() {
        let bus = EventBus::new(16);
        let mut r1 = bus.subscribe().await.unwrap();
        let mut r2 = bus.subscribe().await.unwrap();

        bus.emit(SystemEvent::PostCreated {
            post_id: "p1".into(),
            author_id: "u1".into(),
            created_at: 100,
        })
        .await
        .unwrap();

        let e1 = r1.recv().await.unwrap();
        let e2 = r2.recv().await.unwrap();
        assert!(matches!(e1, SystemEvent::PostCreated { .. }));
        assert!(matches!(e2, SystemEvent::PostCreated { .. }));
    }
}
