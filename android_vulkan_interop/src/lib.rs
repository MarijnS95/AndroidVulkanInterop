use std::{
    fs::File,
    io::{self, BufRead, BufReader},
    thread,
};

use jni::{
    objects::{JClass, JObject},
    JNIEnv,
};
use log::{debug, error, info, LevelFilter};
use ndk::{native_window::NativeWindow, surface_texture::SurfaceTexture};

use crate::native_window::{connect_egl, dequeue_buffer, queue_buffer, NativeWindowBuffer};
// use raw_window_handle::{AndroidDisplayHandle, HasRawWindowHandle, RawDisplayHandle};

mod native_window;

fn render_to_native_window(window: NativeWindow) {
    dbg!(&window);

    // TODO: NDK should implement this!
    // let raw_display_handle = window.raw_display_handle();
    // let raw_display_handle = RawDisplayHandle::Android(AndroidDisplayHandle::empty());
    // let raw_window_handle = window.raw_window_handle();

    window.try_allocate_buffers();
    // connect_egl(&window).expect("Could not connect buffer queue to EGL");
    if let Err(e) = connect_egl(&window) {
        error!("Failed to connect {window:?} to EGL, skipping: {e}");
        return;
    }
    let (nwbuf, fence) = dbg!(dequeue_buffer(&window).unwrap());
    let hwbuf = dbg!(nwbuf.hardware_buffer());
    dbg!(hwbuf.describe());
    let nwbuf: NativeWindowBuffer = dbg!(hwbuf.into());

    queue_buffer(&window, nwbuf, fence).unwrap();
}

#[no_mangle]
pub extern "system" fn Java_rust_androidvulkaninterop_MainActivity_00024Companion_init(
    _env: JNIEnv,
    _class: JClass,
) {
    android_logger::init_once(android_logger::Config::default().with_max_level(LevelFilter::Trace));

    info!("Initializing Rust code");

    let file = {
        let (read, write) = rustix::pipe::pipe().unwrap();
        rustix::stdio::dup2_stdout(&write).unwrap();
        rustix::stdio::dup2_stderr(&write).unwrap();

        File::from(read)
    };

    thread::spawn(move || -> io::Result<()> {
        let mut reader = BufReader::new(file);
        let mut buffer = String::new();
        loop {
            buffer.clear();
            let len = reader.read_line(&mut buffer)?;
            if len == 0 {
                break Ok(());
            } else {
                info!(target: "RustStdoutStderr", "{}", buffer);
            }
        }
    });
}

#[no_mangle]
pub extern "system" fn Java_rust_androidvulkaninterop_MainActivity_00024Companion_renderToSurface(
    env: JNIEnv,
    _class: JClass,
    surface: JObject,
) {
    debug!("Java Surface: {:?}", surface);

    let window =
        unsafe { NativeWindow::from_surface(env.get_native_interface(), surface.into_raw()) }
            .unwrap();

    render_to_native_window(window)
}

#[no_mangle]
pub extern "system" fn Java_rust_androidvulkaninterop_MainActivity_00024Companion_renderToSurfaceTexture(
    env: JNIEnv,
    _class: JClass,
    surface_texture: JObject,
) {
    debug!("Java SurfaceTexture: {:?}", surface_texture);

    let surface_texture = unsafe {
        SurfaceTexture::from_surface_texture(env.get_native_interface(), surface_texture.into_raw())
            .unwrap()
    };

    let window = surface_texture.acquire_native_window().unwrap();

    render_to_native_window(window)
}
