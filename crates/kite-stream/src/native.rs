//! Small owned interface to the pinned C transport; no C types escape this module.

use crate::{
    FrameInfo, Input, InputSender, MediaSink, StreamConfig, StreamError, VideoFormat,
    control::Launch,
};
use std::{
    collections::BTreeMap,
    ffi::{CStr, CString, c_char, c_void},
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering},
    },
};

static ACTIVE: AtomicBool = AtomicBool::new(false);

#[repr(C)]
struct Callbacks {
    configure: unsafe extern "C" fn(*mut c_void, i32, i32, i32) -> i32,
    frame: unsafe extern "C" fn(*mut c_void, *const u8, i32, u64, u32) -> i32,
    stage: unsafe extern "C" fn(*mut c_void, i32, i32),
    terminated: unsafe extern "C" fn(*mut c_void, i32),
}
unsafe extern "C" {
    fn kite_ml_start(
        address: *const c_char,
        app: *const c_char,
        gfe: *const c_char,
        rtsp: *const c_char,
        codecs: i32,
        width: i32,
        height: i32,
        fps: i32,
        bitrate: i32,
        key: *const u8,
        iv: *const u8,
        context: *mut c_void,
        callbacks: *const Callbacks,
    ) -> i32;
    fn kite_ml_stop();
    fn kite_ml_interrupt();
    fn kite_ml_launch_query() -> *const c_char;
    fn kite_ml_mouse_position(x: i16, y: i16, width: i16, height: i16) -> i32;
    fn kite_ml_mouse_move(x: i16, y: i16) -> i32;
    fn kite_ml_mouse_button(button: i32, down: i32) -> i32;
    fn kite_ml_scroll(horizontal: i16, vertical: i16) -> i32;
    fn kite_ml_key(key: i16, down: i32, modifiers: u8) -> i32;
    fn kite_ml_text(text: *const u8, length: u32) -> i32;
    fn kite_ml_request_idr();
}

struct Context {
    sink: Arc<dyn MediaSink>,
    stage: AtomicI32,
    error: AtomicI32,
    terminated: AtomicBool,
    received: AtomicU64,
}

#[derive(Default)]
struct Pressed {
    keys: BTreeMap<i16, u8>,
    buttons: BTreeMap<u8, bool>,
}
struct Sender {
    active: RwLock<bool>,
    pressed: Mutex<Pressed>,
}

impl Sender {
    fn release(&self) {
        if let Ok(mut p) = self.pressed.lock() {
            for (key, modifiers) in std::mem::take(&mut p.keys) {
                unsafe {
                    kite_ml_key(key, 0, modifiers);
                }
            }
            for (button, _) in std::mem::take(&mut p.buttons) {
                unsafe {
                    kite_ml_mouse_button(i32::from(button), 0);
                }
            }
        }
    }
    fn close(&self) {
        if let Ok(mut active) = self.active.write() {
            *active = false;
            self.release();
        }
    }
}
impl InputSender for Sender {
    fn send(&self, input: Input) -> Result<(), StreamError> {
        let active = self
            .active
            .try_read()
            .map_err(|_| StreamError::NotRunning)?;
        if !*active {
            return Err(StreamError::NotRunning);
        }
        let result = match input {
            Input::Relative { x, y } => unsafe { kite_ml_mouse_move(x, y) },
            Input::Position {
                x,
                y,
                width,
                height,
            } if width > 0 && height > 0 => unsafe {
                kite_ml_mouse_position(x.clamp(0, width - 1), y.clamp(0, height - 1), width, height)
            },
            Input::MouseButton { button, down } if (1..=5).contains(&button) => {
                let mut p = self.pressed.lock().map_err(|_| StreamError::NotRunning)?;
                if down {
                    p.buttons.insert(button, true);
                } else {
                    p.buttons.remove(&button);
                }
                unsafe { kite_ml_mouse_button(i32::from(button), i32::from(down)) }
            }
            Input::Key {
                key,
                down,
                modifiers,
            } => {
                let mut p = self.pressed.lock().map_err(|_| StreamError::NotRunning)?;
                if down {
                    p.keys.insert(key, modifiers);
                } else {
                    p.keys.remove(&key);
                }
                unsafe { kite_ml_key(key, i32::from(down), modifiers) }
            }
            Input::Scroll {
                horizontal,
                vertical,
            } => unsafe { kite_ml_scroll(horizontal, vertical) },
            Input::Text(text) if text.len() <= 4096 => unsafe {
                kite_ml_text(text.as_ptr(), text.len() as u32)
            },
            Input::ReleaseAll => {
                self.release();
                0
            }
            Input::RequestIdr => {
                unsafe {
                    kite_ml_request_idr();
                }
                0
            }
            _ => return Err(StreamError::Protocol("invalid input event".into())),
        };
        if result == 0 {
            Ok(())
        } else {
            Err(StreamError::Native {
                stage: 0,
                code: result,
            })
        }
    }
}

pub struct NativeSession {
    context: Arc<Context>,
    sender: Arc<Sender>,
    stopped: AtomicBool,
}
#[derive(Clone, Copy, Debug)]
pub struct Stats {
    pub stage: i32,
    pub error: i32,
    pub terminated: bool,
    pub received: u64,
    pub rendered: u64,
    pub decoder_error: i32,
}

impl NativeSession {
    /// Blocking, cancellable via interrupt_pending(). The application owns its worker.
    pub fn connect(
        launch: Launch,
        config: StreamConfig,
        sink: Arc<dyn MediaSink>,
    ) -> Result<Self, StreamError> {
        config.validate()?;
        let strings = [
            launch.address,
            launch.app_version,
            launch.gfe_version,
            launch.rtsp_url,
        ]
        .into_iter()
        .map(CString::new)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| StreamError::Protocol("invalid native string".into()))?;
        if ACTIVE
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(StreamError::Busy);
        }
        let context = Arc::new(Context {
            sink,
            stage: AtomicI32::new(0),
            error: AtomicI32::new(0),
            terminated: AtomicBool::new(false),
            received: AtomicU64::new(0),
        });
        let callbacks = Callbacks {
            configure,
            frame,
            stage,
            terminated,
        };
        let result = unsafe {
            kite_ml_start(
                strings[0].as_ptr(),
                strings[1].as_ptr(),
                strings[2].as_ptr(),
                strings[3].as_ptr(),
                launch.codec_modes,
                config.width as i32,
                config.height as i32,
                config.fps as i32,
                config.bitrate as i32,
                launch.key.as_ptr(),
                launch.iv.as_ptr(),
                Arc::as_ptr(&context) as *mut c_void,
                &callbacks,
            )
        };
        if result != 0 {
            context.sink.stop();
            ACTIVE.store(false, Ordering::SeqCst);
            return Err(StreamError::Native {
                stage: context.stage.load(Ordering::Relaxed),
                code: result,
            });
        }
        let sender = Arc::new(Sender {
            active: RwLock::new(true),
            pressed: Mutex::new(Pressed::default()),
        });
        context.sink.set_input(Some(sender.clone()));
        Ok(Self {
            context,
            sender,
            stopped: AtomicBool::new(false),
        })
    }
    pub fn stats(&self) -> Stats {
        Stats {
            stage: self.context.stage.load(Ordering::Relaxed),
            error: self.context.error.load(Ordering::Relaxed),
            terminated: self.context.terminated.load(Ordering::Relaxed),
            received: self.context.received.load(Ordering::Relaxed),
            rendered: self.context.sink.rendered_frames(),
            decoder_error: self.context.sink.failure_code(),
        }
    }
    pub fn stop(&self) {
        if !self.stopped.swap(true, Ordering::SeqCst) {
            self.context.sink.set_input(None);
            self.sender.close();
            unsafe {
                kite_ml_stop();
            }
            self.context.sink.stop();
            ACTIVE.store(false, Ordering::SeqCst);
        }
    }
}
impl Drop for NativeSession {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn interrupt_pending() {
    unsafe {
        kite_ml_interrupt();
    }
}
pub fn launch_query() -> String {
    unsafe {
        let p = kite_ml_launch_query();
        if p.is_null() {
            String::new()
        } else {
            CStr::from_ptr(p).to_string_lossy().into_owned()
        }
    }
}

unsafe extern "C" fn configure(context: *mut c_void, width: i32, height: i32, fps: i32) -> i32 {
    if context.is_null() || width <= 0 || height <= 0 || fps <= 0 {
        return 0;
    }
    let c = unsafe { &*(context as *const Context) };
    i32::from(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            c.sink.configure(VideoFormat {
                width: width as u32,
                height: height as u32,
                fps: fps as u32,
            })
        }))
        .unwrap_or(false),
    )
}
unsafe extern "C" fn frame(
    context: *mut c_void,
    data: *const u8,
    length: i32,
    pts: u64,
    flags: u32,
) -> i32 {
    if context.is_null() || data.is_null() || length <= 0 || length > 8 * 1024 * 1024 {
        return 0;
    }
    let c = unsafe { &*(context as *const Context) };
    let bytes = unsafe { std::slice::from_raw_parts(data, length as usize) };
    c.received.fetch_add(
        u64::from(flags & crate::FRAME_CONFIG == 0),
        Ordering::Relaxed,
    );
    i32::from(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            c.sink.submit(
                bytes,
                FrameInfo {
                    presentation_us: pts,
                    flags,
                },
            )
        }))
        .unwrap_or(false),
    )
}
unsafe extern "C" fn stage(context: *mut c_void, stage: i32, error: i32) {
    if !context.is_null() {
        let c = unsafe { &*(context as *const Context) };
        c.stage.store(stage, Ordering::Relaxed);
        if error != 0 {
            c.error.store(error, Ordering::Relaxed);
        }
    }
}
unsafe extern "C" fn terminated(context: *mut c_void, error: i32) {
    if !context.is_null() {
        let c = unsafe { &*(context as *const Context) };
        c.error.store(error, Ordering::Relaxed);
        c.terminated.store(true, Ordering::Relaxed);
    }
}
