//! Effect RPC client over one WebSocket.
//!
//! [`RpcConnection::connect`] opens the socket and starts a task on the networking runtime that
//! owns it. That task routes server frames to callers by request id, acks every stream chunk
//! (the server sends nothing more on a stream until it gets the ack), and pings every 5 seconds;
//! three consecutive missed pongs close the connection, matching upstream's patched
//! `makePinger`. A server `Defect` fails every in-flight request but keeps the socket open
//! (protocol.md 1.7); owners of subscriptions resubscribe.
//!
//! ```ignore
//! let conn = RpcConnection::connect(request).await?;
//! let config = conn.request::<ServerGetConfig>(&Empty {}).await?;
//! let mut shell = conn.subscribe::<SubscribeShell>(&Empty {});
//! while let Some(item) = shell.next().await { /* ... */ }
//! ```
//!
//! A connection never reconnects; the environment supervisor replaces it.

use std::{
    collections::{HashMap, VecDeque},
    marker::PhantomData,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use futures::{SinkExt as _, StreamExt as _};
use serde_json::value::RawValue;
use t3_protocol::{
    Stream, Unary,
    rpc::{CauseReason, ClientFrame, ExitEncoded, ServerFrame, defect_message},
};
use tokio::sync::{mpsc, oneshot, watch};
use tokio_tungstenite::tungstenite::{self, Message, client::IntoClientRequest};

/// Which way a tapped frame went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameDirection {
    Sent,
    Received,
}

type FrameTap = Box<dyn Fn(FrameDirection, &str) + Send + Sync>;
static FRAME_TAP: std::sync::OnceLock<FrameTap> = std::sync::OnceLock::new();

/// Installs a process-wide observer for every text frame sent or received (pings included).
/// For debugging and recording golden transcripts; set it once, before connecting.
pub fn set_frame_tap(tap: impl Fn(FrameDirection, &str) + Send + Sync + 'static) {
    let _ = FRAME_TAP.set(Box::new(tap));
}

fn tap(direction: FrameDirection, text: &str) {
    if let Some(tap) = FRAME_TAP.get() {
        tap(direction, text);
    }
}

const PING_INTERVAL: Duration = Duration::from_secs(5);
/// Consecutive unanswered pings before the connection is considered dead.
const MAX_MISSED_PONGS: u32 = 3;
const OPEN_TIMEOUT: Duration = Duration::from_secs(15);

/// Why a connection ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloseReason {
    /// The client called [`RpcConnection::close`] or dropped every handle.
    Closed,
    /// The server closed the socket.
    Remote(String),
    /// Three pings in a row went unanswered.
    PingTimeout,
    /// The environment has no live connection right now.
    NotConnected,
    /// The socket failed.
    Transport(String),
}

impl std::fmt::Display for CloseReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CloseReason::Closed => write!(f, "connection closed"),
            CloseReason::Remote(reason) if reason.is_empty() => {
                write!(f, "server closed the connection")
            }
            CloseReason::Remote(reason) => write!(f, "server closed the connection: {reason}"),
            CloseReason::PingTimeout => write!(f, "server stopped responding"),
            CloseReason::NotConnected => write!(f, "not connected"),
            CloseReason::Transport(error) => write!(f, "{error}"),
        }
    }
}

/// Failure to open a connection.
#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error("invalid websocket request: {0}")]
    InvalidRequest(String),
    #[error("timed out opening the websocket")]
    Timeout,
    #[error("websocket rejected with HTTP {status}")]
    Rejected { status: u16, body: Option<String> },
    #[error("{0}")]
    Transport(String),
}

/// Failure of one RPC call. `E` is the method's typed error schema.
#[derive(Debug, thiserror::Error)]
pub enum RpcError<E> {
    /// The server failed the request with the method's expected error.
    #[error("request failed")]
    Failed(E),
    /// The server hit an unexpected error (Effect `Die`), or sent an undecodable failure.
    #[error("server defect: {0}")]
    Defect(String),
    /// The request was interrupted on the server.
    #[error("request interrupted")]
    Interrupted,
    /// The connection ended before the request completed.
    #[error("disconnected: {0}")]
    Disconnected(CloseReason),
    /// The response did not match the expected schema.
    #[error("could not decode response: {0}")]
    Decode(String),
}

impl<E> RpcError<E> {
    /// The typed failure, if the server returned one.
    pub fn failure(&self) -> Option<&E> {
        match self {
            RpcError::Failed(error) => Some(error),
            _ => None,
        }
    }
}

/// Untyped outcome delivered by the socket task.
#[derive(Debug)]
enum Outcome {
    Success(Box<RawValue>),
    Failure(Vec<CauseReason>),
    /// A connection-level `Defect` frame killed every in-flight request.
    Defect(String),
    Disconnected(CloseReason),
}

impl Outcome {
    fn into_result<T: serde::de::DeserializeOwned, E: serde::de::DeserializeOwned>(
        self,
    ) -> Result<T, RpcError<E>> {
        match self {
            Outcome::Success(value) => {
                serde_json::from_str(value.get()).map_err(|e| RpcError::Decode(e.to_string()))
            }
            Outcome::Failure(cause) => Err(failure_from_cause(cause)),
            Outcome::Defect(defect) => Err(RpcError::Defect(defect)),
            Outcome::Disconnected(reason) => Err(RpcError::Disconnected(reason)),
        }
    }
}

/// Picks the most meaningful reason from an encoded cause: typed failures first, then defects.
fn failure_from_cause<E: serde::de::DeserializeOwned>(cause: Vec<CauseReason>) -> RpcError<E> {
    let mut defect = None;
    for reason in &cause {
        match reason {
            CauseReason::Fail { error } => {
                return match serde_json::from_str(error.get()) {
                    Ok(error) => RpcError::Failed(error),
                    Err(_) => RpcError::Defect(error.get().to_owned()),
                };
            }
            CauseReason::Die { defect: value } => defect = Some(defect_message(value)),
            CauseReason::Interrupt => {}
        }
    }
    defect.map_or(RpcError::Interrupted, RpcError::Defect)
}

enum StreamEvent {
    Values(Vec<Box<RawValue>>),
    End(Outcome),
}

enum Pending {
    Unary(oneshot::Sender<Outcome>),
    Stream(mpsc::UnboundedSender<StreamEvent>),
}

enum Command {
    Request {
        id: u64,
        tag: &'static str,
        payload: Box<RawValue>,
        pending: Pending,
    },
    Interrupt {
        id: u64,
    },
    Close,
}

struct Shared {
    commands: mpsc::UnboundedSender<Command>,
    next_id: AtomicU64,
    closed: watch::Receiver<Option<CloseReason>>,
}

/// A live RPC connection. Cheap to clone; the socket closes when the last clone drops.
#[derive(Clone)]
pub struct RpcConnection {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for RpcConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RpcConnection")
            .field("closed", &self.close_reason())
            .finish()
    }
}

impl RpcConnection {
    /// Opens the WebSocket described by `request` (a URL or a full HTTP request with headers).
    pub async fn connect(
        request: impl IntoClientRequest + Send + 'static,
    ) -> Result<Self, ConnectError> {
        crate::runtime::spawn(async move {
            let request = request
                .into_client_request()
                .map_err(|e| ConnectError::InvalidRequest(e.to_string()))?;
            let (socket, _response) =
                match tokio::time::timeout(OPEN_TIMEOUT, tokio_tungstenite::connect_async(request))
                    .await
                {
                    Err(_) => return Err(ConnectError::Timeout),
                    Ok(Err(tungstenite::Error::Http(response))) => {
                        return Err(ConnectError::Rejected {
                            status: response.status().as_u16(),
                            body: response
                                .body()
                                .as_ref()
                                .map(|body| String::from_utf8_lossy(body).into_owned()),
                        });
                    }
                    Ok(Err(error)) => return Err(ConnectError::Transport(error.to_string())),
                    Ok(Ok(pair)) => pair,
                };
            let (commands, command_rx) = mpsc::unbounded_channel();
            let (closed_tx, closed) = watch::channel(None);
            tokio::spawn(run_socket(socket, command_rx, closed_tx));
            Ok(RpcConnection {
                shared: Arc::new(Shared {
                    commands,
                    next_id: AtomicU64::new(0),
                    closed,
                }),
            })
        })
        .await
    }

    /// Sends a unary request and waits for its exit.
    pub async fn request<M: Unary>(
        &self,
        payload: &M::Payload,
    ) -> Result<M::Success, RpcError<M::Error>> {
        let payload = encode_payload(payload)?;
        let (tx, rx) = oneshot::channel();
        let id = self.send_request(M::TAG, payload, Pending::Unary(tx));
        let mut guard = InterruptOnDrop {
            shared: Some(self.shared.clone()),
            id,
        };
        let outcome = rx.await.unwrap_or_else(|_| {
            Outcome::Disconnected(self.close_reason().unwrap_or(CloseReason::Closed))
        });
        guard.shared = None;
        outcome.into_result()
    }

    /// Starts a streaming request. Dropping the [`Subscription`] interrupts it on the server.
    pub fn subscribe<M: Stream>(&self, payload: &M::Payload) -> Subscription<M> {
        let (tx, rx) = mpsc::unbounded_channel();
        let (id, done) = match encode_payload::<_, M::Error>(payload) {
            Ok(payload) => (
                self.send_request(M::TAG, payload, Pending::Stream(tx)),
                None,
            ),
            Err(RpcError::Decode(error)) => (u64::MAX, Some(error)),
            Err(_) => unreachable!("encode_payload only fails with Decode"),
        };
        Subscription {
            shared: self.shared.clone(),
            id,
            events: rx,
            buffered: VecDeque::new(),
            finished: done.is_some(),
            encode_error: done,
            _method: PhantomData,
        }
    }

    /// Resolves when the connection ends, with the reason.
    pub async fn closed(&self) -> CloseReason {
        let mut closed = self.shared.closed.clone();
        match closed.wait_for(Option::is_some).await {
            Ok(reason) => reason.clone().unwrap_or(CloseReason::Closed),
            Err(_) => CloseReason::Closed,
        }
    }

    /// The close reason if the connection has already ended.
    pub fn close_reason(&self) -> Option<CloseReason> {
        self.shared.closed.borrow().clone()
    }

    /// Closes the socket. Pending requests fail with [`CloseReason::Closed`].
    pub fn close(&self) {
        let _ = self.shared.commands.send(Command::Close);
    }

    fn send_request(&self, tag: &'static str, payload: Box<RawValue>, pending: Pending) -> u64 {
        let id = self.shared.next_id.fetch_add(1, Ordering::Relaxed);
        if let Err(mpsc::error::SendError(Command::Request { pending, .. })) =
            self.shared.commands.send(Command::Request {
                id,
                tag,
                payload,
                pending,
            })
        {
            // The socket task is gone; fail the caller immediately.
            let reason = self.close_reason().unwrap_or(CloseReason::Closed);
            match pending {
                Pending::Unary(tx) => drop(tx.send(Outcome::Disconnected(reason))),
                Pending::Stream(tx) => {
                    drop(tx.send(StreamEvent::End(Outcome::Disconnected(reason))))
                }
            }
        }
        id
    }
}

fn encode_payload<P: serde::Serialize, E>(payload: &P) -> Result<Box<RawValue>, RpcError<E>> {
    serde_json::value::to_raw_value(payload).map_err(|e| RpcError::Decode(e.to_string()))
}

/// Interrupts an in-flight unary request if its future is dropped before completing.
struct InterruptOnDrop {
    shared: Option<Arc<Shared>>,
    id: u64,
}

impl Drop for InterruptOnDrop {
    fn drop(&mut self) {
        if let Some(shared) = self.shared.take() {
            let _ = shared.commands.send(Command::Interrupt { id: self.id });
        }
    }
}

/// Items from a streaming RPC. Ends with `None` after the server's exit.
pub struct Subscription<M: Stream> {
    shared: Arc<Shared>,
    id: u64,
    events: mpsc::UnboundedReceiver<StreamEvent>,
    buffered: VecDeque<Box<RawValue>>,
    finished: bool,
    encode_error: Option<String>,
    _method: PhantomData<fn() -> M>,
}

/// What [`Subscription::next`] yields: an item, a terminal error, or `None` at the end.
pub type StreamNext<M> = Option<Result<<M as Stream>::Item, RpcError<<M as Stream>::Error>>>;

impl<M: Stream> Subscription<M> {
    /// The next item, a terminal error, or `None` when the stream completed successfully.
    /// Cancel safe. A `Decode` error affects only that item; the stream continues.
    pub async fn next(&mut self) -> StreamNext<M> {
        loop {
            if let Some(ready) = self.take_buffered() {
                return ready;
            }
            let event = self.events.recv().await;
            if let Some(ready) = self.accept(event) {
                return ready;
            }
        }
    }

    /// Like [`next`](Self::next) but never waits: `None` means nothing has arrived yet. Lets a
    /// consumer apply a whole chunk before publishing state once.
    pub fn try_next(&mut self) -> Option<StreamNext<M>> {
        loop {
            if let Some(ready) = self.take_buffered() {
                return Some(ready);
            }
            let event = match self.events.try_recv() {
                Ok(event) => Some(event),
                Err(mpsc::error::TryRecvError::Empty) => return None,
                Err(mpsc::error::TryRecvError::Disconnected) => None,
            };
            if let Some(ready) = self.accept(event) {
                return Some(ready);
            }
        }
    }

    /// A buffered item, the encode error, or the end marker, if any is pending.
    fn take_buffered(&mut self) -> Option<StreamNext<M>> {
        if let Some(raw) = self.buffered.pop_front() {
            return Some(Some(
                serde_json::from_str(raw.get()).map_err(|e| RpcError::Decode(e.to_string())),
            ));
        }
        if let Some(error) = self.encode_error.take() {
            return Some(Some(Err(RpcError::Decode(error))));
        }
        self.finished.then_some(None)
    }

    /// Takes in one event from the socket task. Returns a terminal result for `End`.
    fn accept(&mut self, event: Option<StreamEvent>) -> Option<StreamNext<M>> {
        match event {
            Some(StreamEvent::Values(values)) => {
                self.buffered.extend(values);
                None
            }
            Some(StreamEvent::End(outcome)) => {
                self.finished = true;
                Some(match outcome {
                    Outcome::Success(_) => None,
                    other => other.into_result::<(), M::Error>().err().map(Err),
                })
            }
            None => {
                self.finished = true;
                Some(Some(Err(RpcError::Disconnected(CloseReason::Closed))))
            }
        }
    }
}

impl<M: Stream> Drop for Subscription<M> {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self
                .shared
                .commands
                .send(Command::Interrupt { id: self.id });
        }
    }
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Owns the socket until it closes, then fails every pending request with the close reason.
async fn run_socket(
    mut socket: Socket,
    mut commands: mpsc::UnboundedReceiver<Command>,
    closed: watch::Sender<Option<CloseReason>>,
) {
    let mut pending: HashMap<u64, Pending> = HashMap::new();
    let mut ping =
        tokio::time::interval_at(tokio::time::Instant::now() + PING_INTERVAL, PING_INTERVAL);
    let mut pings = PingState::default();

    let reason = loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(Command::Request { id, tag, payload, pending: entry }) => {
                    pending.insert(id, entry);
                    let frame = ClientFrame::Request { id: id.to_string(), tag: tag.to_owned(), payload, headers: vec![] };
                    if let Err(error) = send(&mut socket, &frame).await { break error; }
                }
                Some(Command::Interrupt { id }) => {
                    if pending.remove(&id).is_some()
                        && let Err(error) = send(&mut socket, &ClientFrame::Interrupt { request_id: id.to_string() }).await
                    {
                        break error;
                    }
                }
                // Never send `Eof`: it wedges the server side of the connection (protocol.md 1.8).
                Some(Command::Close) | None => {
                    let _ = socket.close(None).await;
                    break CloseReason::Closed;
                }
            },
            message = socket.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    if let Err(reason) = handle_text(&mut socket, &mut pending, &mut pings, text.as_str()).await {
                        break reason;
                    }
                }
                Some(Ok(Message::Binary(bytes))) => {
                    let text = String::from_utf8_lossy(&bytes).into_owned();
                    if let Err(reason) = handle_text(&mut socket, &mut pending, &mut pings, &text).await {
                        break reason;
                    }
                }
                Some(Ok(Message::Close(frame))) => {
                    break CloseReason::Remote(frame.map(|f| f.reason.to_string()).unwrap_or_default());
                }
                Some(Ok(_)) => {}
                Some(Err(error)) => break CloseReason::Transport(error.to_string()),
                None => break CloseReason::Remote(String::new()),
            },
            _ = ping.tick() => {
                if pings.awaiting {
                    pings.missed += 1;
                    if pings.missed >= MAX_MISSED_PONGS { break CloseReason::PingTimeout; }
                }
                pings.awaiting = true;
                if let Err(error) = send(&mut socket, &ClientFrame::Ping).await { break error; }
            }
        }
    };

    tracing::debug!(%reason, "rpc connection closed");
    let _ = closed.send(Some(reason.clone()));
    for (_, entry) in pending.drain() {
        match entry {
            Pending::Unary(tx) => drop(tx.send(Outcome::Disconnected(reason.clone()))),
            Pending::Stream(tx) => {
                drop(tx.send(StreamEvent::End(Outcome::Disconnected(reason.clone()))))
            }
        }
    }
}

/// Ping bookkeeping. Pongs are not correlated with pings; any pong clears the miss count.
#[derive(Default)]
struct PingState {
    awaiting: bool,
    missed: u32,
}

/// Applies one text frame (one message or a batch). Returns a close reason if the connection
/// must end.
async fn handle_text(
    socket: &mut Socket,
    pending: &mut HashMap<u64, Pending>,
    pings: &mut PingState,
    text: &str,
) -> Result<(), CloseReason> {
    tap(FrameDirection::Received, text);
    let frames = match ServerFrame::decode_all(text) {
        Ok(frames) => frames,
        Err(error) => {
            tracing::warn!(%error, "ignoring undecodable rpc frame");
            return Ok(());
        }
    };
    for frame in frames {
        handle_frame(socket, pending, pings, frame).await?;
    }
    Ok(())
}

async fn handle_frame(
    socket: &mut Socket,
    pending: &mut HashMap<u64, Pending>,
    pings: &mut PingState,
    frame: ServerFrame,
) -> Result<(), CloseReason> {
    match frame {
        ServerFrame::Chunk { request_id, values } => {
            let Ok(id) = request_id.parse::<u64>() else {
                return Ok(());
            };
            let delivered = match pending.get(&id) {
                Some(Pending::Stream(tx)) => tx.send(StreamEvent::Values(values)).is_ok(),
                // Unknown or finished id: do not ack (protocol.md 1.5).
                _ => return Ok(()),
            };
            // Ack so the server sends the next chunk; interrupt if nobody is listening anymore.
            let reply = if delivered {
                ClientFrame::Ack { request_id }
            } else {
                pending.remove(&id);
                ClientFrame::Interrupt { request_id }
            };
            send(socket, &reply).await
        }
        ServerFrame::Exit { request_id, exit } => {
            let Ok(id) = request_id.parse::<u64>() else {
                return Ok(());
            };
            let outcome = match exit {
                ExitEncoded::Success { value } => Outcome::Success(value),
                ExitEncoded::Failure { cause } => Outcome::Failure(cause),
            };
            match pending.remove(&id) {
                Some(Pending::Unary(tx)) => drop(tx.send(outcome)),
                Some(Pending::Stream(tx)) => drop(tx.send(StreamEvent::End(outcome))),
                None => {}
            }
            Ok(())
        }
        ServerFrame::Pong => {
            pings.awaiting = false;
            pings.missed = 0;
            Ok(())
        }
        // The request that died never gets an `Exit`, and the frame does not say which one it
        // was, so fail them all (what the TS client does). The socket itself stays usable.
        ServerFrame::Defect { defect } => {
            let message = defect_message(&defect);
            tracing::warn!(%message, "server defect; failing in-flight requests");
            for (_, entry) in pending.drain() {
                match entry {
                    Pending::Unary(tx) => drop(tx.send(Outcome::Defect(message.clone()))),
                    Pending::Stream(tx) => {
                        drop(tx.send(StreamEvent::End(Outcome::Defect(message.clone()))))
                    }
                }
            }
            Ok(())
        }
        ServerFrame::ClientProtocolError { error } => {
            tracing::warn!(%error, "server reported a client protocol error");
            Ok(())
        }
        ServerFrame::Unknown { tag } => {
            tracing::debug!(%tag, "ignoring unknown rpc frame");
            Ok(())
        }
    }
}

async fn send(socket: &mut Socket, frame: &ClientFrame) -> Result<(), CloseReason> {
    let text = serde_json::to_string(frame).expect("client frames always serialize");
    tap(FrameDirection::Sent, &text);
    socket
        .send(Message::text(text))
        .await
        .map_err(|e| CloseReason::Transport(e.to_string()))
}
