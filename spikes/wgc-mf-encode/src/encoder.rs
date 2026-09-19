//! Hardware H.264 encoder over Media Foundation, fed D3D11 textures directly.
//!
//! The whole point of the spike lives here. Three things must be true or the
//! plan's Track B branch changes:
//!   1. A hardware MFT can be handed the SAME D3D11 device the capture uses,
//!      so frames never round-trip through system memory.
//!   2. Bitrate can be retuned mid-stream (congestion response).
//!   3. A keyframe can be forced on demand (PLI response).
//!
//! Async MFTs are the part that defeats people. A hardware encoder does not
//! work like `ProcessInput` then `ProcessOutput`; it raises METransformNeedInput
//! and METransformHaveOutput on its own schedule and you must obey them. Calling
//! ProcessInput without a preceding NeedInput is the classic bug, and it
//! deadlocks rather than erroring.

use anyhow::{anyhow, bail, Context, Result};
use windows::core::{Interface, GUID};
use windows::Win32::Graphics::Direct3D11::ID3D11Device;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Variant::VARIANT;

/// 100-nanosecond units - Media Foundation's time base throughout.
pub type Hns = i64;

pub struct EncodedFrame {
    pub data: Vec<u8>,
    pub timestamp: Hns,
    pub keyframe: bool,
}

pub struct H264Encoder {
    transform: IMFTransform,
    events: IMFMediaEventGenerator,
    codec_api: ICodecAPI,
    input_id: u32,
    output_id: u32,
    /// Async MFTs will not accept input until they have asked for it. Feeding
    /// them early is the single most common way this pipeline hangs.
    pending_input_requests: u32,
    _device_manager: IMFDXGIDeviceManager,
    name: String,
}

impl H264Encoder {
    /// `device` must be the same ID3D11Device the capture produces textures on.
    /// Handing the MFT a different device silently forces a CPU copy, which
    /// defeats the entire exercise.
    pub fn new(device: &ID3D11Device, width: u32, height: u32, fps: u32, bitrate: u32) -> Result<Self> {
        unsafe {
            let (activate, name) = find_hardware_encoder()?;
            let transform: IMFTransform = activate
                .ActivateObject()
                .context("activating the hardware encoder MFT")?;

            let attrs = transform.GetAttributes().context("MFT has no attributes")?;

            // A hardware MFT is async and arrives LOCKED. Until this is set it
            // will refuse to process anything, with an error that does not say so.
            let is_async = attrs.GetUINT32(&MF_TRANSFORM_ASYNC).unwrap_or(0) == 1;
            if is_async {
                attrs
                    .SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1)
                    .context("unlocking the async MFT")?;
            }
            // Ask for low latency: without it encoders buffer several frames
            // deep, which is fine for recording and useless for remote desktop.
            let _ = attrs.SetUINT32(&MF_LOW_LATENCY, 1);

            let mut input_ids = [0u32; 1];
            let mut output_ids = [0u32; 1];
            // Most encoders have fixed stream ids of 0 and report E_NOTIMPL here,
            // which is not a failure.
            let (input_id, output_id) = match transform.GetStreamIDs(&mut input_ids, &mut output_ids) {
                Ok(()) => (input_ids[0], output_ids[0]),
                Err(_) => (0, 0),
            };

            // Bind our D3D11 device so the MFT reads our textures in place.
            let device_manager = create_device_manager(device)?;
            transform
                .ProcessMessage(
                    MFT_MESSAGE_SET_D3D_MANAGER,
                    device_manager.as_raw() as usize,
                )
                .context("binding the D3D11 device manager - encoder may not support D3D11 input")?;

            // Output type MUST be set before input type. The encoder derives
            // which input formats it will admit from the output it is asked for,
            // so doing this in the other order fails with a misleading error.
            let out = MFCreateMediaType().context("creating output media type")?;
            out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            out.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
            out.SetUINT32(&MF_MT_AVG_BITRATE, bitrate)?;
            set_ratio(&out, &MF_MT_FRAME_RATE, fps, 1)?;
            set_ratio(&out, &MF_MT_FRAME_SIZE, width, height)?;
            out.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            set_ratio(&out, &MF_MT_PIXEL_ASPECT_RATIO, 1, 1)?;
            out.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_Main.0 as u32)?;
            transform
                .SetOutputType(output_id, &out, 0)
                .context("setting H.264 output type")?;

            // NV12 is the only input every vendor's encoder agrees on. WGC hands
            // us BGRA, so a GPU colour conversion sits in front - see convert.rs.
            let inp = MFCreateMediaType().context("creating input media type")?;
            inp.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            inp.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
            set_ratio(&inp, &MF_MT_FRAME_SIZE, width, height)?;
            set_ratio(&inp, &MF_MT_FRAME_RATE, fps, 1)?;
            inp.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            set_ratio(&inp, &MF_MT_PIXEL_ASPECT_RATIO, 1, 1)?;
            transform
                .SetInputType(input_id, &inp, 0)
                .context("setting NV12 input type")?;

            let codec_api: ICodecAPI = transform.cast().context("MFT does not expose ICodecAPI")?;
            let events: IMFMediaEventGenerator =
                transform.cast().context("MFT is not an event generator")?;

            transform.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0)?;
            transform.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
            transform.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;

            Ok(Self {
                transform,
                events,
                codec_api,
                input_id,
                output_id,
                pending_input_requests: 0,
                _device_manager: device_manager,
                name,
            })
        }
    }

    pub fn encoder_name(&self) -> &str {
        &self.name
    }

    /// Kill criterion 2: congestion response. GCC hands us a target bitrate and
    /// the encoder has to actually follow it mid-stream.
    pub fn set_bitrate(&self, bps: u32) -> Result<()> {
        unsafe {
            self.codec_api
                .SetValue(&CODECAPI_AVEncCommonMeanBitRate, &VARIANT::from(bps))
                .context("retuning bitrate")?;
        }
        Ok(())
    }

    /// Kill criterion 3: PLI response. A newly-joined or recovering viewer is
    /// blind until the next keyframe, so we must be able to demand one.
    pub fn force_keyframe(&self) -> Result<()> {
        unsafe {
            self.codec_api
                .SetValue(&CODECAPI_AVEncVideoForceKeyFrame, &VARIANT::from(1u32))
                .context("forcing a keyframe")?;
        }
        Ok(())
    }

    /// Pump the event queue. Returns any frames the encoder finished.
    ///
    /// Non-blocking: MF_EVENT_FLAG_NO_WAIT means we never park the capture
    /// thread waiting on the encoder.
    // The METransform* constants keep Windows' own naming, so the style lint
    // fires. They DO resolve as constants - the warning says "constant in
    // pattern", not "unused variable" - so the match is a real comparison and
    // not four catch-all bindings, which is the bug this would otherwise be.
    #[allow(non_upper_case_globals)]
    pub fn poll(&mut self, out: &mut Vec<EncodedFrame>) -> Result<()> {
        unsafe {
            loop {
                let event = match self.events.GetEvent(MF_EVENT_FLAG_NO_WAIT) {
                    Ok(e) => e,
                    // Queue drained. Expected, not an error.
                    Err(_) => return Ok(()),
                };
                let kind = event.GetType()?;
                match MF_EVENT_TYPE(kind as i32) {
                    METransformNeedInput => self.pending_input_requests += 1,
                    METransformHaveOutput => {
                        if let Some(frame) = self.drain_one_output()? {
                            out.push(frame);
                        }
                    }
                    METransformDrainComplete => return Ok(()),
                    _ => {}
                }
            }
        }
    }

    /// True when the encoder has asked for a frame. Submitting without this
    /// being true is the deadlock.
    pub fn wants_input(&self) -> bool {
        self.pending_input_requests > 0
    }

    /// Hand the encoder an NV12 sample from the converter. Still zero-copy:
    /// the sample wraps a GPU surface, nothing is read back to system memory.
    pub fn submit(&mut self, sample: &IMFSample) -> Result<()> {
        if self.pending_input_requests == 0 {
            bail!("submit() called before the encoder asked for input");
        }
        unsafe {
            self.transform
                .ProcessInput(self.input_id, sample, 0)
                .context("ProcessInput")?;
        }
        self.pending_input_requests -= 1;
        Ok(())
    }

    unsafe fn drain_one_output(&mut self) -> Result<Option<EncodedFrame>> {
        // Hardware MFTs allocate their own output samples, so we pass a null
        // sample and let the MFT fill it in.
        let mut bufs = [MFT_OUTPUT_DATA_BUFFER {
            dwStreamID: self.output_id,
            pSample: std::mem::ManuallyDrop::new(None),
            dwStatus: 0,
            pEvents: std::mem::ManuallyDrop::new(None),
        }];
        let mut status = 0u32;
        self.transform
            .ProcessOutput(0, &mut bufs, &mut status)
            .context("ProcessOutput")?;

        let sample = match bufs[0].pSample.as_ref() {
            Some(s) => s.clone(),
            None => return Ok(None),
        };

        let timestamp = sample.GetSampleTime().unwrap_or(0);
        // Absence of the attribute means "not a keyframe", which is the common
        // case, so a missing attribute is not an error.
        let keyframe = sample
            .GetUINT32(&MFSampleExtension_CleanPoint)
            .map(|v| v == 1)
            .unwrap_or(false);

        let media_buffer = sample.ConvertToContiguousBuffer()?;
        let mut ptr: *mut u8 = std::ptr::null_mut();
        let mut len = 0u32;
        media_buffer.Lock(&mut ptr, None, Some(&mut len))?;
        let data = std::slice::from_raw_parts(ptr, len as usize).to_vec();
        media_buffer.Unlock()?;

        Ok(Some(EncodedFrame { data, timestamp, keyframe }))
    }

    pub fn drain(&mut self, out: &mut Vec<EncodedFrame>) -> Result<()> {
        unsafe {
            self.transform.ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0)?;
            self.transform.ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0)?;
        }
        self.poll(out)
    }
}

/// Enumerate hardware H.264 encoders. SORTANDFILTER puts the preferred one
/// first; on a machine with no hardware encoder this returns nothing, which is
/// a real result worth reporting rather than silently falling back to software.
unsafe fn find_hardware_encoder() -> Result<(IMFActivate, String)> {
    let output_info = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_H264,
    };
    let mut activates: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count = 0u32;

    MFTEnumEx(
        MFT_CATEGORY_VIDEO_ENCODER,
        MFT_ENUM_FLAG(MFT_ENUM_FLAG_HARDWARE.0 | MFT_ENUM_FLAG_SORTANDFILTER.0),
        None,
        Some(&output_info),
        &mut activates,
        &mut count,
    )
    .context("MFTEnumEx for hardware H.264 encoders")?;

    if count == 0 {
        bail!(
            "no hardware H.264 encoder on this machine. That is a finding, not a bug - \
             note the GPU and re-run on the other test machines."
        );
    }

    let list = std::slice::from_raw_parts(activates, count as usize);
    let activate = list[0]
        .clone()
        .ok_or_else(|| anyhow!("MFTEnumEx returned a null activate"))?;

    let mut name_ptr = windows::core::PWSTR::null();
    let mut name_len = 0u32;
    let name = match activate.GetAllocatedString(
        &MFT_FRIENDLY_NAME_Attribute,
        &mut name_ptr,
        &mut name_len,
    ) {
        Ok(()) => {
            let s = name_ptr.to_string().unwrap_or_default();
            windows::Win32::System::Com::CoTaskMemFree(Some(name_ptr.0 as *const _));
            s
        }
        Err(_) => "<unnamed>".to_string(),
    };

    windows::Win32::System::Com::CoTaskMemFree(Some(activates as *const _));
    Ok((activate, name))
}

unsafe fn create_device_manager(device: &ID3D11Device) -> Result<IMFDXGIDeviceManager> {
    let mut reset_token = 0u32;
    let mut manager: Option<IMFDXGIDeviceManager> = None;
    MFCreateDXGIDeviceManager(&mut reset_token, &mut manager)
        .context("MFCreateDXGIDeviceManager")?;
    let manager = manager.ok_or_else(|| anyhow!("device manager was null"))?;
    manager
        .ResetDevice(device, reset_token)
        .context("ResetDevice - is the D3D11 device multithread-protected?")?;
    Ok(manager)
}

/// MF packs paired values (width/height, numerator/denominator) into one u64.
unsafe fn set_ratio(ty: &IMFMediaType, key: &GUID, high: u32, low: u32) -> Result<()> {
    ty.SetUINT64(key, ((high as u64) << 32) | low as u64)?;
    Ok(())
}
