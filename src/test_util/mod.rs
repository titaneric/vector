use crate::{
    config::{Config, ConfigDiff},
    topology::{self, RunningTopology},
    trace, Event,
};
use flate2::read::GzDecoder;
use futures::{
    compat::Stream01CompatExt, future, ready, stream, task::noop_waker_ref, FutureExt, SinkExt,
    Stream, StreamExt, TryStreamExt,
};
use futures01::{sync::mpsc, Stream as Stream01};
use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use std::{
    collections::HashMap,
    convert::Infallible,
    fs::File,
    future::Future,
    io::Read,
    iter,
    net::{Shutdown, SocketAddr},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, Result as IoResult},
    net::{TcpListener, TcpStream},
    runtime,
    sync::oneshot,
    task::JoinHandle,
    time::{delay_for, Duration, Instant},
};
use tokio_util::codec::{Encoder, FramedRead, FramedWrite, LinesCodec};

pub mod stats;

#[macro_export]
macro_rules! assert_downcast_matches {
    ($e:expr, $t:ty, $v:pat) => {{
        match $e.downcast_ref::<$t>() {
            Some($v) => (),
            got => panic!("Assertion failed: got wrong error variant {:?}", got),
        }
    }};
}

#[macro_export]
macro_rules! assert_within {
    // Adapted from std::assert_eq
    ($expr:expr, $low:expr, $high:expr) => ({
        match (&$expr, &$low, &$high) {
            (expr, low, high) => {
                if *expr < *low {
                    panic!(
                        r#"assertion failed: `(expr < low)`
expr: {} = `{:?}`,
 low: `{:?}`"#,
                        stringify!($expr),
                        &*expr,
                        &*low
                    );
                }
                if *expr > *high {
                    panic!(
                        r#"assertion failed: `(expr > high)`
expr: {} = `{:?}`,
high: `{:?}`"#,
                        stringify!($expr),
                        &*expr,
                        &*high
                    );
                }
            }
        }
    });
    ($expr:expr, $low:expr, $high:expr, $($arg:tt)+) => ({
        match (&$expr, &$low, &$high) {
            (expr, low, high) => {
                if *expr < *low {
                    panic!(
                        r#"assertion failed: `(expr < low)`
expr: {} = `{:?}`,
 low: `{:?}`
{}"#,
                        stringify!($expr),
                        &*expr,
                        &*low,
                        format_args!($($arg)+)
                    );
                }
                if *expr > *high {
                    panic!(
                        r#"assertion failed: `(expr > high)`
expr: {} = `{:?}`,
high: `{:?}`
{}"#,
                        stringify!($expr),
                        &*expr,
                        &*high,
                        format_args!($($arg)+)
                    );
                }
            }
        }
    });

}

static NEXT_PORT: AtomicUsize = AtomicUsize::new(1234);

pub fn next_addr() -> SocketAddr {
    use std::net::{IpAddr, Ipv4Addr};

    let port = NEXT_PORT.fetch_add(1, Ordering::AcqRel) as u16;
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port)
}

pub fn trace_init() {
    #[cfg(unix)]
    let color = atty::is(atty::Stream::Stdout);
    // Windows: ANSI colors are not supported by cmd.exe
    // Color is false for everything except unix.
    #[cfg(not(unix))]
    let color = false;

    let levels = std::env::var("TEST_LOG").unwrap_or_else(|_| "off".to_string());

    trace::init(color, false, &levels);
}

pub async fn send_lines(
    addr: SocketAddr,
    lines: impl IntoIterator<Item = String>,
) -> Result<(), Infallible> {
    send_encodable(addr, LinesCodec::new(), lines).await
}

pub async fn send_encodable<I, E: From<std::io::Error> + std::fmt::Debug>(
    addr: SocketAddr,
    encoder: impl Encoder<I, Error = E>,
    lines: impl IntoIterator<Item = I>,
) -> Result<(), Infallible> {
    let stream = TcpStream::connect(&addr).await.unwrap();
    let mut sink = FramedWrite::new(stream, encoder);

    let mut lines = stream::iter(lines.into_iter()).map(Ok);
    sink.send_all(&mut lines).await.unwrap();

    let stream = sink.get_mut();
    stream.shutdown(Shutdown::Both).unwrap();

    Ok(())
}

pub async fn send_lines_tls(
    addr: SocketAddr,
    host: String,
    lines: impl Iterator<Item = String>,
) -> Result<(), Infallible> {
    let stream = TcpStream::connect(&addr).await.unwrap();

    let mut connector = SslConnector::builder(SslMethod::tls()).unwrap();
    connector.set_verify(SslVerifyMode::NONE);
    let config = connector.build().configure().unwrap();

    let stream = tokio_openssl::connect(config, &host, stream).await.unwrap();
    let mut sink = FramedWrite::new(stream, LinesCodec::new());

    let mut lines = stream::iter(lines).map(Ok);
    sink.send_all(&mut lines).await.unwrap();

    let stream = sink.get_mut().get_mut();
    stream.shutdown(Shutdown::Both).unwrap();

    Ok(())
}

pub fn temp_file() -> PathBuf {
    let path = std::env::temp_dir();
    let file_name = random_string(16);
    path.join(file_name + ".log")
}

pub fn temp_dir() -> PathBuf {
    let path = std::env::temp_dir();
    let dir_name = random_string(16);
    path.join(dir_name)
}

pub fn random_lines_with_stream(
    len: usize,
    count: usize,
) -> (Vec<String>, impl Stream<Item = Result<Event, ()>>) {
    let lines = (0..count).map(|_| random_string(len)).collect::<Vec<_>>();
    let stream = stream::iter(lines.clone()).map(Event::from).map(Ok);
    (lines, stream)
}

fn random_events_with_stream_generic<F>(
    count: usize,
    generator: F,
) -> (Vec<Event>, impl Stream<Item = Result<Event, ()>>)
where
    F: Fn() -> Event,
{
    let events = (0..count).map(|_| generator()).collect::<Vec<_>>();
    let stream = stream::iter(events.clone()).map(Ok);
    (events, stream)
}

pub fn random_events_with_stream(
    len: usize,
    count: usize,
) -> (Vec<Event>, impl Stream<Item = Result<Event, ()>>) {
    random_events_with_stream_generic(count, move || Event::from(random_string(len)))
}

pub fn random_string(len: usize) -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .collect::<String>()
}

pub fn random_lines(len: usize) -> impl Iterator<Item = String> {
    std::iter::repeat(()).map(move |_| random_string(len))
}

pub fn random_map(max_size: usize, field_len: usize) -> HashMap<String, String> {
    let size = thread_rng().gen_range(0, max_size);

    (0..size)
        .map(move |_| (random_string(field_len), random_string(field_len)))
        .collect()
}

pub fn random_maps(
    max_size: usize,
    field_len: usize,
) -> impl Iterator<Item = HashMap<String, String>> {
    iter::repeat(()).map(move |_| random_map(max_size, field_len))
}

pub async fn collect_n<T>(rx: mpsc::Receiver<T>, n: usize) -> Result<Vec<T>, ()> {
    rx.compat().take(n).try_collect().await
}

pub async fn collect_ready<S>(rx: S) -> Result<Vec<S::Item>, ()>
where
    S: Stream01<Item = Event, Error = ()>,
{
    let mut rx = rx.compat();

    let waker = noop_waker_ref();
    let mut cx = Context::from_waker(waker);

    let mut vec = Vec::new();
    loop {
        match rx.poll_next_unpin(&mut cx) {
            Poll::Ready(Some(Ok(item))) => vec.push(item),
            Poll::Ready(Some(Err(()))) => return Err(()),
            Poll::Ready(None) | Poll::Pending => return Ok(vec),
        }
    }
}

pub fn lines_from_file<P: AsRef<Path>>(path: P) -> Vec<String> {
    trace!(message = "Reading file.", path = %path.as_ref().display());
    let mut file = File::open(path).unwrap();
    let mut output = String::new();
    file.read_to_string(&mut output).unwrap();
    output.lines().map(|s| s.to_owned()).collect()
}

pub fn lines_from_gzip_file<P: AsRef<Path>>(path: P) -> Vec<String> {
    trace!(message = "Reading gzip file.", path = %path.as_ref().display());
    let mut file = File::open(path).unwrap();
    let mut gzip_bytes = Vec::new();
    file.read_to_end(&mut gzip_bytes).unwrap();
    let mut output = String::new();
    GzDecoder::new(&gzip_bytes[..])
        .read_to_string(&mut output)
        .unwrap();
    output.lines().map(|s| s.to_owned()).collect()
}

pub fn runtime() -> runtime::Runtime {
    runtime::Builder::new()
        .threaded_scheduler()
        .enable_all()
        .build()
        .unwrap()
}

pub async fn wait_for<F, Fut>(mut f: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool> + Send + 'static,
{
    let started = Instant::now();
    while !f().await {
        delay_for(Duration::from_millis(5)).await;
        if started.elapsed().as_secs() > 5 {
            panic!("Timed out while waiting");
        }
    }
}

pub async fn wait_for_tcp(addr: SocketAddr) {
    wait_for(|| async move { TcpStream::connect(addr).await.is_ok() }).await
}

pub async fn wait_for_atomic_usize<T, F>(value: T, unblock: F)
where
    T: AsRef<AtomicUsize>,
    F: Fn(usize) -> bool,
{
    let value = value.as_ref();
    wait_for(|| {
        let result = unblock(value.load(Ordering::SeqCst));
        future::ready(result)
    })
    .await
}

pub struct CountReceiver<T> {
    count: Arc<AtomicUsize>,
    trigger: Option<oneshot::Sender<()>>,
    connected: Option<oneshot::Receiver<()>>,
    handle: JoinHandle<Vec<T>>,
}

impl<T: Send + 'static> CountReceiver<T> {
    pub fn count(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }

    /// Succeds once first connection has been made.
    pub async fn connected(&mut self) {
        if let Some(tripwire) = self.connected.take() {
            tripwire.await.unwrap();
        }
    }

    fn new<F, Fut>(make_fut: F) -> CountReceiver<T>
    where
        F: FnOnce(Arc<AtomicUsize>, oneshot::Receiver<()>, oneshot::Sender<()>) -> Fut,
        Fut: Future<Output = Vec<T>> + Send + 'static,
    {
        let count = Arc::new(AtomicUsize::new(0));
        let (trigger, tripwire) = oneshot::channel();
        let (trigger_connected, connected) = oneshot::channel();

        CountReceiver {
            count: Arc::clone(&count),
            trigger: Some(trigger),
            connected: Some(connected),
            handle: tokio::spawn(make_fut(count, tripwire, trigger_connected)),
        }
    }
}

impl<T> Future for CountReceiver<T> {
    type Output = Vec<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if let Some(trigger) = this.trigger.take() {
            let _ = trigger.send(());
        }

        let result = ready!(this.handle.poll_unpin(cx));
        Poll::Ready(result.unwrap())
    }
}

impl CountReceiver<String> {
    pub fn receive_lines(addr: SocketAddr) -> CountReceiver<String> {
        CountReceiver::new(|count, tripwire, connected| async move {
            let mut listener = TcpListener::bind(addr).await.unwrap();
            CountReceiver::receive_lines_stream(
                listener.incoming(),
                count,
                tripwire,
                Some(connected),
            )
            .await
        })
    }

    #[cfg(unix)]
    pub fn receive_lines_unix<P>(path: P) -> CountReceiver<String>
    where
        P: AsRef<Path> + Send + 'static,
    {
        CountReceiver::new(|count, tripwire, connected| async move {
            let mut listener = tokio::net::UnixListener::bind(path).unwrap();
            CountReceiver::receive_lines_stream(
                listener.incoming(),
                count,
                tripwire,
                Some(connected),
            )
            .await
        })
    }

    async fn receive_lines_stream<S, T>(
        stream: S,
        count: Arc<AtomicUsize>,
        tripwire: oneshot::Receiver<()>,
        mut connected: Option<oneshot::Sender<()>>,
    ) -> Vec<String>
    where
        S: Stream<Item = IoResult<T>>,
        T: AsyncWrite + AsyncRead,
    {
        stream
            .take_until(tripwire)
            .map_ok(|socket| FramedRead::new(socket, LinesCodec::new()))
            .map(|x| {
                connected.take().map(|trigger| trigger.send(()));
                x.unwrap()
            })
            .flatten()
            .map(|x| x.unwrap())
            .inspect(move |_| {
                count.fetch_add(1, Ordering::Relaxed);
            })
            .collect::<Vec<String>>()
            .await
    }
}

impl CountReceiver<Event> {
    pub fn receive_events<S>(stream: S) -> CountReceiver<Event>
    where
        S: Stream01<Item = Event> + Send + 'static,
        <S as Stream01>::Error: std::fmt::Debug,
    {
        CountReceiver::new(|count, tripwire, connected| async move {
            connected.send(()).unwrap();
            stream
                .compat()
                .take_until(tripwire)
                .map(|x| x.unwrap())
                .inspect(move |_| {
                    count.fetch_add(1, Ordering::Relaxed);
                })
                .collect::<Vec<Event>>()
                .await
        })
    }
}

pub async fn start_topology(
    config: Config,
    require_healthy: bool,
) -> (RunningTopology, mpsc::UnboundedReceiver<()>) {
    let diff = ConfigDiff::initial(&config);
    let pieces = topology::validate(&config, &diff).await.unwrap();
    topology::start_validated(config, diff, pieces, require_healthy)
        .await
        .unwrap()
}
