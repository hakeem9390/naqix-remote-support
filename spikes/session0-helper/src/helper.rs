//! The helper: capture one monitor at 5 fps and write PNGs.
//!
//! Deliberately the least interesting half. No codec, no network, no encoder -
//! if this spike fails it must be obvious that session and desktop handling is
//! at fault and not something downstream. PNGs on disk are the evidence.

use anyhow::Result;
use std::path::PathBuf;
use std::time::Instant;
use windows_capture::capture::{Context as CaptureContext, GraphicsCaptureApiHandler};
use windows_capture::encoder::ImageFormat;
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};

use crate::desktop;
use crate::logf;

const ROLE: &str = "helper";
const FPS: u64 = 5;

pub fn out_dir() -> PathBuf {
    PathBuf::from(r"C:\ProgramData\naqix-spike-a\frames")
}

struct Capture {
    last: Instant,
    written: u64,
    tag: String,
}

impl GraphicsCaptureApiHandler for Capture {
    type Flags = String;
    type Error = anyhow::Error;

    fn new(ctx: CaptureContext<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self { last: Instant::now(), written: 0, tag: ctx.flags.clone() })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        _control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        // WGC delivers on change, which on a static lock screen can be almost
        // never and during video can be 60/s. Throttle rather than trust it.
        if self.last.elapsed().as_millis() < (1000 / FPS) as u128 && self.written > 0 {
            return Ok(());
        }
        self.last = Instant::now();

        let dir = out_dir();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}-{:05}.png", self.tag, self.written));
        frame.save_as_image(path.to_string_lossy().as_ref(), ImageFormat::Png)?;

        if self.written == 0 {
            logf!(
                ROLE,
                "FIRST FRAME {}x{} -> {} (this is the result that matters)",
                frame.width(),
                frame.height(),
                path.display()
            );
        }
        self.written += 1;
        if self.written.is_multiple_of(FPS * 5) {
            logf!(ROLE, "{} frames written", self.written);
        }
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        logf!(ROLE, "capture closed after {} frames", self.written);
        Ok(())
    }
}

pub fn run(tag: &str) -> Result<()> {
    // Attach to whichever desktop has input BEFORE opening capture: the API
    // binds to the calling thread's desktop, so doing this afterwards captures
    // the wrong one.
    match desktop::follow_input_desktop() {
        Ok(name) => logf!(ROLE, "attached to input desktop '{name}'"),
        Err(e) => logf!(ROLE, "could not follow input desktop: {e:#} (continuing on the inherited one)"),
    }
    match desktop::current_desktop_name() {
        Ok(name) => logf!(ROLE, "thread desktop is '{name}'"),
        Err(e) => logf!(ROLE, "current_desktop_name failed: {e:#}"),
    }

    let monitor = match Monitor::primary() {
        Ok(m) => m,
        Err(e) => {
            // Expected and informative when there is no visible desktop at all.
            logf!(ROLE, "NO PRIMARY MONITOR: {e:#} - no desktop is visible from here");
            return Ok(());
        }
    };

    let settings = Settings::new(
        monitor,
        CursorCaptureSettings::WithCursor,
        DrawBorderSettings::WithoutBorder,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Default,
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        tag.to_string(),
    );

    logf!(ROLE, "starting capture, tag={tag}");
    if let Err(e) = Capture::start(settings) {
        // The interesting failure. On the secure desktop WGC does not error at
        // all - it runs and the prompt is simply absent from the frames - so an
        // error here means something else, and the message is the finding.
        logf!(ROLE, "CAPTURE FAILED: {e:?}");
        return Err(anyhow::anyhow!("capture failed: {e:?}"));
    }
    Ok(())
}
