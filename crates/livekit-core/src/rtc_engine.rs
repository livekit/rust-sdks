use crate::proto::signal_response;
use crate::signal_client::{SignalClient, SignalClientError};
use log::error;
use tokio::sync::broadcast;

pub struct RTCEngine {
    signal_client: SignalClient,
}

impl RTCEngine {
    pub fn new() -> RTCEngine {
        Self {
            signal_client: SignalClient::new(),
        }
    }

    pub async fn connect(&mut self, url: &str, token: &str) -> Result<(), SignalClientError> {
        self.signal_client.connect(url, token).await?;

        tokio::spawn(Self::handle_rtc(
            self.signal_client.response_rx.resubscribe(),
        ));

        Ok(())
    }


    pub fn update(&self) {}

    async fn handle_rtc(mut signal_receiver: broadcast::Receiver<signal_response::Message>) {
        loop {
            let msg = match signal_receiver.recv().await {
                Ok(msg) => msg,
                Err(error) => {
                    error!("Failed to receive SignalResponse: {:?}", error);
                    continue;
                }
            };

            match msg {
                signal_response::Message::Join(join) => {}
                signal_response::Message::Trickle(trickle) => {}
                signal_response::Message::Answer(answer) => {}
                signal_response::Message::Offer(offer) => {}
                _ => {}
            }
        }
    }
}

#[tokio::test]
async fn test_test() {
    env_logger::init();

    let mut engine = RTCEngine::new();
    engine.connect("ws://localhost:7880", "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjE2NjQ1OTY4MDYsImlzcyI6IkFQSUNrSG04M01oZ2hQeCIsIm5hbWUiOiJ1c2VyMSIsIm5iZiI6MTY2MDk5NjgwNiwic3ViIjoidXNlcjEiLCJ2aWRlbyI6eyJyb29tIjoibXktZmlyc3Qtcm9vbSIsInJvb21Kb2luIjp0cnVlfX0.SWU_LETMK6ZmFOf38pYjVhpur0o7jJc6u61h8BH7g20").await.unwrap();

    // Wait before exiting the program
    tokio::time::sleep(core::time::Duration::from_millis(1000 * 25)).await;
}
