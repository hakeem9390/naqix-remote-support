//! Spike C - WGC capture -> zero-copy D3D11 -> hardware H.264 -> Annex-B file.
//!
//! Not a product. The deliverable is a yes/no on three questions:
//!   1. Can a hardware MFT encode WGC's textures without a CPU round-trip?
//!   2. Does mid-stream bitrate retuning actually take effect?
//!   3. Can a keyframe be forced on demand?
//!
//! Kill criterion: if async MFT sequencing defeats you inside two weeks, stop
//! and take the GStreamer webrtcsink + rtpgccbwe path. Do not keep going
//! because it feels nearly working - see spikes/README.md.
//!
//! Run on three machines, not one: your own, a cheap old Intel iGPU laptop, and
//! something with no discrete GPU. Customers do not have RTX 4090s.

mod convert;
mod encoder;

use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::time::Instant;

use windows::Win32::Media::MediaFoundation::{MFShutdown, MFStartup, MFSTARTUP_NOSOCKET, MF_VERSION};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
use windows_capture::capture::{Context as CaptureContext, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};

use convert::BgraToNv12;
use encoder::{EncodedFrame, H264Encoder};

const FPS: u32 = 30;
const START_BITRATE: u32 = 4_000_000;
/// Second 5: drop the bitrate hard. Second 10: put it back. If the output file
/// does not visibly change size per second across those boundaries, criterion 2
/// has failed however healthy the API calls looked.
const BITRATE_DROP_AT_SECS: u64 = 5;
const BITRATE_RESTORE_AT_SECS: u64 = 10;
/// Second 8: demand a keyframe and check one actually arrives.
const FORCE_KEYFRAME_AT_SECS: u64 = 8;
const RUN_SECS: u64 = 15;

/// windows-capture requires the handler to be `Send`, because it hands the
/// struct to the capture thread. COM interface pointers are not `Send`.
///
/// Safety: every object wrapped here is CREATED inside `on_frame_arrived` and
/// only ever touched from that same capture thread, and `main` initialises COM
/// as MTA (`COINIT_MULTITHREADED`), where interface pointers are legal to use
/// from any thread in the apartment. If either of those stops being true -
/// notably if anything here is constructed in `new()` on the calling thread, or
/// COM is switched to STA - this wrapper becomes unsound and must go.
struct CaptureThreadOnly<T>(T);
unsafe impl<T> Send for CaptureThreadOnly<T> {}

impl<T> std::ops::Deref for CaptureThreadOnly<T> {
    type Target = T;
    fn deref(&self) -> &T { &self.0 }
}
impl<T> std::ops::DerefMut for CaptureThreadOnly<T> {
    fn deref_mut(&mut self) -> &mut T { &mut self.0 }
}

struct Spike {
    encoder: Option<CaptureThreadOnly<H264Encoder>>,
    converter: Option<CaptureThreadOnly<BgraToNv12>>,
    writer: BufWriter<File>,
    started: Instant,
    frames_in: u64,
    frames_out: u64,
    keyframes: u64,
    bytes: u64,
    /// Bytes per wall-clock second, so the bitrate change is visible in data
    /// rather than inferred from the API not erroring.
    per_second: Vec<u64>,
    dropped_no_demand: u64,
    did_drop: bool,
    did_restore: bool,
    did_force_key: bool,
    pending: Vec<EncodedFrame>,
    last_pts: i64,
    pts_regressions: u64,
}

impl GraphicsCaptureApiHandler for Spike {
    type Flags = String;
    type Error = anyhow::Error;

    fn new(ctx: CaptureContext<Self::Flags>) -> Result<Self, Self::Error> {
        let path = ctx.flags.clone();
        println!("writing Annex-B to {path}");
        Ok(Self {
            encoder: None,
            converter: None,
            writer: BufWriter::new(File::create(&path)?),
            started: Instant::now(),
            frames_in: 0,
            frames_out: 0,
            keyframes: 0,
            bytes: 0,
            per_second: vec![0; (RUN_SECS + 2) as usize],
            dropped_no_demand: 0,
            did_drop: false,
            did_restore: false,
            did_force_key: false,
            pending: Vec::new(),
            last_pts: -1,
            pts_regressions: 0,
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        let elapsed = self.started.elapsed();
        if elapsed.as_secs() >= RUN_SECS {
            control.stop();
            return Ok(());
        }

        // Built lazily: the D3D11 device comes from the first frame, and it has
        // to be the SAME device the encoder binds, or the zero-copy claim is
        // false while everything still appears to work.
        if self.encoder.is_none() {
            let device = frame.device().clone();
            let (w, h) = (frame.width(), frame.height());
            println!("capture {w}x{h}, building pipeline on the capture device");
            let enc = H264Encoder::new(&device, w, h, FPS, START_BITRATE)?;
            println!("hardware encoder: {}", enc.encoder_name());
            self.converter = Some(CaptureThreadOnly(BgraToNv12::new(&device, w, h)?));
            self.encoder = Some(CaptureThreadOnly(enc));
        }

        let encoder = self.encoder.as_mut().unwrap();
        let converter = self.converter.as_ref().unwrap();

        // Drain events BEFORE submitting. NeedInput has to be observed first or
        // submit() refuses - which is the whole async MFT contract.
        encoder.poll(&mut self.pending)?;

        let secs = elapsed.as_secs();
        if secs >= BITRATE_DROP_AT_SECS && !self.did_drop {
            encoder.set_bitrate(START_BITRATE / 5)?;
            self.did_drop = true;
            println!("[{secs}s] bitrate -> {} (criterion 2, down)", START_BITRATE / 5);
        }
        if secs >= FORCE_KEYFRAME_AT_SECS && !self.did_force_key {
            encoder.force_keyframe()?;
            self.did_force_key = true;
            println!("[{secs}s] forced keyframe requested (criterion 3)");
        }
        if secs >= BITRATE_RESTORE_AT_SECS && !self.did_restore {
            encoder.set_bitrate(START_BITRATE)?;
            self.did_restore = true;
            println!("[{secs}s] bitrate -> {START_BITRATE} (criterion 2, up)");
        }

        if encoder.wants_input() {
            let ts = elapsed.as_nanos() as i64 / 100;
            let dur = 10_000_000i64 / FPS as i64;
            let nv12 = converter.convert(frame.as_raw_texture(), ts, dur)?;
            encoder.submit(&nv12)?;
            self.frames_in += 1;
        } else {
            // Not an error: the encoder is busy and the frame is stale by the
            // time it would be free. Worth counting - a large number here means
            // the encoder cannot keep up with capture on this hardware.
            self.dropped_no_demand += 1;
        }

        encoder.poll(&mut self.pending)?;
        self.flush_pending(secs)?;
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        if let Some(encoder) = self.encoder.as_mut() {
            encoder.drain(&mut self.pending)?;
        }
        let secs = self.started.elapsed().as_secs();
        self.flush_pending(secs)?;
        self.writer.flush()?;
        self.report();
        Ok(())
    }
}

impl Spike {
    fn flush_pending(&mut self, secs: u64) -> Result<()> {
        for f in self.pending.drain(..) {
            // Presentation timestamps must not go backwards. When they do the
            // file still writes happily and only fails later, in the decoder.
            if f.timestamp < self.last_pts {
                self.pts_regressions += 1;
            }
            self.last_pts = f.timestamp;
            self.writer.write_all(&f.data)?;
            self.bytes += f.data.len() as u64;
            self.frames_out += 1;
            if f.keyframe {
                self.keyframes += 1;
            }
            if let Some(slot) = self.per_second.get_mut(secs as usize) {
                *slot += f.data.len() as u64;
            }
        }
        Ok(())
    }

    fn report(&self) {
        println!("\n─── results ───");
        println!("frames submitted : {}", self.frames_in);
        println!("frames encoded   : {}", self.frames_out);
        println!("keyframes        : {}", self.keyframes);
        println!("skipped (busy)   : {}", self.dropped_no_demand);
        println!("bytes written    : {}", self.bytes);
        println!(
            "pts regressions  : {}{}",
            self.pts_regressions,
            if self.pts_regressions > 0 { "  <- timestamps went backwards; the file will not decode cleanly" } else { "" }
        );
        println!("\nbytes per second (criterion 2 - expect a dip from {BITRATE_DROP_AT_SECS}s to {BITRATE_RESTORE_AT_SECS}s):");
        for (s, b) in self.per_second.iter().enumerate().take(RUN_SECS as usize) {
            let bar = "█".repeat((*b / 4096).min(60) as usize);
            println!("  {s:>2}s {b:>9}  {bar}");
        }
        println!(
            "\ncriterion 1 zero-copy  : pipeline ran on the capture device - confirm with GPU counters, not this line"
        );
        println!("criterion 2 bitrate    : judge the dip above, not the API return");
        println!(
            "criterion 3 keyframes  : {} seen; forced request was {}",
            self.keyframes,
            if self.did_force_key { "sent" } else { "NOT sent" }
        );
        println!("\nNow verify the file actually decodes:");
        println!("  ffprobe -show_frames -select_streams v out.h264 | findstr key_frame");
    }
}

fn main() -> Result<()> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok().context("CoInitializeEx")?;
        MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET).context("MFStartup")?;
    }

    let out = std::env::args().nth(1).unwrap_or_else(|| "out.h264".to_string());

    let monitor = Monitor::primary().context("no primary monitor")?;
    let settings = Settings::new(
        monitor,
        CursorCaptureSettings::WithCursor,
        DrawBorderSettings::WithoutBorder,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Default,
        DirtyRegionSettings::Default,
        // BGRA is what WGC gives natively; asking for anything else would add a
        // conversion we are trying to keep on the GPU.
        ColorFormat::Bgra8,
        out,
    );

    println!("capturing primary monitor for {RUN_SECS}s…");
    let result = Spike::start(settings);

    unsafe {
        let _ = MFShutdown();
        CoUninitialize();
    }
    result.context("capture failed")?;
    Ok(())
}
