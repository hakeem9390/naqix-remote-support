//! BGRA -> NV12 colour conversion on the GPU.
//!
//! WGC hands out BGRA. Every vendor's hardware H.264 encoder wants NV12. Doing
//! that conversion on the CPU would read the frame back over PCIe and undo the
//! entire zero-copy argument, so it runs as a Video Processor MFT bound to the
//! same D3D11 device - textures never leave the GPU.
//!
//! Sunshine does this with a hand-written shader (src/platform/windows/
//! display_vram.cpp) which is faster and far more code. The stock Video
//! Processor is the right trade for a spike: if it is fast enough here, a
//! shader is a later optimisation rather than a prerequisite.

use anyhow::{anyhow, Context, Result};
use windows::core::Interface;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};

pub struct BgraToNv12 {
    transform: IMFTransform,
    _device_manager: IMFDXGIDeviceManager,
}

impl BgraToNv12 {
    pub fn new(device: &ID3D11Device, width: u32, height: u32) -> Result<Self> {
        unsafe {
            let transform: IMFTransform =
                CoCreateInstance(&CLSID_VideoProcessorMFT, None, CLSCTX_INPROC_SERVER)
                    .context("creating the Video Processor MFT")?;

            let mut reset_token = 0u32;
            let mut manager: Option<IMFDXGIDeviceManager> = None;
            MFCreateDXGIDeviceManager(&mut reset_token, &mut manager)?;
            let manager = manager.ok_or_else(|| anyhow!("device manager was null"))?;
            manager.ResetDevice(device, reset_token)?;

            transform.ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, manager.as_raw() as usize)
                .context("binding D3D11 to the video processor")?;

            // Unlike the encoder, the video processor wants its INPUT set first.
            let inp = MFCreateMediaType()?;
            inp.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            inp.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_ARGB32)?;
            inp.SetUINT64(&MF_MT_FRAME_SIZE, ((width as u64) << 32) | height as u64)?;
            inp.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            transform.SetInputType(0, &inp, 0).context("video processor input type")?;

            let out = MFCreateMediaType()?;
            out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            out.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
            out.SetUINT64(&MF_MT_FRAME_SIZE, ((width as u64) << 32) | height as u64)?;
            out.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            transform.SetOutputType(0, &out, 0).context("video processor output type")?;

            transform.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
            transform.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;

            Ok(Self { transform, _device_manager: manager })
        }
    }

    /// Synchronous, unlike the encoder: the video processor is a plain MFT, so
    /// ProcessInput then ProcessOutput is correct here and would be a deadlock
    /// there. Returns an NV12 sample ready to hand to the encoder.
    pub fn convert(&self, bgra: &ID3D11Texture2D, timestamp: i64, duration: i64) -> Result<IMFSample> {
        unsafe {
            let buffer = MFCreateDXGISurfaceBuffer(&ID3D11Texture2D::IID, bgra, 0, false)
                .context("wrapping the BGRA texture")?;
            let sample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            sample.SetSampleTime(timestamp)?;
            sample.SetSampleDuration(duration)?;

            self.transform.ProcessInput(0, &sample, 0).context("video processor ProcessInput")?;

            // The video processor does NOT allocate its own output, so we have
            // to supply a sample backed by memory it can write into.
            let info = self.transform.GetOutputStreamInfo(0)?;
            let out_buffer = MFCreateMemoryBuffer(info.cbSize)?;
            let out_sample = MFCreateSample()?;
            out_sample.AddBuffer(&out_buffer)?;

            let mut bufs = [MFT_OUTPUT_DATA_BUFFER {
                dwStreamID: 0,
                pSample: std::mem::ManuallyDrop::new(Some(out_sample.clone())),
                dwStatus: 0,
                pEvents: std::mem::ManuallyDrop::new(None),
            }];
            let mut status = 0u32;
            self.transform
                .ProcessOutput(0, &mut bufs, &mut status)
                .context("video processor ProcessOutput")?;

            out_sample.SetSampleTime(timestamp)?;
            out_sample.SetSampleDuration(duration)?;
            Ok(out_sample)
        }
    }
}
