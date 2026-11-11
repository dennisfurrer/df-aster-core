//! The websocket for native programs: a [`Feed`] carries one [`crate::live::Session`] over
//! tokio-tungstenite and hands back [`crate::live::Event`]s, each frame's text beside it when
//! asked. Subscriptions made while down are sent on the next open; frames go out as the session's
//! rate allows; the venue's protocol pings are answered by tungstenite and count as the socket
//! being alive. [`keep_listen_key_alive`] keeps an account's listen key from lapsing.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::client::native::{Native, clock};
use crate::client::{Client, Credentials};
use crate::live::{Event, FeedError, Refused, Session, Subscription, Tick, account};
use futures_util::{SinkExt, StreamExt};
use tokio::{
    net::TcpStream,
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time::{Instant, sleep_until},
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Message, Utf8Bytes},
};

enum Command {
    Subscribe(Subscription, oneshot::Sender<Result<(), Refused>>),
    Unsubscribe(Subscription),
    ListenKey(String, oneshot::Sender<Result<(), Refused>>),
    AskSubscriptions,
}

/// An event, and for a frame the text it arrived as.
type Delivered = (Event, Option<Utf8Bytes>);

/// One websocket connection, kept open. Dropping the feed closes it.
pub struct Feed {
    commands: mpsc::UnboundedSender<Command>,
    events: mpsc::UnboundedReceiver<Delivered>,
    task: JoinHandle<()>,
}

impl Feed {
    /// Connects to the session's socket on the current tokio runtime.
    #[must_use]
    pub fn start(session: Session) -> Feed {
        let url = session.url().to_owned();
        Feed::start_at(url, session)
    }

    /// Connects to `url` instead of the session's: a proxy, or a test.
    #[must_use]
    pub fn start_at(url: impl Into<String>, session: Session) -> Feed {
        let (commands, inbox) = mpsc::unbounded_channel();
        let (outbox, events) = mpsc::unbounded_channel();
        let task = tokio::spawn(run(url.into(), session, inbox, outbox));
        Feed { commands, events, task }
    }

    /// Wants `subscription` from now on, across reconnects. Refused past the session's limit, or
    /// for a stream the venue cannot serve.
    pub async fn subscribe(&self, subscription: Subscription) -> Result<(), FeedError> {
        let (reply, answer) = oneshot::channel();
        self.commands.send(Command::Subscribe(subscription, reply)).map_err(|_| FeedError::Stopped)?;
        Ok(answer.await.map_err(|_| FeedError::Stopped)??)
    }

    pub fn unsubscribe(&self, subscription: Subscription) -> Result<(), FeedError> {
        self.commands.send(Command::Unsubscribe(subscription)).map_err(|_| FeedError::Stopped)
    }

    /// Follows the account stream with `listen_key`, in place of any key before it.
    pub async fn set_listen_key(&self, listen_key: impl Into<String>) -> Result<(), FeedError> {
        let (reply, answer) = oneshot::channel();
        self.commands.send(Command::ListenKey(listen_key.into(), reply)).map_err(|_| FeedError::Stopped)?;
        Ok(answer.await.map_err(|_| FeedError::Stopped)??)
    }

    /// Asks the venue what the socket streams; the answer arrives as a
    /// [`crate::live::Frame::Answer`].
    pub fn ask_subscriptions(&self) -> Result<(), FeedError> {
        self.commands.send(Command::AskSubscriptions).map_err(|_| FeedError::Stopped)
    }

    /// The next event; `None` once the feed has stopped.
    pub async fn next(&mut self) -> Option<Event> {
        self.events.recv().await.map(|(event, _)| event)
    }

    /// The next event, and for a frame, read or not, the text it arrived as: for a program that
    /// still reads some streams its own way. `None` once the feed has stopped.
    pub async fn next_with_text(&mut self) -> Option<(Event, Option<String>)> {
        let (event, text) = self.events.recv().await?;
        Some((event, text.map(|text| text.as_str().to_owned())))
    }

    /// Closes the socket and stops.
    pub fn close(self) {
        drop(self);
    }
}

impl Drop for Feed {
    fn drop(&mut self) {
        self.task.abort();
    }
}

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Whether the connection was lost, and why, or ended to renew, or the feed is done.
enum Outcome {
    Lost(String),
    Renew,
    Stop,
}

async fn run(
    url: String,
    mut session: Session,
    mut commands: mpsc::UnboundedReceiver<Command>,
    events: mpsc::UnboundedSender<Delivered>,
) {
    loop {
        // While it dials, the session still answers: a subscription is kept for the open.
        let dial = connect_async(url.as_str());
        tokio::pin!(dial);
        let dialled = loop {
            tokio::select! {
                dialled = &mut dial => break dialled,
                command = commands.recv() => match command {
                    Some(command) => handle(&mut session, command),
                    None => return,
                },
            }
        };
        let (reason, retry) = match dialled {
            Ok((mut socket, _)) => match open(&mut socket, &mut session, &mut commands, &events).await {
                Outcome::Lost(reason) => (reason, session.closed(clock(), random())),
                Outcome::Renew => ("the socket reached its 24 hours".to_owned(), session.renewed()),
                Outcome::Stop => return,
            },
            Err(error) => (error.to_string(), session.closed(clock(), random())),
        };
        let until = Instant::now() + Duration::from_millis(retry.after_ms);
        if events.send((Event::Closed { reason, retry }, None)).is_err() {
            return;
        }
        // Until the next dial, subscriptions are kept for it.
        loop {
            tokio::select! {
                () = sleep_until(until) => break,
                command = commands.recv() => match command {
                    Some(command) => handle(&mut session, command),
                    None => return,
                },
            }
        }
    }
}

async fn open(
    socket: &mut Socket,
    session: &mut Session,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    events: &mpsc::UnboundedSender<Delivered>,
) -> Outcome {
    if let Err(reason) = send(socket, session.opened(clock())).await {
        return Outcome::Lost(reason);
    }
    if events.send((Event::Opened, None)).is_err() {
        return Outcome::Stop;
    }
    loop {
        let wake = session.due().map(|at| Instant::now() + Duration::from_millis(at.saturating_sub(clock())));
        tokio::select! {
            message = socket.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    let event = match session.received(clock(), &text) {
                        Ok(frame) => Event::Frame(Box::new(frame)),
                        Err(bad) => Event::Unreadable(bad),
                    };
                    if events.send((event, Some(text))).is_err() {
                        return Outcome::Stop;
                    }
                }
                Some(Ok(Message::Close(frame))) => {
                    return Outcome::Lost(frame.map_or_else(|| "the venue closed the socket".to_owned(), |frame| frame.reason.to_string()));
                }
                // The venue's protocol ping, answered by tungstenite: the socket is alive.
                Some(Ok(_)) => session.heard(clock()),
                Some(Err(error)) => return Outcome::Lost(error.to_string()),
                None => return Outcome::Lost("the socket ended".to_owned()),
            },
            command = commands.recv() => match command {
                Some(command) => {
                    handle(session, command);
                    if let Err(reason) = send(socket, session.owed(clock())).await {
                        return Outcome::Lost(reason);
                    }
                }
                None => {
                    let _ = socket.close(None).await;
                    return Outcome::Stop;
                }
            },
            () = until(wake) => match session.tick(clock()) {
                Tick::Wait => {}
                Tick::Send => {
                    if let Err(reason) = send(socket, session.owed(clock())).await {
                        return Outcome::Lost(reason);
                    }
                }
                Tick::Reconnect => {
                    let _ = socket.close(None).await;
                    return Outcome::Lost("nothing arrived for too long".to_owned());
                }
                Tick::Renew => {
                    let _ = socket.close(None).await;
                    return Outcome::Renew;
                }
            },
        }
    }
}

/// Sends `frames` in order, or says why the socket would not take them.
async fn send(socket: &mut Socket, frames: Vec<String>) -> Result<(), String> {
    for frame in frames {
        socket.send(Message::text(frame)).await.map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Puts a command to the session and answers its caller. What it owes the venue goes out with the
/// next [`Session::owed`].
fn handle(session: &mut Session, command: Command) {
    match command {
        Command::Subscribe(subscription, reply) => {
            let _ = reply.send(session.subscribe(subscription));
        }
        Command::Unsubscribe(subscription) => {
            session.unsubscribe(&subscription);
        }
        Command::ListenKey(key, reply) => {
            let _ = reply.send(session.set_listen_key(&key));
        }
        Command::AskSubscriptions => session.ask_subscriptions(),
    }
}

async fn until(wake: Option<Instant>) {
    match wake {
        Some(at) => sleep_until(at).await,
        None => std::future::pending().await,
    }
}

/// Enough to spread reconnects: many clients dropped at once must not return at once. Not for
/// anything secret.
fn random() -> u64 {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| u64::from(since.subsec_nanos()));
    nanos.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ clock().rotate_left(29)
}

/// Keeps the account's listen key alive (`PUT /fapi/v3/listenKey`) every `every`, while anything
/// else holds the client, and ends by itself once the last holder lets go. A keepalive that fails
/// is tried again at the next turn; [`account::KEEP_ALIVE_MS`] leaves room for one.
pub fn keep_listen_key_alive<K: Credentials + Send + Sync + 'static>(
    client: &Arc<Client<Native, K>>,
    every: Duration,
) -> JoinHandle<()> {
    let held = Arc::downgrade(client);
    tokio::spawn(async move {
        let mut ticks = tokio::time::interval_at(tokio::time::Instant::now() + every, every);
        loop {
            ticks.tick().await;
            let Some(client) = held.upgrade() else {
                return;
            };
            let _ = account::keep_alive(&*client).await;
        }
    })
}

#[cfg(test)]
#[path = "tests/socket.rs"]
mod tests;
