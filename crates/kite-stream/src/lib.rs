//! Sunshine control travels over authenticated SSH; native Moonlight media travels directly.

use std::sync::Arc;

pub mod control;
pub mod display;
pub mod native;

#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    #[error(transparent)]
    Ssh(#[from] kite_ssh::SshError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Crypto(#[from] openssl::error::ErrorStack),
    #[error("Sunshine request timed out")]
    Timeout,
    #[error("invalid Sunshine response: {0}")]
    Protocol(String),
    #[error("Sunshine returned HTTP {0}")]
    Http(u16),
    #[error("Moonlight connection failed at stage {stage}: {code}")]
    Native { stage: i32, code: i32 },
    #[error("another desktop stream is active")]
    Busy,
    #[error("pairing challenge verification failed")]
    PairingRejected,
    #[error("the host has no Desktop application")]
    DesktopMissing,
    #[error("desktop stream is not running")]
    NotRunning,
    #[error("streaming requires a private network address")]
    PublicAddress,
    #[error("could not determine the Sunshine capture size: {0}")]
    Display(String),
    #[error("the decoder cannot render {width}x{height} with at most 15 percent downsampling")]
    DecoderSize { width: u32, height: u32 },
}

pub const FRAME_CONFIG: u32 = 1;
pub const FRAME_KEY: u32 = 2;

#[derive(Clone, Copy, Debug)]
pub struct VideoFormat {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct FrameInfo {
    pub presentation_us: u64,
    pub flags: u32,
}

#[derive(Clone, Debug)]
pub enum Input {
    Relative {
        x: i16,
        y: i16,
    },
    Position {
        x: i16,
        y: i16,
        width: i16,
        height: i16,
    },
    MouseButton {
        button: u8,
        down: bool,
    },
    Scroll {
        horizontal: i16,
        vertical: i16,
    },
    Key {
        key: i16,
        down: bool,
        modifiers: u8,
    },
    Text(String),
    ReleaseAll,
    RequestIdr,
}

pub trait InputSender: Send + Sync {
    fn send(&self, input: Input) -> Result<(), StreamError>;
}

/// Implemented by the platform adapter. Frames are borrowed only for this call.
pub trait MediaSink: Send + Sync {
    fn supports(&self, format: VideoFormat) -> bool;
    fn configure(&self, format: VideoFormat) -> bool;
    fn submit(&self, data: &[u8], info: FrameInfo) -> bool;
    fn set_input(&self, sender: Option<Arc<dyn InputSender>>);
    fn rendered_frames(&self) -> u64;
    fn failure_code(&self) -> i32;
    fn set_takeover(&self, enabled: bool);
    fn stop(&self);
}

#[derive(Clone, Copy, Debug)]
pub struct StreamConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate: u32,
}

impl StreamConfig {
    /// Keep the captured desktop at native size whenever the decoder supports it.
    /// H.264 needs even dimensions; only capability limits permit a small reduction.
    pub fn for_desktop(
        source: display::DesktopSize,
        supports: impl Fn(VideoFormat) -> bool,
    ) -> Result<Self, StreamError> {
        source.validate()?;
        // Prefer the shorter frame interval for interaction. A small reduction
        // may be needed to sustain 60 fps; use 30 fps only when none fits.
        for (fps, percent) in [60, 30]
            .into_iter()
            .flat_map(|fps| (85..=100).rev().map(move |p| (fps, p)))
        {
            let width = source.width * percent / 100 / 2 * 2;
            let height = source.height * percent / 100 / 2 * 2;
            let config = Self {
                width,
                height,
                fps,
                // 0.125 bits/pixel/frame: about 15.5 Mbps for 1080p60.
                bitrate: ((u64::from(width) * u64::from(height) * u64::from(fps) / 8000)
                    .clamp(1000, 40000)) as u32,
            };
            if config.validate().is_ok()
                && supports(VideoFormat {
                    width,
                    height,
                    fps: config.fps,
                })
            {
                return Ok(config);
            }
        }
        Err(StreamError::DecoderSize {
            width: source.width,
            height: source.height,
        })
    }

    pub fn validate(&self) -> Result<(), StreamError> {
        // moonlight-common-c permits dimensions above 4096 only with HEVC.
        if !(320..=4096).contains(&self.width)
            || !(240..=4096).contains(&self.height)
            || !self.width.is_multiple_of(2)
            || !self.height.is_multiple_of(2)
            || !(15..=60).contains(&self.fps)
            || !(1000..=40000).contains(&self.bitrate)
        {
            return Err(StreamError::Protocol("invalid stream configuration".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod resolution_tests {
    use super::*;
    use display::DesktopSize;

    #[test]
    fn native_desktop_is_preserved_in_both_orientations() {
        for (width, height) in [(1116, 2480), (2560, 1440), (3840, 2160)] {
            let config =
                StreamConfig::for_desktop(DesktopSize { width, height }, |_| true).unwrap();
            assert_eq!((config.width, config.height), (width, height));
            assert_eq!(config.fps, 60);
            assert!(config.bitrate >= 6000);
        }
    }

    #[test]
    fn decoder_limit_only_allows_a_small_uniform_reduction() {
        let config = StreamConfig::for_desktop(
            DesktopSize {
                width: 1116,
                height: 2480,
            },
            |f| f.height <= 2160,
        )
        .unwrap();
        assert_eq!((config.width, config.height), (970, 2156));
        assert!(
            (f64::from(config.width) / f64::from(config.height) - 1116.0 / 2480.0).abs() < 0.001
        );
        assert!(matches!(
            StreamConfig::for_desktop(
                DesktopSize {
                    width: 3840,
                    height: 2160
                },
                |f| f.width <= 1920
            ),
            Err(StreamError::DecoderSize { .. })
        ));
    }

    #[test]
    fn odd_source_dimensions_only_lose_one_pixel_for_h264() {
        let config = StreamConfig::for_desktop(
            DesktopSize {
                width: 1921,
                height: 1081,
            },
            |_| true,
        )
        .unwrap();
        assert_eq!((config.width, config.height), (1920, 1080));
    }

    #[test]
    fn prefer_small_downsampling_to_halving_interaction_frame_rate() {
        let source = DesktopSize {
            width: 1920,
            height: 1080,
        };
        let config = StreamConfig::for_desktop(source, |f| f.fps <= 30 || f.width <= 1800).unwrap();
        assert_eq!((config.width, config.height, config.fps), (1784, 1004, 60));
        let config = StreamConfig::for_desktop(source, |f| f.fps <= 30).unwrap();
        assert_eq!((config.width, config.height, config.fps), (1920, 1080, 30));
    }
}
