//! The host's cameras on Linux, through the kernel's Video4Linux2
//! interface (kernel.org/doc/html/latest/userspace-api/media/v4l/), with
//! no library of the system's: a capture device is opened, given a
//! format, and its buffers are mapped and streamed on a thread of the
//! capture's own, which sleeps in `poll` between pictures.
//!
//! Formats taken, best first: NV12, YU12 (I420) and YUYV, all 8-bit
//! Y'CbCr that become [`VideoFrame::yuv420_strided`] frames without a
//! decoder of their own. A camera that offers only MJPEG or H.264 is not
//! listed.
//!
//! The structures below are the kernel's UAPI (`linux/videodev2.h`) as
//! laid out on a 64-bit target; the module is built on those alone.

#![allow(unsafe_code)]

use std::ffi::CStr;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::fs::OpenOptionsExt;
use std::thread::JoinHandle;

use crate::camera::{CameraFrames, CameraInfo, CameraPosition, CameraService, Capture};
use crate::frame_input::{StridedPlane, VideoFrame, YuvColour, YuvMatrix};

/// The size asked for: enough to read a QR code across a room, cheap to
/// convert every picture of. The driver answers with what it has.
const WANTED: (u32, u32) = (640, 480);
const BUFFERS: u32 = 4;
const LOST: &str = "The camera was disconnected";

const fn fourcc(c: &[u8; 4]) -> u32 {
    (c[0] as u32) | ((c[1] as u32) << 8) | ((c[2] as u32) << 16) | ((c[3] as u32) << 24)
}

const NV12: u32 = fourcc(b"NV12");
const YU12: u32 = fourcc(b"YU12");
const YUYV: u32 = fourcc(b"YUYV");
/// Best first.
const TAKEN: [u32; 3] = [NV12, YU12, YUYV];

const BUF_TYPE_VIDEO_CAPTURE: u32 = 1;
const MEMORY_MMAP: u32 = 1;
const FIELD_NONE: u32 = 1;
const CAP_VIDEO_CAPTURE: u32 = 0x0000_0001;
const CAP_STREAMING: u32 = 0x0400_0000;
const CAP_DEVICE_CAPS: u32 = 0x8000_0000;

#[repr(C)]
struct Capability {
    driver: [u8; 16],
    card: [u8; 32],
    bus_info: [u8; 32],
    version: u32,
    capabilities: u32,
    device_caps: u32,
    reserved: [u32; 3],
}

#[repr(C)]
struct FmtDesc {
    index: u32,
    kind: u32,
    flags: u32,
    description: [u8; 32],
    pixelformat: u32,
    mbus_code: u32,
    reserved: [u32; 3],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct PixFormat {
    width: u32,
    height: u32,
    pixelformat: u32,
    field: u32,
    bytesperline: u32,
    sizeimage: u32,
    colorspace: u32,
    private: u32,
    flags: u32,
    ycbcr_enc: u32,
    quantization: u32,
    xfer_func: u32,
}

/// The kernel's union holds pointers, which align it to 8.
#[repr(C)]
union FormatUnion {
    pix: PixFormat,
    raw: [u8; 200],
    _align: *const u8,
}

#[repr(C)]
struct Format {
    kind: u32,
    fmt: FormatUnion,
}

#[repr(C)]
struct RequestBuffers {
    count: u32,
    kind: u32,
    memory: u32,
    capabilities: u32,
    flags: u8,
    reserved: [u8; 3],
}

#[repr(C)]
struct Timecode {
    kind: u32,
    flags: u32,
    frames: u8,
    seconds: u8,
    minutes: u8,
    hours: u8,
    userbits: [u8; 4],
}

#[repr(C)]
union BufferM {
    offset: u32,
    userptr: libc::c_ulong,
    planes: *mut u8,
    fd: i32,
}

#[repr(C)]
struct Buffer {
    index: u32,
    kind: u32,
    bytesused: u32,
    flags: u32,
    field: u32,
    timestamp: libc::timeval,
    timecode: Timecode,
    sequence: u32,
    memory: u32,
    m: BufferM,
    length: u32,
    reserved2: u32,
    request_fd: i32,
}

const fn ioc(dir: u64, nr: u64, size: usize) -> u64 {
    (dir << 30) | ((size as u64) << 16) | ((b'V' as u64) << 8) | nr
}
const READ: u64 = 2;
const WRITE: u64 = 1;
const QUERYCAP: u64 = ioc(READ, 0, size_of::<Capability>());
const ENUM_FMT: u64 = ioc(READ | WRITE, 2, size_of::<FmtDesc>());
const S_FMT: u64 = ioc(READ | WRITE, 5, size_of::<Format>());
const REQBUFS: u64 = ioc(READ | WRITE, 8, size_of::<RequestBuffers>());
const QUERYBUF: u64 = ioc(READ | WRITE, 9, size_of::<Buffer>());
const QBUF: u64 = ioc(READ | WRITE, 15, size_of::<Buffer>());
const DQBUF: u64 = ioc(READ | WRITE, 17, size_of::<Buffer>());
const STREAMON: u64 = ioc(WRITE, 18, size_of::<libc::c_int>());
const STREAMOFF: u64 = ioc(WRITE, 19, size_of::<libc::c_int>());

// The kernel's sizes on a 64-bit target: a mismatch would make every
// request a different one.
const _: () = assert!(size_of::<Capability>() == 104);
const _: () = assert!(size_of::<FmtDesc>() == 64);
const _: () = assert!(size_of::<Format>() == 208);
const _: () = assert!(size_of::<RequestBuffers>() == 20);
const _: () = assert!(size_of::<Buffer>() == 88);

/// `ioctl` until it is not interrupted.
///
/// # Safety
/// `arg` is the structure `request` takes.
unsafe fn ioctl<T>(fd: RawFd, request: u64, arg: *mut T) -> std::io::Result<()> {
    loop {
        // SAFETY: the caller's.
        if unsafe { libc::ioctl(fd, request as _, arg) } != -1 {
            return Ok(());
        }
        let err = std::io::Error::last_os_error();
        if err.kind() != std::io::ErrorKind::Interrupted {
            return Err(err);
        }
    }
}

/// A zeroed UAPI structure.
fn zeroed<T>() -> T {
    // SAFETY: only used for the plain-data structures above, for which
    // all zeroes is a value.
    unsafe { std::mem::zeroed() }
}

fn open(path: &str) -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
}

/// The formats of ours `fd` offers, best first.
fn formats(fd: RawFd) -> Vec<u32> {
    let mut offered = Vec::new();
    for index in 0.. {
        let mut d: FmtDesc = zeroed();
        d.index = index;
        d.kind = BUF_TYPE_VIDEO_CAPTURE;
        // SAFETY: ENUM_FMT takes a FmtDesc.
        if unsafe { ioctl(fd, ENUM_FMT, &raw mut d) }.is_err() {
            break;
        }
        offered.push(d.pixelformat);
    }
    TAKEN.into_iter().filter(|f| offered.contains(f)).collect()
}

/// The camera at `path`, when it captures pictures we take.
fn describe(path: &str) -> Option<CameraInfo> {
    let file = open(path).ok()?;
    let fd = file.as_raw_fd();
    let mut cap: Capability = zeroed();
    // SAFETY: QUERYCAP takes a Capability.
    unsafe { ioctl(fd, QUERYCAP, &raw mut cap) }.ok()?;
    let caps = if cap.capabilities & CAP_DEVICE_CAPS != 0 {
        cap.device_caps
    } else {
        cap.capabilities
    };
    // A webcam has a second node for its metadata, which captures none.
    if caps & CAP_VIDEO_CAPTURE == 0 || caps & CAP_STREAMING == 0 || formats(fd).is_empty() {
        return None;
    }
    let card = CStr::from_bytes_until_nul(&cap.card)
        .ok()
        .and_then(|c| c.to_str().ok())
        .filter(|c| !c.is_empty())
        .unwrap_or(path);
    Some(CameraInfo {
        id: path.to_owned(),
        description: card.to_owned(),
        position: CameraPosition::Unspecified,
        is_default: false,
    })
}

/// The cameras V4L2 has, the first by path the default.
#[derive(Clone, Copy, Debug, Default)]
pub struct V4l2Cameras;

impl CameraService for V4l2Cameras {
    fn cameras(&self) -> Vec<CameraInfo> {
        let Ok(dir) = std::fs::read_dir("/dev") else {
            return Vec::new();
        };
        let mut paths: Vec<String> = dir
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| {
                n.strip_prefix("video")
                    .is_some_and(|d| d.parse::<u32>().is_ok())
            })
            .map(|n| format!("/dev/{n}"))
            .collect();
        paths.sort_by_key(|p| {
            p.trim_start_matches("/dev/video")
                .parse::<u32>()
                .unwrap_or(0)
        });
        let mut found: Vec<CameraInfo> = paths.iter().filter_map(|p| describe(p)).collect();
        if let Some(first) = found.first_mut() {
            first.is_default = true;
        }
        found
    }

    fn open(&self, camera: &CameraInfo, frames: CameraFrames) -> Result<Capture, String> {
        let stream =
            Stream::start(&camera.id).map_err(|e| format!("{}: {e}", camera.description))?;
        let (wake_rx, wake_tx) = pipe().map_err(|e| e.to_string())?;
        let thread = std::thread::Builder::new()
            .name("ljos-camera".into())
            .spawn(move || stream.run(&wake_rx, &frames))
            .map_err(|e| e.to_string())?;
        Ok(Capture::new(Running {
            wake: Some(File::from(wake_tx)),
            thread: Some(thread),
        }))
    }
}

fn pipe() -> std::io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0; 2];
    // SAFETY: two ints for the two ends.
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } == -1 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: both ends are new and ours.
    Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

/// A capture's thread, stopped and joined when dropped.
struct Running {
    wake: Option<File>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(mut wake) = self.wake.take() {
            let _ = wake.write_all(&[1]);
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// A mapped buffer.
struct Mapped {
    ptr: *mut libc::c_void,
    len: usize,
}

/// A streaming device and its mapped buffers.
struct Stream {
    file: File,
    pix: PixFormat,
    buffers: Vec<Mapped>,
}

// SAFETY: the mappings are the stream's own, used by one thread at a
// time: made on the Engine thread, then moved to the capture's.
unsafe impl Send for Stream {}

impl Stream {
    fn start(path: &str) -> std::io::Result<Stream> {
        let file = open(path)?;
        let fd = file.as_raw_fd();
        let Some(&pixelformat) = formats(fd).first() else {
            return Err(std::io::Error::other("captures in no format Ljós reads"));
        };
        let mut f: Format = zeroed();
        f.kind = BUF_TYPE_VIDEO_CAPTURE;
        f.fmt.pix = PixFormat {
            width: WANTED.0,
            height: WANTED.1,
            pixelformat,
            field: FIELD_NONE,
            ..zeroed()
        };
        // SAFETY: S_FMT takes a Format.
        unsafe { ioctl(fd, S_FMT, &raw mut f) }?;
        // SAFETY: the driver filled in the pix member it was given.
        let pix = unsafe { f.fmt.pix };
        if !TAKEN.contains(&pix.pixelformat) {
            return Err(std::io::Error::other(
                "the driver chose a format Ljós does not read",
            ));
        }
        let mut req: RequestBuffers = zeroed();
        req.count = BUFFERS;
        req.kind = BUF_TYPE_VIDEO_CAPTURE;
        req.memory = MEMORY_MMAP;
        // SAFETY: REQBUFS takes a RequestBuffers.
        unsafe { ioctl(fd, REQBUFS, &raw mut req) }?;
        let mut stream = Stream {
            file,
            pix,
            buffers: Vec::new(),
        };
        for index in 0..req.count {
            let mut b = buffer(index);
            // SAFETY: QUERYBUF takes a Buffer.
            unsafe { ioctl(fd, QUERYBUF, &raw mut b) }?;
            // SAFETY: the offset and length are the driver's, for this fd.
            let ptr = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    b.length as usize,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED,
                    fd,
                    b.m.offset as libc::off_t,
                )
            };
            if ptr == libc::MAP_FAILED {
                return Err(std::io::Error::last_os_error());
            }
            stream.buffers.push(Mapped {
                ptr,
                len: b.length as usize,
            });
            // SAFETY: QBUF takes a Buffer.
            unsafe { ioctl(fd, QBUF, &raw mut b) }?;
        }
        let mut kind = BUF_TYPE_VIDEO_CAPTURE as libc::c_int;
        // SAFETY: STREAMON takes the buffer type.
        unsafe { ioctl(fd, STREAMON, &raw mut kind) }?;
        Ok(stream)
    }

    /// Hands each picture to `frames` until woken through `wake` or the
    /// device is lost.
    fn run(self, wake: &OwnedFd, frames: &CameraFrames) {
        let fd = self.file.as_raw_fd();
        loop {
            let mut polled = [
                libc::pollfd {
                    fd,
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: wake.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            // SAFETY: two pollfds; no timeout, so nothing runs between
            // pictures.
            let n = unsafe { libc::poll(polled.as_mut_ptr(), 2, -1) };
            if n == -1 {
                if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                frames.fail(LOST);
                return;
            }
            if polled[1].revents != 0 {
                return;
            }
            if polled[0].revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                frames.fail(LOST);
                return;
            }
            let mut b = buffer(0);
            // SAFETY: DQBUF takes a Buffer.
            match unsafe { ioctl(fd, DQBUF, &raw mut b) } {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(_) => {
                    frames.fail(LOST);
                    return;
                }
            }
            if let Some(m) = self.buffers.get(b.index as usize) {
                let used = (b.bytesused as usize).min(m.len);
                // SAFETY: the driver handed this buffer back to us until
                // it is queued again below.
                let bytes = unsafe { std::slice::from_raw_parts(m.ptr.cast::<u8>(), used) };
                if let Some(frame) = picture(&self.pix, bytes) {
                    frames.frame(frame);
                }
            }
            // SAFETY: QBUF takes a Buffer.
            if unsafe { ioctl(fd, QBUF, &raw mut b) }.is_err() {
                frames.fail(LOST);
                return;
            }
        }
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        let fd = self.file.as_raw_fd();
        let mut kind = BUF_TYPE_VIDEO_CAPTURE as libc::c_int;
        // SAFETY: STREAMOFF takes the buffer type.
        let _ = unsafe { ioctl(fd, STREAMOFF, &raw mut kind) };
        for m in self.buffers.drain(..) {
            // SAFETY: mapped by `start`, unmapped once.
            unsafe { libc::munmap(m.ptr, m.len) };
        }
    }
}

fn buffer(index: u32) -> Buffer {
    let mut b: Buffer = zeroed();
    b.index = index;
    b.kind = BUF_TYPE_VIDEO_CAPTURE;
    b.memory = MEMORY_MMAP;
    b
}

/// Webcams send standard-definition video's colours.
const COLOUR: YuvColour = YuvColour {
    matrix: YuvMatrix::Bt601,
    full_range: false,
};

/// One buffer's picture, in the format the driver chose.
fn picture(pix: &PixFormat, bytes: &[u8]) -> Option<VideoFrame> {
    let (w, h) = (pix.width, pix.height);
    let stride = pix.bytesperline as usize;
    fn plane(data: &[u8], row_stride: usize, pixel_stride: usize) -> StridedPlane<'_> {
        StridedPlane {
            data,
            row_stride,
            pixel_stride,
        }
    }
    match pix.pixelformat {
        NV12 => {
            let uv = bytes.get(stride * h as usize..)?;
            VideoFrame::yuv420_strided(
                w,
                h,
                [
                    plane(bytes, stride, 1),
                    plane(uv, stride, 2),
                    plane(uv.get(1..)?, stride, 2),
                ],
                COLOUR,
            )
            .ok()
        }
        YU12 => {
            let half = stride / 2;
            let u = bytes.get(stride * h as usize..)?;
            let v = u.get(half * (h as usize).div_ceil(2)..)?;
            VideoFrame::yuv420_strided(
                w,
                h,
                [
                    plane(bytes, stride, 1),
                    plane(u, half, 1),
                    plane(v, half, 1),
                ],
                COLOUR,
            )
            .ok()
        }
        YUYV => yuyv(w, h, stride, bytes),
        _ => None,
    }
}

/// Y'CbCr 4:2:2 packed as Y0 Cb Y1 Cr, taken to 4:2:0 by keeping the
/// chroma of every other row.
fn yuyv(width: u32, height: u32, stride: usize, bytes: &[u8]) -> Option<VideoFrame> {
    let (w, h) = (width as usize, height as usize);
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    if w == 0 || h == 0 || stride < w * 2 || bytes.len() < (h - 1) * stride + w * 2 {
        return None;
    }
    let mut luma = Vec::with_capacity(w * h);
    let mut cb = Vec::with_capacity(cw * ch);
    let mut cr = Vec::with_capacity(cw * ch);
    for (y, row) in bytes.chunks(stride).take(h).enumerate() {
        let row = &row[..w * 2];
        luma.extend(row.iter().step_by(2));
        if y % 2 == 0 {
            for pair in row.chunks(4) {
                cb.push(pair[1]);
                cr.push(*pair.get(3).unwrap_or(&128));
            }
        }
    }
    VideoFrame::yuv420(width, height, [luma, cb, cr], COLOUR).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_input::PixelFormat;

    #[test]
    fn requests_are_the_kernels() {
        // Values from linux/videodev2.h as a 64-bit build sees them.
        assert_eq!(QUERYCAP, 0x8068_5600);
        assert_eq!(S_FMT, 0xc0d0_5605);
        assert_eq!(QBUF, 0xc058_560f);
        assert_eq!(DQBUF, 0xc058_5611);
        assert_eq!(STREAMON, 0x4004_5612);
    }

    #[test]
    fn yuyv_keeps_every_luma_and_every_other_rows_chroma() {
        // 2 by 2, rows of 6 bytes (2 of padding).
        let bytes = [10, 100, 11, 200, 0, 0, 12, 101, 13, 201, 0, 0];
        let frame = yuyv(2, 2, 6, &bytes).unwrap();
        assert_eq!(frame.format(), PixelFormat::Yuv420P8);
        let planes = frame.planes();
        assert_eq!(planes[0].data, &[10, 11, 12, 13]);
        assert_eq!((planes[1].data, planes[2].data), (&[100][..], &[200][..]));
        assert!(yuyv(2, 2, 6, &bytes[..9]).is_none());
    }

    #[test]
    fn nv12_from_a_buffer() {
        let pix = PixFormat {
            width: 2,
            height: 2,
            pixelformat: NV12,
            bytesperline: 2,
            ..zeroed()
        };
        let frame = picture(&pix, &[1, 2, 3, 4, 50, 60]).unwrap();
        let planes = frame.planes();
        assert_eq!(planes[0].data, &[1, 2, 3, 4]);
        assert_eq!((planes[1].data, planes[2].data), (&[50][..], &[60][..]));
    }

    #[test]
    fn a_machine_without_cameras_lists_none_and_does_not_fail() {
        // Whatever the machine has, listing never panics, and every camera
        // listed has a path.
        for c in V4l2Cameras.cameras() {
            assert!(c.id.starts_with("/dev/video"), "{c:?}");
        }
    }
}
