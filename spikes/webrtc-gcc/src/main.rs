//! Spike B — does webrtc-rs 0.21's congestion control actually track a
//! bandwidth step?
//!
//! Only runs if Spike C2 fails. C2's WebSocket path, if it works, removes the
//! need for WebRTC entirely, because MeshAgent already dials outbound and NAT
//! is therefore already solved.
//!
//! No capture and no encoder: a pre-recorded Annex-B file is sent to a browser
//! so the only thing under test is the estimator. GCC shipped in 0.21.0 on
//! 2026-09-19 — days before this was written — so it is new code, and "the API
//! exists" is not the same claim as "the algorithm converges".
//!
//! Kill criterion: target bitrate must follow a 3 Mbps → 800 kbps → 3 Mbps step
//! within ~5s in BOTH directions, without oscillating. Judge the logged trace,
//! not whether the program ran.

mod annexb;

use anyhow::{Context, Result, anyhow};
use rtc::interceptor::{BandwidthEstimator, EstimatorStats, Gcc, PacketReport, Registry};
use rtc::media::Sample;
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::configuration::interceptor_registry::{
    CongestionFeedback, configure_congestion_control, register_default_interceptors,
};
use rtc::peer_connection::configuration::media_engine::{MIME_TYPE_H264, MediaEngine};
use rtc::peer_connection::sdp::RTCSessionDescription;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodecParameters, RTCRtpCodingParameters, RTCRtpEncodingParameters,
    RtpCodecKind,
};
use rtc::rtp_transceiver::SSRC;
use std::fs;
use std::io::Write as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};
use webrtc::media_stream::Track;
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_local::static_sample::TrackLocalStaticSample;
use webrtc::peer_connection::{
    // PeerConnection is the TRAIT; build() returns an opaque impl of it, so
    // without this import every method on the connection is "not found".
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::runtime::default_runtime;

/// Where the estimator starts, and the floor and ceiling it may move between.
/// Starting low on purpose: opening at the ceiling congests the very path the
/// estimator is still trying to measure.
const DEFAULT_INITIAL: f64 = 800_000.0;
const DEFAULT_MIN: f64 = 100_000.0;
const DEFAULT_MAX: f64 = 5_000_000.0;

const FPS: u64 = 30;

/// Overridable, because the interesting runs are the constrained ones and
/// recompiling to change a ceiling wastes the tester's attention.
fn env_bps(key: &str, fallback: f64) -> f64 {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(fallback)
}

// ── Estimator ────────────────────────────────────────────────────────────────

/// Delegates to `inner` and publishes its target after every update.
///
/// `configure_congestion_control` takes the estimator by value and boxes it
/// inside the interceptor chain, so the number cannot be pulled out - it has to
/// be pushed. Wrapping the estimator is the documented integration point, and
/// it reaches into no internals: a `BandwidthEstimator` is a function from
/// acknowledgements to a number, so anything wanting to watch that number can
/// sit where the algorithm sits.
struct ReportingEstimator<E: BandwidthEstimator> {
    inner: E,
    target: Arc<AtomicU64>,
}

impl<E: BandwidthEstimator> ReportingEstimator<E> {
    fn new(inner: E) -> (Self, Arc<AtomicU64>) {
        let target = Arc::new(AtomicU64::new(inner.target_bitrate().to_bits()));
        let handle = Arc::clone(&target);
        (Self { inner, target }, handle)
    }
    fn publish(&self) {
        self.target.store(self.inner.target_bitrate().to_bits(), Ordering::Relaxed);
    }
}

impl<E: BandwidthEstimator> BandwidthEstimator for ReportingEstimator<E> {
    // on_reports and handle_timeout are the two points the interceptor contract
    // names as able to move the estimate. target_bitrate takes &self, so it
    // cannot publish.
    fn on_reports(&mut self, now: Instant, reports: &[PacketReport]) {
        self.inner.on_reports(now, reports);
        self.publish();
    }
    fn target_bitrate(&self) -> f64 {
        self.inner.target_bitrate()
    }
    fn handle_timeout(&mut self, now: Instant) {
        self.inner.handle_timeout(now);
        self.publish();
    }
    fn poll_timeout(&self) -> Option<Instant> {
        self.inner.poll_timeout()
    }
    fn stats(&self) -> EstimatorStats {
        self.inner.stats()
    }
}

// ── Peer connection events ───────────────────────────────────────────────────

#[derive(Clone)]
struct Handler {
    gathered: Arc<tokio::sync::Notify>,
    connected: Arc<tokio::sync::Notify>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            self.gathered.notify_waiters();
        }
    }
    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        println!("peer connection: {state}");
        if state == RTCPeerConnectionState::Connected {
            self.connected.notify_waiters();
        }
    }
}

// ── Signalling ───────────────────────────────────────────────────────────────

struct Offer {
    sdp: String,
    reply: oneshot::Sender<String>,
}

/// Hand-rolled HTTP/1.1, deliberately. The example this is based on pastes
/// base64 SDP through the terminal, which cannot be automated; a real HTTP
/// framework would be a dependency that has nothing to do with what is being
/// measured.
async fn signalling(port: u16, tx: mpsc::Sender<Offer>) -> Result<()> {
    let page = include_str!("../web/index.html");
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    println!("http://localhost:{port}");

    loop {
        let (mut sock, _) = listener.accept().await?;
        let tx = tx.clone();
        let page = page.to_string();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 64 * 1024];
            let n = match sock.read(&mut buf).await {
                Ok(n) if n > 0 => n,
                _ => return,
            };
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let is_offer = req.starts_with("POST /offer");

            let response = if is_offer {
                let body = req.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
                let (reply_tx, reply_rx) = oneshot::channel();
                if tx.send(Offer { sdp: body, reply: reply_tx }).await.is_err() {
                    return;
                }
                match reply_rx.await {
                    Ok(answer) => format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                        answer.len(),
                        answer
                    ),
                    Err(_) => "HTTP/1.1 500 Internal Server Error\r\ncontent-length: 0\r\n\r\n".into(),
                }
            } else {
                format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-length: {}\r\n\r\n{}",
                    page.len(),
                    page
                )
            };
            let _ = sock.write_all(response.as_bytes()).await;
        });
    }
}

// ── Entry point ──────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let path = std::env::args().nth(1).unwrap_or_else(|| "sample.h264".into());
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(5191);

    let raw = fs::read(&path).with_context(|| format!("reading {path}"))?;
    let units = annexb::to_access_units(&raw);
    if units.is_empty() {
        return Err(anyhow!("no access units in {path} - is it raw Annex-B rather than MP4?"));
    }
    println!(
        "{path}: {} access units, {} keyframes",
        units.len(),
        units.iter().filter(|u| u.key).count()
    );

    let mut media_engine = MediaEngine::default();
    let video_codec = RTCRtpCodecParameters {
        rtp_codec: RTCRtpCodec {
            mime_type: MIME_TYPE_H264.to_owned(),
            clock_rate: 90000,
            channels: 0,
            sdp_fmtp_line: "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f"
                .to_owned(),
            rtcp_feedback: vec![],
        },
        payload_type: 102,
    };
    media_engine.register_codec(video_codec.clone(), RtpCodecKind::Video)?;

    // configure_congestion_control places the send history, pacer and TWCC
    // sender/receiver at the slots the chain reserves, and registers the
    // transport-cc feedback and header extension the remote needs to report
    // arrivals at all. Without those the estimator holds its initial rate
    // forever and never reports a problem - which looks exactly like "GCC does
    // not work" while actually being a setup error.
    let initial = env_bps("INITIAL_BPS", DEFAULT_INITIAL);
    let min = env_bps("MIN_BPS", DEFAULT_MIN);
    let max = env_bps("MAX_BPS", DEFAULT_MAX);
    println!(
        "gcc bounds: initial {:.0} kbps, min {:.0} kbps, max {:.0} kbps",
        initial / 1000.0,
        min / 1000.0,
        max / 1000.0
    );
    let (estimator, target_bitrate) = ReportingEstimator::new(Gcc::new(initial, min, max));
    let registry = configure_congestion_control(
        Registry::new(),
        estimator,
        CongestionFeedback::Twcc,
        &mut media_engine,
    )?;
    let registry = register_default_interceptors(registry, &mut media_engine)?;

    let handler = Handler {
        gathered: Arc::new(tokio::sync::Notify::new()),
        connected: Arc::new(tokio::sync::Notify::new()),
    };

    let pc = PeerConnectionBuilder::new()
        .with_configuration(RTCConfigurationBuilder::new().build())
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .with_handler(Arc::new(handler.clone()))
        .with_runtime(default_runtime().ok_or_else(|| anyhow!("no runtime feature enabled"))?)
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    let ssrc: SSRC = rand::random::<u32>();
    let track: Arc<TrackLocalStaticSample> = Arc::new(TrackLocalStaticSample::new(
        Instant::now(),
        MediaStreamTrack::new(
            "spike-b-stream".to_owned(),
            "spike-b-video".to_owned(),
            "spike-b".to_owned(),
            RtpCodecKind::Video,
            vec![RTCRtpEncodingParameters {
                rtp_coding_parameters: RTCRtpCodingParameters {
                    ssrc: Some(ssrc),
                    ..Default::default()
                },
                codec: video_codec.rtp_codec.clone(),
                ..Default::default()
            }],
        ),
    )?);
    let sender = pc.add_track(Arc::clone(&track) as Arc<dyn TrackLocal>).await?;

    let (offer_tx, mut offer_rx) = mpsc::channel::<Offer>(4);
    tokio::spawn(async move {
        if let Err(e) = signalling(port, offer_tx).await {
            eprintln!("signalling stopped: {e}");
        }
    });

    println!("waiting for a browser to connect…");
    let offer_msg = offer_rx.recv().await.ok_or_else(|| anyhow!("signalling closed"))?;
    let offer: RTCSessionDescription = serde_json::from_str(&offer_msg.sdp)
        .context("parsing the browser's offer")?;

    let gathered = handler.gathered.notified();
    pc.set_remote_description(offer).await?;
    let answer = pc.create_answer(None).await?;
    pc.set_local_description(answer).await?;
    // Non-trickle: wait for gathering to finish so the single answer carries
    // every candidate.
    gathered.await;

    let local = pc.local_description().await.ok_or_else(|| anyhow!("no local description"))?;
    let _ = offer_msg.reply.send(serde_json::to_string(&local)?);

    handler.connected.notified().await;
    println!("connected — streaming\n");

    let payload_type = sender
        .get_parameters()
        .await?
        .rtp_parameters
        .codecs
        .first()
        .map(|c| c.payload_type)
        .ok_or_else(|| anyhow!("sender has no negotiated codec"))?;

    tokio::spawn(log_target_bitrate(Arc::clone(&target_bitrate)));

    let ssrc = *track.ssrcs().await.first().ok_or_else(|| anyhow!("track has no ssrc"))?;
    let frame_duration = Duration::from_millis(1000 / FPS);
    let mut ticker = tokio::time::interval(frame_duration);
    let mut i = 0usize;

    loop {
        ticker.tick().await;
        let unit = &units[i];
        i = (i + 1) % units.len();

        track
            .sample_writer(ssrc, payload_type)
            .write_sample(&Sample {
                data: unit.data.clone(),
                duration: frame_duration,
                ..Sample::new(Instant::now())
            })
            .await?;
    }
}

/// The deliverable. Prints the estimate over time so the kill criterion is
/// judged from a trace rather than from the program not crashing.
async fn log_target_bitrate(target: Arc<AtomicU64>) {
    let started = Instant::now();
    let mut csv = match fs::File::create("gcc-trace.csv") {
        Ok(f) => Some(f),
        Err(e) => {
            eprintln!("could not write gcc-trace.csv: {e}");
            None
        }
    };
    if let Some(f) = csv.as_mut() {
        let _ = writeln!(f, "seconds,target_bps");
    }

    let mut ticker = tokio::time::interval(Duration::from_millis(500));
    loop {
        ticker.tick().await;
        let secs = started.elapsed().as_secs_f64();
        let bps = f64::from_bits(target.load(Ordering::Relaxed));
        // One block per 100 kbps, so the shape of the response is visible in
        // the terminal without opening the CSV.
        let bar = "█".repeat(((bps / 100_000.0) as usize).min(60));
        println!("{secs:6.1}s  {:>8.0} kbps  {bar}", bps / 1000.0);
        if let Some(f) = csv.as_mut() {
            let _ = writeln!(f, "{secs:.1},{bps:.0}");
        }
    }
}
