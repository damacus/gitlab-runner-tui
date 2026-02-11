use crossterm::event::{Event as CrosstermEvent, KeyEvent};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug)]
pub enum Event {
    Key(KeyEvent),
    Tick,
}

pub struct EventHandler {
    receiver: mpsc::UnboundedReceiver<Event>,
    cancellation_token: CancellationToken,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        let cancellation_token = CancellationToken::new();
        let token = cancellation_token.clone();

        tokio::spawn(async move {
            let mut reader = crossterm::event::EventStream::new();
            use futures::StreamExt;

            loop {
                tokio::select! {
                    _ = token.cancelled() => {
                        break;
                    }
                    Some(Ok(event)) = reader.next() => {
                        if let CrosstermEvent::Key(key) = event {
                            if sender.send(Event::Key(key)).is_err() {
                                break;
                            }
                        }
                    }
                    _ = tokio::time::sleep(tick_rate) => {
                        if sender.send(Event::Tick).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        Self {
            receiver,
            cancellation_token,
        }
    }

    pub async fn next(&mut self) -> Option<Event> {
        self.receiver.recv().await
    }

    pub fn stop(&self) {
        self.cancellation_token.cancel();
    }
}
