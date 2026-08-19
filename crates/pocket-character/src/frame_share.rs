//! POSIX shared-memory transport for camera BGRA frames and person mattes.
//!
//! The Rust host creates a private shm object and passes its name to the
//! local Swift helper. A sequence lock protects one packed snapshot because
//! macOS POSIX shm does not support `flock`. The JSON reader thread copies a
//! newly published snapshot into an `Arc<VideoFrame>`; the render thread
//! never waits on the cross-process transport.

use std::ffi::CString;
use std::fs::File;
use std::os::fd::FromRawFd;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use anyhow::{Context, Result, bail};
use memmap2::{MmapMut, MmapOptions};

pub const HEADER_BYTES: usize = 64;
pub const FRAME_SHARE_VERSION: u32 = 1;
const MAGIC: &[u8; 8] = b"PKLVFRM\0";
static NEXT_SHARE: AtomicU32 = AtomicU32::new(0);

#[derive(Clone, Debug)]
pub struct VideoFrame {
    pub sequence: u64,
    pub captured_at_ns: u64,
    pub width: u32,
    pub height: u32,
    /// Tightly packed BGRA8, top row first.
    pub bgra: Vec<u8>,
    pub mask_width: u32,
    pub mask_height: u32,
    /// Tightly packed 8-bit person matte, 0 = background, 255 = person.
    pub person_mask: Vec<u8>,
}

pub struct FrameShareReader {
    name: CString,
    _file: File,
    mmap: MmapMut,
    max_width: u32,
    max_height: u32,
    last_sequence: u64,
}

impl FrameShareReader {
    pub fn create(max_width: u32, max_height: u32) -> Result<(Self, String)> {
        if max_width == 0 || max_height == 0 {
            bail!("frame-share dimensions must be non-zero");
        }
        let suffix = NEXT_SHARE.fetch_add(1, Ordering::Relaxed);
        let name_string = format!("/pocket-live-{}-{suffix}", std::process::id());
        let name = CString::new(name_string.clone()).expect("shm name contains no NUL");
        // Remove only our exact process-scoped name if a previous abnormal
        // run left it behind.
        unsafe { libc::shm_unlink(name.as_ptr()) };
        let fd = unsafe {
            libc::shm_open(
                name.as_ptr(),
                libc::O_CREAT | libc::O_EXCL | libc::O_RDWR,
                0o600,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error()).context("creating frame-share shm");
        }
        let file = unsafe { File::from_raw_fd(fd) };
        let pixels = max_width as usize * max_height as usize;
        let capacity = HEADER_BYTES
            .checked_add(pixels.checked_mul(5).context("frame-share size overflow")?)
            .context("frame-share capacity overflow")?;
        file.set_len(capacity as u64)
            .context("sizing frame-share shm")?;
        let mmap = unsafe { MmapOptions::new().len(capacity).map_mut(&file) }
            .context("mapping frame-share shm")?;
        Ok((
            Self {
                name,
                _file: file,
                mmap,
                max_width,
                max_height,
                last_sequence: 0,
            },
            name_string,
        ))
    }

    /// Read the newest complete snapshot. A busy writer or unchanged
    /// sequence returns `Ok(None)` so the consumer never blocks.
    pub fn read(&mut self, expected_sequence: u64) -> Result<Option<VideoFrame>> {
        let published = load_sequence(&self.mmap);
        if published == 0 || published != expected_sequence || published <= self.last_sequence {
            return Ok(None);
        }
        let frame = parse_snapshot(
            &self.mmap,
            self.max_width,
            self.max_height,
            self.last_sequence,
        )?;
        // If the writer began another frame while we copied, its release
        // store changed the sequence to 0 (or to a newer value). Discard the
        // torn snapshot and wait for the matching JSON publication.
        if load_sequence(&self.mmap) != published {
            return Ok(None);
        }
        let Some(frame) = frame else { return Ok(None) };
        if frame.sequence != published {
            // Tracking JSON and pixels are published by the same capture
            // callback. Refuse cross-frame combinations.
            return Ok(None);
        }
        self.last_sequence = frame.sequence;
        Ok(Some(frame))
    }
}

fn load_sequence(bytes: &[u8]) -> u64 {
    debug_assert!(bytes.len() >= 24);
    debug_assert_eq!((bytes.as_ptr() as usize + 16) % align_of::<AtomicU64>(), 0);
    // The shm mapping is page aligned and the sequence lives at aligned
    // offset 16. Swift publishes it through a C11 `_Atomic(uint64_t)`.
    let sequence = unsafe { &*(bytes.as_ptr().add(16).cast::<AtomicU64>()) };
    u64::from_le(sequence.load(Ordering::Acquire))
}

impl Drop for FrameShareReader {
    fn drop(&mut self) {
        unsafe { libc::shm_unlink(self.name.as_ptr()) };
    }
}

fn parse_snapshot(
    bytes: &[u8],
    max_width: u32,
    max_height: u32,
    last_sequence: u64,
) -> Result<Option<VideoFrame>> {
    if bytes.len() < HEADER_BYTES || &bytes[0..8] != MAGIC {
        return Ok(None);
    }
    let version = u32_at(bytes, 8)?;
    let header_bytes = u32_at(bytes, 12)? as usize;
    if version != FRAME_SHARE_VERSION || header_bytes != HEADER_BYTES {
        bail!("unsupported frame-share header version={version} bytes={header_bytes}");
    }
    let sequence = u64_at(bytes, 16)?;
    if sequence == 0 || sequence <= last_sequence {
        return Ok(None);
    }
    let captured_at_ns = u64_at(bytes, 24)?;
    let width = u32_at(bytes, 32)?;
    let height = u32_at(bytes, 36)?;
    let bgra_stride = u32_at(bytes, 40)?;
    let bgra_bytes = u32_at(bytes, 44)? as usize;
    let mask_width = u32_at(bytes, 48)?;
    let mask_height = u32_at(bytes, 52)?;
    let mask_stride = u32_at(bytes, 56)?;
    let mask_bytes = u32_at(bytes, 60)? as usize;

    if width == 0 || height == 0 || width > max_width || height > max_height {
        bail!("invalid shared video dimensions {width}x{height}");
    }
    if bgra_stride != width * 4 || bgra_bytes != (width * height * 4) as usize {
        bail!("shared BGRA must be tightly packed");
    }
    if mask_bytes > 0
        && (mask_width == 0
            || mask_height == 0
            || mask_width > max_width
            || mask_height > max_height
            || mask_stride != mask_width
            || mask_bytes != (mask_width * mask_height) as usize)
    {
        bail!("shared person mask must be tightly packed and within capacity");
    }
    let bgra_start = HEADER_BYTES;
    let bgra_end = bgra_start
        .checked_add(bgra_bytes)
        .context("BGRA range overflow")?;
    let mask_end = bgra_end
        .checked_add(mask_bytes)
        .context("mask range overflow")?;
    if mask_end > bytes.len() {
        bail!("shared frame exceeds mapped capacity");
    }
    Ok(Some(VideoFrame {
        sequence,
        captured_at_ns,
        width,
        height,
        bgra: bytes[bgra_start..bgra_end].to_vec(),
        mask_width,
        mask_height,
        person_mask: bytes[bgra_end..mask_end].to_vec(),
    }))
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32> {
    let raw: [u8; 4] = bytes
        .get(offset..offset + 4)
        .context("truncated frame-share u32")?
        .try_into()
        .expect("slice length checked");
    Ok(u32::from_le_bytes(raw))
}

fn u64_at(bytes: &[u8], offset: usize) -> Result<u64> {
    let raw: [u8; 8] = bytes
        .get(offset..offset + 8)
        .context("truncated frame-share u64")?
        .try_into()
        .expect("slice length checked");
    Ok(u64::from_le_bytes(raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(sequence: u64) -> Vec<u8> {
        let mut bytes = vec![0u8; HEADER_BYTES + 2 * 1 * 4 + 2 * 1];
        bytes[0..8].copy_from_slice(MAGIC);
        bytes[8..12].copy_from_slice(&FRAME_SHARE_VERSION.to_le_bytes());
        bytes[12..16].copy_from_slice(&(HEADER_BYTES as u32).to_le_bytes());
        bytes[16..24].copy_from_slice(&sequence.to_le_bytes());
        bytes[24..32].copy_from_slice(&99u64.to_le_bytes());
        bytes[32..36].copy_from_slice(&2u32.to_le_bytes());
        bytes[36..40].copy_from_slice(&1u32.to_le_bytes());
        bytes[40..44].copy_from_slice(&8u32.to_le_bytes());
        bytes[44..48].copy_from_slice(&8u32.to_le_bytes());
        bytes[48..52].copy_from_slice(&2u32.to_le_bytes());
        bytes[52..56].copy_from_slice(&1u32.to_le_bytes());
        bytes[56..60].copy_from_slice(&2u32.to_le_bytes());
        bytes[60..64].copy_from_slice(&2u32.to_le_bytes());
        bytes[64..72].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        bytes[72..74].copy_from_slice(&[0, 255]);
        bytes
    }

    #[test]
    fn parses_the_cross_language_layout() {
        let frame = parse_snapshot(&snapshot(7), 1920, 1080, 0)
            .unwrap()
            .unwrap();
        assert_eq!(frame.sequence, 7);
        assert_eq!(frame.bgra, [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(frame.person_mask, [0, 255]);
    }

    #[test]
    fn ignores_an_unchanged_sequence() {
        assert!(
            parse_snapshot(&snapshot(7), 1920, 1080, 7)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn rejects_a_dimension_over_capacity() {
        let mut bytes = snapshot(1);
        bytes[32..36].copy_from_slice(&4000u32.to_le_bytes());
        assert!(parse_snapshot(&bytes, 1920, 1080, 0).is_err());
    }
}
