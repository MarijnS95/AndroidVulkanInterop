//! Polyfills for missing `ANativeWindowBuffer` bindings in the NDK
//! <https://cs.android.com/android/platform/superproject/main/+/main:frameworks/base/libs/hwui/renderthread/ReliableSurface.cpp;drc=59bf534b3ad4cbe626f1c5539d422f6c8fe4b90a>

use core::fmt;
use std::{
    ffi::c_void,
    io,
    mem::{size_of, MaybeUninit},
    ptr::NonNull,
    sync::OnceLock,
};

use ndk::{hardware_buffer::HardwareBuffer, native_window::NativeWindow};
use rustix::fd::{FromRawFd, IntoRawFd, OwnedFd, RawFd};

pub mod private_hardware_buffer_helpers {
    pub use super::ANativeWindowBuffer;
    pub use ndk_sys::{AHardwareBuffer, AHardwareBuffer_Desc};
    include!(concat!(env!("OUT_DIR"), "/AHardwareBufferHelpers.rs"));
}

/// <https://cs.android.com/android/platform/superproject/main/+/main:frameworks/native/libs/nativebase/include/nativebase/nativebase.h>
#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct android_native_base_t {
    magic: i32,
    version: i32,
    reserved: [*mut c_void; 4],
    inc_ref: *mut extern "C" fn(*mut Self),
    dec_ref: *mut extern "C" fn(*mut Self),
}

/// <https://cs.android.com/android/platform/superproject/main/+/main:frameworks/native/libs/nativewindow/include/system/window.h>
#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct ANativeWindow {
    common: android_native_base_t,
    flags: u32,
    min_swap_interval: i32,
    max_swap_interval: i32,
    xdpi: f32,
    ydpi: f32,
    oem: [isize; 4],
    set_swap_interval: extern "C" fn(*mut Self, i32) -> i32,
    dequeue_buffer_deprecated: *mut c_void,
    lock_buffer_deprecated: *mut c_void,
    queue_buffer_deprecated: *mut c_void,
    query_buffer_deprecated: *mut c_void,
    perform: extern "C" fn(*mut Self, i32, ...) -> i32,
}

type native_handle_t = c_void;

/// <https://cs.android.com/android/platform/superproject/main/+/main:frameworks/native/libs/nativebase/include/nativebase/nativebase.h>
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ANativeWindowBuffer {
    // _unused: [u8; 0],
    common: android_native_base_t,
    width: i32,
    height: i32,
    stride: i32,
    format: i32,
    usage_deprecated: i32,
    layer_count: usize,
    reserved: [*mut c_void; 1],
    handle: *const native_handle_t,
    usage: u64,
    reserved_proc: [*mut c_void; 8 - size_of::<u64>() / size_of::<*const c_void>()],
}

type ANativeWindow_dequeueBuffer = unsafe extern "system" fn(
    window: *mut ndk_sys::ANativeWindow,
    buffer: *mut *mut ANativeWindowBuffer,
    fence_fd: *mut RawFd,
) -> i32;
type ANativeWindow_queueBuffer = unsafe extern "system" fn(
    window: *mut ndk_sys::ANativeWindow,
    buffer: *mut ANativeWindowBuffer,
    fence_fd: RawFd,
) -> i32;
type ANativeWindowBuffer_getHardwareBuffer =
    unsafe extern "system" fn(buffer: *mut ANativeWindowBuffer) -> *mut ndk_sys::AHardwareBuffer;
type AHardwareBuffer_to_ANativeWindowBuffer =
    unsafe extern "system" fn(buffer: *mut ndk_sys::AHardwareBuffer) -> *mut ANativeWindowBuffer;

pub struct NativeWindowBuffer(NonNull<ANativeWindowBuffer>);

impl fmt::Debug for NativeWindowBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeWindowBuffer")
            .field("0", &self.0)
            .field("inner", unsafe { &*self.0.as_ptr() })
            .finish()
    }
}

impl NativeWindowBuffer {
    /// Returns a [`HardwareBuffer`] which does **not** consume a reference on
    /// [`NativeWindowBuffer`] and might have broken lifetime.
    #[doc(alias = "ANativeWindowBuffer_getHardwareBuffer")]
    pub fn hardware_buffer(self) -> HardwareBuffer {
        static FN: OnceLock<ANativeWindowBuffer_getHardwareBuffer> = OnceLock::new();
        let func = FN.get_or_init(|| {
            *unsafe { lib().get(b"ANativeWindowBuffer_getHardwareBuffer\0") }.unwrap()
        });

        let buffer = unsafe { (func)(self.0.as_ptr()) };
        let buffer = NonNull::new(buffer).unwrap();

        unsafe { HardwareBuffer::from_ptr(buffer) }
    }
}

impl From<HardwareBuffer> for NativeWindowBuffer {
    /// Returns a [`NativeWindowBuffer`] which does **not** consume a reference on
    /// [`HardwareBuffer`] and might have broken lifetime.
    #[doc(alias = "AHardwareBuffer_to_ANativeWindowBuffer")]
    fn from(buffer: HardwareBuffer) -> Self {
        let nwb = unsafe {
            private_hardware_buffer_helpers::android_AHardwareBuffer_to_ANativeWindowBuffer1(
                buffer.as_ptr(),
            )
        };
        Self(NonNull::new(nwb).unwrap())
    }
}

static LIB: OnceLock<libloading::Library> = OnceLock::new();

fn lib() -> &'static libloading::Library {
    LIB.get_or_init(|| unsafe { libloading::Library::new("libnativewindow.so") }.unwrap())
}

pub fn connect_egl(window: &NativeWindow) -> io::Result<()> {
    let mut win = window.ptr().cast::<ANativeWindow>();
    let win = unsafe { win.as_mut() };

    const NATIVE_WINDOW_API_CONNECT: i32 = 13;
    const NATIVE_WINDOW_API_EGL: i32 = 1;

    let status = (win.perform)(win, NATIVE_WINDOW_API_CONNECT, NATIVE_WINDOW_API_EGL);

    status_to_io_result(status)
}

pub fn dequeue_buffer(window: &NativeWindow) -> io::Result<(NativeWindowBuffer, Option<OwnedFd>)> {
    static FN: OnceLock<ANativeWindow_dequeueBuffer> = OnceLock::new();
    let func = FN.get_or_init(|| *unsafe { lib().get(b"ANativeWindow_dequeueBuffer\0") }.unwrap());

    let mut buffer = MaybeUninit::uninit();
    let mut fd = MaybeUninit::uninit();

    let status = unsafe { (func)(window.ptr().as_ptr(), buffer.as_mut_ptr(), fd.as_mut_ptr()) };

    status_to_io_result(status)?;

    Ok((
        NativeWindowBuffer(NonNull::new(unsafe { buffer.assume_init() }).unwrap()),
        match unsafe { fd.assume_init() } {
            -1 => None,
            fd => Some(unsafe { OwnedFd::from_raw_fd(fd) }),
        },
    ))
}

pub fn queue_buffer(
    window: &NativeWindow,
    buffer: NativeWindowBuffer,
    fd: Option<OwnedFd>,
) -> io::Result<()> {
    static FN: OnceLock<ANativeWindow_queueBuffer> = OnceLock::new();
    let func = FN.get_or_init(|| *unsafe { lib().get(b"ANativeWindow_queueBuffer\0") }.unwrap());

    let status = unsafe {
        (func)(
            window.ptr().as_ptr(),
            buffer.0.as_ptr(),
            fd.map_or(-1, |fd| fd.into_raw_fd()),
        )
    };

    status_to_io_result(status)
}

pub(crate) fn status_to_io_result(status: i32) -> io::Result<()> {
    match status {
        0 => Ok(()),
        r if r < 0 => Err(io::Error::from_raw_os_error(-r)),
        r => unreachable!("Status is positive integer {}", r),
    }
}
