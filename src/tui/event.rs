use crossterm::event::{Event as CrosstermEvent, KeyEvent};
use futures::{Stream, StreamExt};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;

const EVENT_QUEUE_CAPACITY: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Key(KeyEvent),
    Tick,
}

/// Sends events without allowing periodic ticks to consume the queue.
///
/// Key events are lossless and retain FIFO order. When the bounded queue is
/// full, the producer applies backpressure and stops polling the terminal until
/// capacity is available. Ticks never wait for capacity: one pending tick is
/// retained, additional ticks are coalesced, and a tick attempted while the
/// queue is full is discarded so keys can drain first.
#[derive(Clone)]
struct EventSender {
    sender: mpsc::Sender<Event>,
    tick_pending: Arc<AtomicBool>,
}

impl EventSender {
    async fn send_key(&self, key: KeyEvent, token: &CancellationToken) -> bool {
        tokio::select! {
            _ = token.cancelled() => false,
            result = self.sender.send(Event::Key(key)) => result.is_ok(),
        }
    }

    fn try_send_tick(&self) -> bool {
        if self.tick_pending.swap(true, Ordering::AcqRel) {
            return true;
        }

        match self.sender.try_send(Event::Tick) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.tick_pending.store(false, Ordering::Release);
                true
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                self.tick_pending.store(false, Ordering::Release);
                false
            }
        }
    }
}

pub struct EventHandler {
    receiver: mpsc::Receiver<Event>,
    tick_pending: Arc<AtomicBool>,
    cancellation_token: CancellationToken,
    producer: Option<JoinHandle<()>>,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        Self::with_stream(
            tick_rate,
            EVENT_QUEUE_CAPACITY,
            crossterm::event::EventStream::new(),
        )
    }

    fn with_stream<S>(tick_rate: Duration, capacity: usize, reader: S) -> Self
    where
        S: Stream<Item = std::io::Result<CrosstermEvent>> + Send + Unpin + 'static,
    {
        let (sender, receiver) = mpsc::channel(capacity);
        let tick_pending = Arc::new(AtomicBool::new(false));
        let event_sender = EventSender {
            sender,
            tick_pending: Arc::clone(&tick_pending),
        };
        let cancellation_token = CancellationToken::new();
        let token = cancellation_token.clone();
        let producer = tokio::spawn(run_event_producer(reader, tick_rate, event_sender, token));

        Self {
            receiver,
            tick_pending,
            cancellation_token,
            producer: Some(producer),
        }
    }

    pub async fn next(&mut self) -> Option<Event> {
        let event = self.receiver.recv().await?;
        if matches!(event, Event::Tick) {
            self.tick_pending.store(false, Ordering::Release);
        }
        Some(event)
    }

    pub async fn stop(&mut self) {
        self.cancellation_token.cancel();
        if let Some(producer) = self.producer.take() {
            let _ = producer.await;
        }
    }
}

impl Drop for EventHandler {
    fn drop(&mut self) {
        self.cancellation_token.cancel();
    }
}

async fn run_event_producer<S>(
    mut reader: S,
    tick_rate: Duration,
    sender: EventSender,
    token: CancellationToken,
) where
    S: Stream<Item = std::io::Result<CrosstermEvent>> + Unpin,
{
    loop {
        tokio::select! {
            _ = token.cancelled() => break,
            event = reader.next() => match event {
                Some(Ok(CrosstermEvent::Key(key))) => {
                    if !sender.send_key(key, &token).await {
                        break;
                    }
                }
                Some(Ok(_)) | Some(Err(_)) => {}
                None => break,
            },
            _ = tokio::time::sleep(tick_rate) => {
                if !sender.try_send_tick() {
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};
    use futures::stream;
    use std::{
        pin::Pin,
        task::{Context, Poll},
    };

    fn key(character: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE)
    }

    fn test_channel(capacity: usize) -> (EventSender, EventHandler) {
        let (sender, receiver) = mpsc::channel(capacity);
        let tick_pending = Arc::new(AtomicBool::new(false));
        let event_sender = EventSender {
            sender,
            tick_pending: Arc::clone(&tick_pending),
        };
        let handler = EventHandler {
            receiver,
            tick_pending,
            cancellation_token: CancellationToken::new(),
            producer: None,
        };
        (event_sender, handler)
    }

    #[tokio::test]
    async fn stalled_receiver_keeps_only_one_tick_in_bounded_queue() {
        let (sender, mut handler) = test_channel(4);

        for _ in 0..1_000 {
            assert!(sender.try_send_tick());
        }

        assert_eq!(handler.receiver.len(), 1);
        assert_eq!(handler.next().await, Some(Event::Tick));
        assert_eq!(handler.receiver.len(), 0);

        assert!(sender.try_send_tick());
        assert_eq!(handler.receiver.len(), 1);
    }

    #[tokio::test]
    async fn key_overflow_applies_lossless_fifo_backpressure() {
        let (sender, mut handler) = test_channel(2);
        let token = CancellationToken::new();

        assert!(sender.send_key(key('a'), &token).await);
        assert!(sender.send_key(key('b'), &token).await);
        assert_eq!(handler.receiver.len(), 2);

        let blocked_sender = sender.clone();
        let blocked_token = token.clone();
        let mut blocked =
            tokio::spawn(async move { blocked_sender.send_key(key('c'), &blocked_token).await });
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut blocked)
                .await
                .is_err()
        );

        assert_eq!(handler.next().await, Some(Event::Key(key('a'))));
        assert!(tokio::time::timeout(Duration::from_secs(1), &mut blocked)
            .await
            .expect("blocked key send should resume when capacity is available")
            .expect("key sender task should not panic"));
        assert_eq!(handler.next().await, Some(Event::Key(key('b'))));
        assert_eq!(handler.next().await, Some(Event::Key(key('c'))));
    }

    struct DropAwarePendingStream {
        dropped: Arc<AtomicBool>,
    }

    impl Stream for DropAwarePendingStream {
        type Item = std::io::Result<CrosstermEvent>;

        fn poll_next(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Pending
        }
    }

    impl Drop for DropAwarePendingStream {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::Release);
        }
    }

    #[tokio::test]
    async fn stop_waits_for_the_producer_to_terminate() {
        let dropped = Arc::new(AtomicBool::new(false));
        let reader = DropAwarePendingStream {
            dropped: Arc::clone(&dropped),
        };
        let mut handler = EventHandler::with_stream(Duration::from_secs(60), 2, reader);

        handler.stop().await;

        assert!(dropped.load(Ordering::Acquire));
        assert!(handler.producer.is_none());
    }

    #[tokio::test]
    async fn dropping_handler_cancels_the_producer() {
        let dropped = Arc::new(AtomicBool::new(false));
        let reader = DropAwarePendingStream {
            dropped: Arc::clone(&dropped),
        };
        let handler = EventHandler::with_stream(Duration::from_secs(60), 2, reader);

        drop(handler);

        tokio::time::timeout(Duration::from_secs(1), async {
            while !dropped.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("producer should terminate when its handler is dropped");
    }

    #[tokio::test]
    async fn producer_delivers_keys_and_stops_when_stream_closes() {
        let reader = stream::iter([Ok(CrosstermEvent::Key(key('x')))]);
        let mut handler = EventHandler::with_stream(Duration::from_secs(60), 2, reader);

        assert_eq!(handler.next().await, Some(Event::Key(key('x'))));
        handler.stop().await;
        assert!(handler.producer.is_none());
    }
}
