//! iroh transport between Utre devices.
//!
//! - [`replicate`] (M3): pairing, trusted peers and op-log sync on ALPN `tickalt/sync/1` (kept after the rename to Utre, so older builds can still pair).
//! - The Spike 2 protocol (`tickalt/spike/0`, a pull of open tasks by `tt1…` ticket) is
//!   still served for the app's spike Sync tab and `utre sync spike-pull`.

pub mod replicate;
pub mod service;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use iroh::endpoint::{Connection, presets};
use iroh::protocol::{AcceptError, ProtocolHandler, Router};
use iroh::{Endpoint, EndpointAddr, RelayMode, SecretKey, TransportAddr};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use utre_core::Store;

/// Application protocol id. Bump it whenever the wire format changes.
pub const ALPN: &[u8] = b"tickalt/spike/0";
const TICKET_PREFIX: &str = "tt1";
const MAX_MSG: usize = 1 << 20;
/// How long the client waits for hole punching to upgrade a relay path to a direct one.
const DIRECT_WAIT: Duration = Duration::from_secs(4);
/// The UDP port a node prefers (`$UTRE_SYNC_PORT` overrides; 0 means random).
pub const DEFAULT_SYNC_PORT: u16 = 41824;

fn sync_port() -> u16 {
    utre_core::envvar::var("SYNC_PORT")
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_SYNC_PORT)
}

/// How long `sync_with` waits for a peer to answer before giving up on it.
const SYNC_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// What the dialing side sends first.
#[derive(Debug, Serialize, Deserialize)]
struct Hello {
    device: String,
}

/// What the serving side answers with.
#[derive(Debug, Serialize, Deserialize)]
struct Reply {
    device: String,
    tasks: Vec<WireTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WireTask {
    pub id: String,
    pub title: String,
}

/// How the connection reached the peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    Direct,
    Relay,
    Unknown,
}

impl PathKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PathKind::Direct => "direct",
            PathKind::Relay => "relay",
            PathKind::Unknown => "unknown",
        }
    }
}

/// The result of one pull.
#[derive(Debug, Clone)]
pub struct PullReport {
    pub peer_device: String,
    pub peer_id: String,
    pub tasks: Vec<WireTask>,
    /// Time from dialing to having the reply.
    pub elapsed: Duration,
    /// Selected path when the reply arrived.
    pub first_path: PathKind,
    /// Selected path after waiting up to [`DIRECT_WAIT`] for hole punching.
    pub final_path: PathKind,
    pub rtt: Option<Duration>,
    /// Every open path at the end, e.g. `ip 192.168.1.5:5123 (selected)`.
    pub paths: Vec<String>,
}

/// A connection the node served.
#[derive(Debug, Clone)]
pub struct Served {
    pub peer_device: String,
    pub peer_id: String,
    pub task_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relays {
    /// n0's public relays plus n0 DNS address lookup: works across networks.
    N0,
    /// No relays and no address lookup: only addresses in the ticket are tried.
    Off,
}

pub struct SyncNode {
    router: Router,
    device: String,
    replicator: replicate::Replicator,
    served_log: Arc<Mutex<Vec<Served>>>,
    served_tx: broadcast::Sender<Served>,
}

impl SyncNode {
    /// Bind an endpoint whose identity is the key in `key_path` (created if missing),
    /// and serve the open tasks from the database at `db_path`.
    pub async fn start(
        db_path: PathBuf,
        key_path: &Path,
        device: String,
        relays: Relays,
    ) -> Result<Self> {
        let key = load_or_create_key(key_path)?;
        let builder = || match relays {
            Relays::N0 => Endpoint::builder(presets::N0),
            Relays::Off => Endpoint::builder(presets::Minimal).relay_mode(RelayMode::Disabled),
        };
        // A fixed UDP port lets paired devices on the LAN reach this one again after a
        // restart even without relays (they dial the address they stored). If it's taken
        // (another node runs here), fall back to a random port; relays still find us.
        let port = sync_port();
        let fixed = match port {
            0 => None,
            p => match builder().bind_addr(format!("0.0.0.0:{p}")) {
                Ok(b) => b.secret_key(key.clone()).bind().await.ok(),
                Err(_) => None,
            },
        };
        let endpoint = match fixed {
            Some(e) => e,
            None => builder()
                .secret_key(key)
                .bind()
                .await
                .context("binding iroh endpoint")?,
        };

        let served_log = Arc::new(Mutex::new(Vec::new()));
        let (served_tx, _) = broadcast::channel(32);
        let handler = TaskServer {
            db_path,
            device: device.clone(),
            log: served_log.clone(),
            tx: served_tx.clone(),
        };
        let replicator = replicate::Replicator::new(
            replicate::FileReplica::new(handler.db_path.clone()),
            device.clone(),
            endpoint.id().to_string(),
        );
        let router = Router::builder(endpoint)
            .accept(ALPN, handler)
            .accept(replicate::SYNC_ALPN, replicator.clone())
            .spawn();
        Ok(Self {
            router,
            device,
            replicator,
            served_log,
            served_tx,
        })
    }

    fn endpoint(&self) -> &Endpoint {
        self.router.endpoint()
    }

    pub fn id(&self) -> String {
        self.endpoint().id().to_string()
    }

    /// Wait until a home relay is connected (so the ticket carries a relay URL), up to `timeout`.
    /// Returns false on timeout; the endpoint still works on direct addresses.
    pub async fn wait_online(&self, timeout: Duration) -> bool {
        tokio::time::timeout(timeout, self.endpoint().online())
            .await
            .is_ok()
    }

    /// A pasteable address: `tt1` + hex of the endpoint address as JSON.
    pub fn ticket(&self) -> Result<String> {
        encode_ticket(&self.endpoint().addr())
    }

    /// Addresses currently in the ticket, for display.
    pub fn addrs(&self) -> Vec<String> {
        self.endpoint()
            .addr()
            .addrs
            .iter()
            .map(describe_addr)
            .collect()
    }

    /// Connections served so far (newest last).
    pub fn served(&self) -> Vec<Served> {
        self.served_log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn subscribe_served(&self) -> broadcast::Receiver<Served> {
        self.served_tx.subscribe()
    }

    /// Dial the peer in `ticket` and fetch its open tasks.
    pub async fn pull(&self, ticket: &str) -> Result<PullReport> {
        let addr = decode_ticket(ticket)?;
        let started = Instant::now();
        let conn = self
            .endpoint()
            .connect(addr, ALPN)
            .await
            .context("connecting to peer")?;

        let (mut send, mut recv) = conn.open_bi().await.context("opening stream")?;
        let hello = serde_json::to_vec(&Hello {
            device: self.device.clone(),
        })?;
        send.write_all(&hello).await.context("sending hello")?;
        send.finish().context("finishing hello")?;
        let bytes = recv.read_to_end(MAX_MSG).await.context("reading reply")?;
        let reply: Reply = serde_json::from_slice(&bytes).context("decoding reply")?;
        let elapsed = started.elapsed();

        let first_path = selected_path(&conn).0;
        let deadline = Instant::now() + DIRECT_WAIT;
        while selected_path(&conn).0 != PathKind::Direct && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let (final_path, rtt) = selected_path(&conn);
        let paths = describe_paths(&conn);
        conn.close(0u32.into(), b"done");

        Ok(PullReport {
            peer_device: reply.device,
            peer_id: conn.remote_id().to_string(),
            tasks: reply.tasks,
            elapsed,
            first_path,
            final_path,
            rtt,
            paths,
        })
    }

    // ----- M3 sync -----

    /// A one-time pairing ticket (`tt2…`) for this node, valid for 5 minutes.
    pub fn pairing_ticket(&self) -> Result<String> {
        self.pairing_ticket_at(&self.endpoint().addr())
    }

    /// The same, advertising `addr` (tests use a loopback address).
    pub fn pairing_ticket_at(&self, addr: &EndpointAddr) -> Result<String> {
        self.replicator.offer_pairing(addr)
    }

    /// Pair with the device that made `ticket`, then sync with it.
    pub async fn join(&self, ticket: &str) -> Result<replicate::SyncReport> {
        let t = replicate::decode_pair_ticket(ticket)?;
        let conn = self
            .endpoint()
            .connect(t.addr.clone(), replicate::SYNC_ALPN)
            .await
            .with_context(|| format!("connecting to {}", t.name))?;
        self.replicator
            .client_session(&conn, Some(self.addr_json()?), Some((t.secret, t.addr)))
            .await
    }

    /// Sync with one trusted peer (by node id).
    pub async fn sync_with(&self, peer: &utre_core::Peer) -> Result<replicate::SyncReport> {
        let addr: EndpointAddr = match &peer.last_addr {
            Some(json) => serde_json::from_str(json).context("stored peer address")?,
            None => EndpointAddr::new(peer.node_id.parse().context("peer id")?),
        };
        let conn = tokio::time::timeout(
            SYNC_CONNECT_TIMEOUT,
            self.endpoint().connect(addr, replicate::SYNC_ALPN),
        )
        .await
        .with_context(|| format!("{} didn't answer", peer.name))?
        .with_context(|| format!("connecting to {}", peer.name))?;
        self.replicator
            .client_session(&conn, Some(self.addr_json()?), None)
            .await
    }

    /// Sync with every trusted peer, one after another.
    pub async fn sync_all(&self) -> Vec<(utre_core::Peer, Result<replicate::SyncReport>)> {
        let peers = match self.replicator.replica.peers().await {
            Ok(p) => p,
            Err(_) => return Vec::new(),
        };
        let mut out = Vec::new();
        for peer in peers {
            let r = self.sync_with(&peer).await;
            out.push((peer, r));
        }
        out
    }

    pub async fn peers(&self) -> Result<Vec<utre_core::Peer>> {
        self.replicator.replica.peers().await
    }

    /// Pairing, sync and refusal events (the daemon's tray shows them).
    pub fn subscribe_sync(&self) -> broadcast::Receiver<replicate::SyncEvent> {
        self.replicator.subscribe()
    }

    fn addr_json(&self) -> Result<String> {
        Ok(serde_json::to_string(&self.endpoint().addr())?)
    }

    pub async fn shutdown(self) {
        if let Err(e) = self.router.shutdown().await {
            eprintln!("router shutdown: {e}");
        }
    }
}

#[derive(Debug, Clone)]
struct TaskServer {
    db_path: PathBuf,
    device: String,
    log: Arc<Mutex<Vec<Served>>>,
    tx: broadcast::Sender<Served>,
}

impl TaskServer {
    async fn serve(&self, conn: &Connection) -> Result<Served> {
        let (mut send, mut recv) = conn.accept_bi().await?;
        let hello: Hello = serde_json::from_slice(&recv.read_to_end(MAX_MSG).await?)?;

        // Each connection opens its own SQLite handle; WAL lets it read beside the app's.
        let db_path = self.db_path.clone();
        let tasks = tokio::task::spawn_blocking(move || -> Result<Vec<WireTask>> {
            let store = Store::open(&db_path)?;
            Ok(store
                .open_tasks()?
                .into_iter()
                .map(|t| WireTask {
                    id: t.id,
                    title: t.title,
                })
                .collect())
        })
        .await??;

        let served = Served {
            peer_device: hello.device,
            peer_id: conn.remote_id().to_string(),
            task_count: tasks.len(),
        };
        let reply = serde_json::to_vec(&Reply {
            device: self.device.clone(),
            tasks,
        })?;
        send.write_all(&reply).await?;
        send.finish()?;
        Ok(served)
    }
}

impl ProtocolHandler for TaskServer {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        let served = self
            .serve(&conn)
            .await
            .map_err(|e| AcceptError::from_boxed(e.into()))?;
        self.log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(served.clone());
        let _ = self.tx.send(served);
        // The client closes once it has measured the path.
        conn.closed().await;
        Ok(())
    }
}

fn selected_path(conn: &Connection) -> (PathKind, Option<Duration>) {
    let paths = conn.paths();
    for p in paths.iter() {
        if p.is_selected() {
            let kind = if p.is_relay() {
                PathKind::Relay
            } else if p.is_ip() {
                PathKind::Direct
            } else {
                PathKind::Unknown
            };
            return (kind, Some(p.rtt()));
        }
    }
    (PathKind::Unknown, None)
}

fn describe_paths(conn: &Connection) -> Vec<String> {
    let paths = conn.paths();
    paths
        .iter()
        .map(|p| {
            let mark = if p.is_selected() { " (selected)" } else { "" };
            format!(
                "{}, rtt {} ms{mark}",
                describe_addr(p.remote_addr()),
                p.rtt().as_millis()
            )
        })
        .collect()
}

fn describe_addr(addr: &TransportAddr) -> String {
    match addr {
        TransportAddr::Ip(a) => format!("ip {a}"),
        TransportAddr::Relay(u) => format!("relay {u}"),
        other => format!("{other:?}"),
    }
}

fn load_or_create_key(path: &Path) -> Result<SecretKey> {
    let read = |path: &Path| -> Result<SecretKey> {
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let arr: [u8; 32] = bytes
            .as_slice()
            .try_into()
            .with_context(|| format!("{} is not a 32-byte key", path.display()))?;
        Ok(SecretKey::from_bytes(&arr))
    };
    match std::fs::metadata(path) {
        Ok(_) => return read(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let key = SecretKey::generate();
    // Write the whole key to a private temp file first, then publish it under its final
    // name in one step, so another process never sees a half-written (empty) key.
    let tmp = path.with_extension(format!(
        "key-tmp-{}-{:x}",
        std::process::id(),
        rand_suffix()
    ));
    write_private(&tmp, &key.to_bytes())?;
    let published = publish_new(&tmp, path);
    let _ = std::fs::remove_file(&tmp);
    match published {
        Ok(()) => Ok(key),
        // Someone else created it first: theirs is the key.
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => read(path),
        Err(e) => Err(e).with_context(|| format!("creating {}", path.display())),
    }
}

fn rand_suffix() -> u64 {
    let mut b = [0u8; 8];
    let _ = getrandom::fill(&mut b);
    u64::from_le_bytes(b)
}

/// Give `tmp`'s contents the name `to`, failing with `AlreadyExists` if `to` exists.
fn publish_new(tmp: &Path, to: &Path) -> std::io::Result<()> {
    publish_chain(tmp, to, |a, b| std::fs::hard_link(a, b), rename_noreplace)
}

/// Errors that mean "this mechanism is not allowed or not supported here", as opposed to a
/// real answer such as `AlreadyExists`.
fn is_unsupported(e: &std::io::Error) -> bool {
    if e.kind() == std::io::ErrorKind::AlreadyExists {
        return false;
    }
    #[cfg(unix)]
    if let Some(code) = e.raw_os_error() {
        return matches!(
            code,
            libc::EACCES | libc::EPERM | libc::ENOTSUP | libc::EINVAL | libc::ENOSYS
        ) || code == libc::EOPNOTSUPP;
    }
    e.kind() == std::io::ErrorKind::PermissionDenied || e.kind() == std::io::ErrorKind::Unsupported
}

/// The fallback chain behind `publish_new`, with the two syscalls injectable for tests.
///
/// 1. `link` (a hard link): atomic and never clobbers. Used on Linux and macOS.
/// 2. Android's SELinux policy denies `link` to untrusted apps (`avc: denied { link }`,
///    surfacing as EACCES), so fall back to `rename_noreplace` (renameat2 with
///    RENAME_NOREPLACE): equally atomic and no-clobber.
/// 3. Old kernels or filesystems may not offer that either (EINVAL/ENOSYS/EPERM/EACCES), so
///    last, check that `to` is absent and plain `rename`. The only race is two processes
///    creating the very first key at the same instant; on Android all writers share one
///    process and one sync node.
fn publish_chain(
    tmp: &Path,
    to: &Path,
    link: impl Fn(&Path, &Path) -> std::io::Result<()>,
    noreplace: impl Fn(&Path, &Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    match link(tmp, to) {
        Err(e) if is_unsupported(&e) => {}
        other => return other,
    }
    match noreplace(tmp, to) {
        Err(e) if is_unsupported(&e) => {}
        other => return other,
    }
    if to.symlink_metadata().is_ok() {
        return Err(std::io::ErrorKind::AlreadyExists.into());
    }
    std::fs::rename(tmp, to)
}

/// Atomic rename that fails with `AlreadyExists` instead of replacing `to`.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn rename_noreplace(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let cstr = |p: &Path| {
        std::ffi::CString::new(p.as_os_str().as_bytes())
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))
    };
    let (f, t) = (cstr(from)?, cstr(to)?);
    const RENAME_NOREPLACE: libc::c_uint = 1;
    // The raw syscall, because bionic only wraps renameat2 from API 30.
    let r = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            f.as_ptr(),
            libc::AT_FDCWD,
            t.as_ptr(),
            RENAME_NOREPLACE,
        )
    };
    if r == 0 {
        return Ok(());
    }
    let e = std::io::Error::last_os_error();
    if e.raw_os_error() == Some(libc::EEXIST) {
        return Err(std::io::ErrorKind::AlreadyExists.into());
    }
    Err(e)
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn rename_noreplace(_: &Path, _: &Path) -> std::io::Result<()> {
    Err(std::io::ErrorKind::Unsupported.into())
}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("creating {}", path.display()))?;
    f.write_all(bytes)?;
    f.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::write(path, bytes).with_context(|| format!("creating {}", path.display()))
}

pub fn encode_ticket(addr: &EndpointAddr) -> Result<String> {
    let json = serde_json::to_vec(addr)?;
    let mut s = String::with_capacity(TICKET_PREFIX.len() + json.len() * 2);
    s.push_str(TICKET_PREFIX);
    for b in json {
        s.push_str(&format!("{b:02x}"));
    }
    Ok(s)
}

pub fn decode_ticket(ticket: &str) -> Result<EndpointAddr> {
    let hex = ticket
        .trim()
        .strip_prefix(TICKET_PREFIX)
        .context("not a Utre ticket (expected tt1…)")?;
    if !hex.is_ascii() {
        bail!("ticket is not hex");
    }
    if hex.len() % 2 != 0 {
        bail!("ticket has odd length; was it cut off?");
    }
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
        .collect::<Result<Vec<u8>, _>>()
        .context("ticket is not hex")?;
    serde_json::from_slice(&bytes).context("ticket does not hold an endpoint address")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("utre-sync-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn racing_loaders_agree_and_never_see_a_partial_key() {
        let dir = temp_dir("keyrace");
        for round in 0..30 {
            let path = dir.join(format!("k{round}"));
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
            let handles: Vec<_> = (0..4)
                .map(|_| {
                    let (path, barrier) = (path.clone(), barrier.clone());
                    std::thread::spawn(move || {
                        barrier.wait();
                        load_or_create_key(&path).unwrap().to_bytes()
                    })
                })
                .collect();
            let keys: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
            assert!(keys.windows(2).all(|w| w[0] == w[1]), "round {round}");
            assert_eq!(std::fs::read(&path).unwrap(), keys[0]);
        }
        let leftovers = std::fs::read_dir(&dir).unwrap().count();
        assert_eq!(leftovers, 30, "no temp files left behind");
    }

    fn eacces() -> std::io::Error {
        std::io::Error::from_raw_os_error(libc::EACCES)
    }
    fn no_link(_: &Path, _: &Path) -> std::io::Result<()> {
        Err(eacces())
    }
    fn no_noreplace(_: &Path, _: &Path) -> std::io::Result<()> {
        Err(std::io::Error::from_raw_os_error(libc::ENOSYS))
    }
    fn staged(name: &str) -> (PathBuf, PathBuf) {
        let dir = temp_dir(name);
        let tmp = dir.join("k-tmp");
        write_private(&tmp, &[7u8; 32]).unwrap();
        (tmp, dir.join("k"))
    }
    fn check_published(to: &Path) {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::read(to).unwrap(), [7u8; 32]);
        assert_eq!(
            std::fs::metadata(to).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn publish_hard_link_first() {
        let (tmp, to) = staged("pub-link");
        let no = |_: &Path, _: &Path| -> std::io::Result<()> { panic!("not reached") };
        publish_chain(&tmp, &to, |a, b| std::fs::hard_link(a, b), no).unwrap();
        check_published(&to);
    }

    #[test]
    fn publish_link_denied_uses_renameat2() {
        let (tmp, to) = staged("pub-r2");
        publish_chain(&tmp, &to, no_link, rename_noreplace).unwrap();
        check_published(&to);
        assert!(!tmp.exists());
    }

    #[test]
    fn publish_last_fallback_plain_rename() {
        let (tmp, to) = staged("pub-last");
        publish_chain(&tmp, &to, no_link, no_noreplace).unwrap();
        check_published(&to);
    }

    #[test]
    fn publish_existing_target_is_already_exists_everywhere() {
        let kind = std::io::ErrorKind::AlreadyExists;
        let (tmp, to) = staged("pub-exists");
        std::fs::write(&to, b"other").unwrap();
        let hl = |a: &Path, b: &Path| std::fs::hard_link(a, b);
        assert_eq!(
            publish_chain(&tmp, &to, hl, rename_noreplace)
                .unwrap_err()
                .kind(),
            kind
        );
        assert_eq!(
            publish_chain(&tmp, &to, no_link, rename_noreplace)
                .unwrap_err()
                .kind(),
            kind
        );
        assert_eq!(
            publish_chain(&tmp, &to, no_link, no_noreplace)
                .unwrap_err()
                .kind(),
            kind
        );
        assert_eq!(std::fs::read(&to).unwrap(), b"other");
    }

    #[test]
    fn key_is_persistent() {
        let dir = temp_dir("key");
        let a = load_or_create_key(&dir.join("k")).unwrap();
        let b = load_or_create_key(&dir.join("k")).unwrap();
        assert_eq!(a.public(), b.public());
    }

    #[test]
    fn ticket_round_trips() {
        let id = SecretKey::generate().public();
        let addr = EndpointAddr::new(id).with_ip_addr("192.168.1.5:4433".parse().unwrap());
        assert_eq!(decode_ticket(&encode_ticket(&addr).unwrap()).unwrap(), addr);
        assert!(decode_ticket("nope").is_err());
        // Pasted text with multi-byte characters must be an error, not a slicing panic.
        assert!(decode_ticket("tt1é1").is_err());
        assert!(decode_ticket("tt1ééé").is_err());
        assert!(replicate::decode_pair_ticket("tt2日本語日本語").is_err());
        assert!(replicate::decode_pair_ticket("tt2a€").is_err());
    }

    /// Two nodes on loopback, no relays: B pulls A's open tasks.
    #[tokio::test]
    async fn pull_over_loopback() {
        let dir = temp_dir("pull");
        let db_a = dir.join("a.db");
        {
            let store = Store::open(&db_a).unwrap();
            store.add_task("Buy milk").unwrap();
            store.add_task("Renew passport").unwrap();
        }
        let a = SyncNode::start(db_a, &dir.join("a.key"), "mac".into(), Relays::Off)
            .await
            .unwrap();
        let b = SyncNode::start(
            dir.join("b.db"),
            &dir.join("b.key"),
            "phone".into(),
            Relays::Off,
        )
        .await
        .unwrap();
        let mut served = a.subscribe_served();

        // Dial loopback only, so the test doesn't depend on the LAN, VPNs or firewalls.
        let port = a.endpoint().bound_sockets()[0].port();
        let local = EndpointAddr::new(a.endpoint().id())
            .with_ip_addr(std::net::SocketAddr::from(([127, 0, 0, 1], port)));
        let report = b.pull(&encode_ticket(&local).unwrap()).await.unwrap();
        let mut titles: Vec<_> = report.tasks.iter().map(|t| t.title.as_str()).collect();
        titles.sort();
        assert_eq!(titles, ["Buy milk", "Renew passport"]);
        assert_eq!(report.peer_device, "mac");
        assert_eq!(report.peer_id, a.id());
        assert_eq!(report.final_path, PathKind::Direct);

        let s = tokio::time::timeout(Duration::from_secs(5), served.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(s.peer_device, "phone");
        assert_eq!(s.peer_id, b.id());
        assert_eq!(s.task_count, 2);

        a.shutdown().await;
        b.shutdown().await;
    }

    // ----- M3 sync -----

    async fn node(dir: &Path, name: &str) -> SyncNode {
        SyncNode::start(
            dir.join(format!("{name}.db")),
            &dir.join(format!("{name}.key")),
            name.into(),
            Relays::Off,
        )
        .await
        .unwrap()
    }

    fn loopback(n: &SyncNode) -> EndpointAddr {
        let port = n.endpoint().bound_sockets()[0].port();
        EndpointAddr::new(n.endpoint().id())
            .with_ip_addr(std::net::SocketAddr::from(([127, 0, 0, 1], port)))
    }

    fn store(dir: &Path, name: &str) -> Store {
        Store::open(dir.join(format!("{name}.db"))).unwrap()
    }

    /// Point a peer's stored address at loopback (what pairing stored may be a LAN one).
    fn pin_loopback(dir: &Path, owner: &str, peer: &SyncNode) {
        let s = store(dir, owner);
        let p = s.peer(&peer.id()).unwrap().unwrap();
        let addr = serde_json::to_string(&loopback(peer)).unwrap();
        s.add_peer(&p.node_id, &p.name, Some(&addr), 0).unwrap();
    }

    #[tokio::test]
    async fn pair_then_sync_converges() {
        let dir = temp_dir("pair");
        store(&dir, "mac").add_task("on the mac").unwrap();
        let t = store(&dir, "arch").add_task("on arch").unwrap();
        store(&dir, "arch")
            .tag_task(&t.id, &store(&dir, "arch").ensure_tag_path("home").unwrap())
            .unwrap();
        let (mac, arch) = (node(&dir, "mac").await, node(&dir, "arch").await);

        let ticket = mac.pairing_ticket_at(&loopback(&mac)).unwrap();
        assert!(ticket.starts_with("tt2"));
        let report = arch.join(&ticket).await.unwrap();
        assert_eq!(report.peer_name, "mac");
        assert!(report.sent > 0 && report.received > 0);

        // Both trust each other now, and hold the same data.
        assert!(store(&dir, "mac").is_trusted(&arch.id()).unwrap());
        assert!(store(&dir, "arch").is_trusted(&mac.id()).unwrap());
        assert_eq!(
            store(&dir, "mac").state_dump().unwrap(),
            store(&dir, "arch").state_dump().unwrap()
        );

        // Later edits on both sides meet on the next sync, either direction.
        store(&dir, "mac")
            .set_title(&t.id, "renamed on the mac")
            .unwrap();
        store(&dir, "arch").add_task("another on arch").unwrap();
        pin_loopback(&dir, "mac", &arch);
        let peer = store(&dir, "mac").peer(&arch.id()).unwrap().unwrap();
        let again = mac.sync_with(&peer).await.unwrap();
        assert_eq!((again.sent > 0, again.received > 0), (true, true));
        let (a, b) = (store(&dir, "mac"), store(&dir, "arch"));
        assert_eq!(a.state_dump().unwrap(), b.state_dump().unwrap());
        assert_eq!(
            b.get_task(&t.id).unwrap().unwrap().title,
            "renamed on the mac"
        );
        assert!(b.peer(&mac.id()).unwrap().unwrap().last_seen.is_some());

        // A sync with nothing new moves nothing.
        let quiet = mac.sync_with(&peer).await.unwrap();
        assert_eq!((quiet.sent, quiet.received), (0, 0));
    }

    #[tokio::test]
    async fn unpaired_devices_are_refused() {
        let dir = temp_dir("refuse");
        let (mac, stranger) = (node(&dir, "mac").await, node(&dir, "stranger").await);
        store(&dir, "mac").add_task("private").unwrap();
        let mut events = mac.subscribe_sync();
        let fake = utre_core::Peer {
            node_id: mac.id(),
            name: "mac".into(),
            added_at: 0,
            last_seen: None,
            last_addr: Some(serde_json::to_string(&loopback(&mac)).unwrap()),
        };
        let err = stranger.sync_with(&fake).await.unwrap_err();
        assert!(format!("{err:#}").contains("isn't paired"), "{err:#}");
        assert!(store(&dir, "stranger").open_tasks().unwrap().is_empty());
        let ev = tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(ev, replicate::SyncEvent::Refused { .. }));
    }

    #[tokio::test]
    async fn a_pairing_ticket_works_once() {
        let dir = temp_dir("once");
        let (mac, a, b) = (
            node(&dir, "mac").await,
            node(&dir, "a").await,
            node(&dir, "b").await,
        );
        let ticket = mac.pairing_ticket_at(&loopback(&mac)).unwrap();
        a.join(&ticket).await.unwrap();
        let err = b.join(&ticket).await.unwrap_err();
        assert!(format!("{err:#}").contains("pairing refused"), "{err:#}");
        assert!(!store(&dir, "mac").is_trusted(&b.id()).unwrap());
    }

    #[tokio::test]
    async fn a_big_sync_streams_in_batches_both_ways() {
        let dir = temp_dir("big");
        {
            let (a, b) = (store(&dir, "a"), store(&dir, "b"));
            for i in 0..700 {
                a.add_task(&format!("a{i}")).unwrap();
                b.add_task(&format!("b{i}")).unwrap();
            }
        }
        let (na, nb) = (node(&dir, "a").await, node(&dir, "b").await);
        let report = nb
            .join(&na.pairing_ticket_at(&loopback(&na)).unwrap())
            .await
            .unwrap();
        assert!(report.sent > 1000 && report.received > 1000, "{report:?}");
        assert_eq!(store(&dir, "a").open_tasks().unwrap().len(), 1400);
        assert_eq!(
            store(&dir, "a").state_dump().unwrap(),
            store(&dir, "b").state_dump().unwrap()
        );
    }

    #[tokio::test]
    async fn trust_and_forgetting_spread_through_a_shared_peer() {
        let dir = temp_dir("trust");
        let (a, b, c) = (
            node(&dir, "a").await,
            node(&dir, "b").await,
            node(&dir, "c").await,
        );
        store(&dir, "c").add_task("from c").unwrap();

        // B and C each pair with A only. C joins second, so A already trusts B and tells C.
        b.join(&a.pairing_ticket_at(&loopback(&a)).unwrap())
            .await
            .unwrap();
        let report = c
            .join(&a.pairing_ticket_at(&loopback(&a)).unwrap())
            .await
            .unwrap();
        assert_eq!(report.learned, ["b"]);
        assert!(store(&dir, "c").is_trusted(&b.id()).unwrap());

        // B syncs with A and learns about C.
        pin_loopback(&dir, "b", &a);
        let peer_a = store(&dir, "b").peer(&a.id()).unwrap().unwrap();
        let r = b.sync_with(&peer_a).await.unwrap();
        assert_eq!(r.learned, ["c"]);

        // So B and C sync directly, though they never paired with each other.
        pin_loopback(&dir, "b", &c);
        let peer_c = store(&dir, "b").peer(&c.id()).unwrap().unwrap();
        b.sync_with(&peer_c).await.unwrap();
        assert_eq!(
            store(&dir, "b").state_dump().unwrap(),
            store(&dir, "c").state_dump().unwrap()
        );

        // A forgets C; at B's next sync with A, B forgets C too, and C is refused.
        assert!(store(&dir, "a").remove_peer(&c.id()).unwrap());
        let r = b.sync_with(&peer_a).await.unwrap();
        assert_eq!(r.dropped, ["c"]);
        assert!(!store(&dir, "b").is_trusted(&c.id()).unwrap());
        pin_loopback(&dir, "c", &b);
        let peer_b = store(&dir, "c").peer(&b.id()).unwrap().unwrap();
        let err = c.sync_with(&peer_b).await.unwrap_err();
        assert!(format!("{err:#}").contains("isn't paired"), "{err:#}");
    }
}
