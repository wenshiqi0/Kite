//! Apple media ABI. Native code owns decoders; handles own callback lifetimes.
use kite_app::{AudioFormat, FrameInfo, Input, InputSender, MediaSink, VideoFormat};
use std::{
    collections::BTreeMap,
    ffi::c_void,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicI32, AtomicU64, Ordering},
    },
};

/// Kept in sync with include/kite_apple_media.h. All callbacks are required.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Callbacks {
    release: Option<unsafe extern "C" fn(*mut c_void)>,
    supports: Option<unsafe extern "C" fn(*mut c_void, u32, u32, u32) -> u8>,
    configure: Option<unsafe extern "C" fn(*mut c_void, u32, u32, u32, u64) -> u8>,
    frame: Option<unsafe extern "C" fn(*mut c_void, *const u8, u32, u64, u32, u64) -> u8>,
    audio_configure: Option<unsafe extern "C" fn(*mut c_void, u32, u32, u32, u64)>,
    audio_frame: Option<unsafe extern "C" fn(*mut c_void, *const u8, u32, u64)>,
    stopped: Option<unsafe extern "C" fn(*mut c_void, u64)>,
}
impl Callbacks {
    fn valid(&self) -> bool {
        self.release.is_some()
            && self.supports.is_some()
            && self.configure.is_some()
            && self.frame.is_some()
            && self.audio_configure.is_some()
            && self.audio_frame.is_some()
            && self.stopped.is_some()
    }
}

#[derive(Default)]
struct InputState {
    sender: Option<Arc<dyn InputSender>>,
    enabled: bool,
}
struct AppleSink {
    // The native owner is retained at registration and released in Drop. Native
    // callbacks are thread safe and must never synchronously wait on the UI.
    context: usize,
    callbacks: Callbacks,
    input: Mutex<InputState>,
    generation: AtomicU64,
    rendered: AtomicU64,
    error: AtomicI32,
}
impl AppleSink {
    fn context(&self) -> *mut c_void {
        self.context as *mut c_void
    }
    fn input(&self, event: Input) -> bool {
        // Serialize gate transitions, release and send: a late event cannot
        // re-press a key after set_takeover(false) has released it.
        self.input.lock().is_ok_and(|state| {
            state.enabled && state.sender.as_ref().is_some_and(|s| s.send(event).is_ok())
        })
    }
    fn release_input(&self) {
        if let Ok(state) = self.input.lock()
            && let Some(sender) = &state.sender
        {
            let _ = sender.send(Input::ReleaseAll);
        }
    }
}
impl Drop for AppleSink {
    fn drop(&mut self) {
        self.release_input();
        // SAFETY: registration requires a valid retained context and callback.
        unsafe { (self.callbacks.release.unwrap())(self.context()) }
    }
}
impl MediaSink for AppleSink {
    fn supports(&self, f: VideoFormat) -> bool {
        unsafe { (self.callbacks.supports.unwrap())(self.context(), f.width, f.height, f.fps) != 0 }
    }
    fn configure(&self, f: VideoFormat) -> bool {
        self.rendered.store(0, Ordering::Relaxed);
        self.error.store(0, Ordering::Relaxed);
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        unsafe {
            (self.callbacks.configure.unwrap())(
                self.context(),
                f.width,
                f.height,
                f.fps,
                generation,
            ) != 0
        }
    }
    fn submit(&self, data: &[u8], info: FrameInfo) -> bool {
        if data.is_empty() || data.len() > 8 * 1024 * 1024 || self.failure_code() != 0 {
            return false;
        }
        unsafe {
            (self.callbacks.frame.unwrap())(
                self.context(),
                data.as_ptr(),
                data.len() as u32,
                info.presentation_us,
                info.flags,
                self.generation.load(Ordering::Acquire),
            ) != 0
        }
    }
    fn configure_audio(&self, f: AudioFormat) {
        unsafe {
            (self.callbacks.audio_configure.unwrap())(
                self.context(),
                f.sample_rate,
                f.channels,
                f.samples_per_frame,
                self.generation.load(Ordering::Acquire),
            )
        }
    }
    fn submit_audio(&self, data: &[u8]) {
        if !data.is_empty() && data.len() <= 4096 {
            unsafe {
                (self.callbacks.audio_frame.unwrap())(
                    self.context(),
                    data.as_ptr(),
                    data.len() as u32,
                    self.generation.load(Ordering::Acquire),
                )
            }
        }
    }
    fn set_input(&self, sender: Option<Arc<dyn InputSender>>) {
        if let Ok(mut state) = self.input.lock() {
            if let Some(old) = state.sender.take() {
                let _ = old.send(Input::ReleaseAll);
            }
            state.enabled = false;
            state.sender = sender;
        }
    }
    fn rendered_frames(&self) -> u64 {
        self.rendered.load(Ordering::Relaxed)
    }
    fn failure_code(&self) -> i32 {
        self.error.load(Ordering::Relaxed)
    }
    fn set_takeover(&self, enabled: bool) {
        if let Ok(mut state) = self.input.lock() {
            state.enabled = enabled;
            if !enabled && let Some(sender) = &state.sender {
                let _ = sender.send(Input::ReleaseAll);
            }
        }
    }
    fn stop(&self) {
        self.set_takeover(false);
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        unsafe { (self.callbacks.stopped.unwrap())(self.context(), generation) }
    }
}

static NEXT: AtomicU64 = AtomicU64::new(1);
fn targets() -> &'static Mutex<BTreeMap<u64, Arc<AppleSink>>> {
    static TARGETS: OnceLock<Mutex<BTreeMap<u64, Arc<AppleSink>>>> = OnceLock::new();
    TARGETS.get_or_init(|| Mutex::new(BTreeMap::new()))
}
fn get(id: u64) -> Option<Arc<AppleSink>> {
    targets().lock().ok()?.get(&id).cloned()
}
pub(crate) fn media_sink(id: u64) -> Option<Arc<dyn MediaSink>> {
    get(id).map(|s| s as Arc<dyn MediaSink>)
}

/// On success takes ownership of one native retain; on failure takes none.
/// The callbacks may outlive detach while a stream still holds the sink.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kite_apple_media_register(
    context: *mut c_void,
    callbacks: *const Callbacks,
) -> u64 {
    if context.is_null() || callbacks.is_null() {
        return 0;
    }
    let callbacks = unsafe { *callbacks };
    if !callbacks.valid() {
        return 0;
    }
    let Ok(mut map) = targets().lock() else {
        return 0;
    };
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    map.insert(
        id,
        Arc::new(AppleSink {
            context: context as usize,
            callbacks,
            input: Mutex::new(InputState::default()),
            generation: AtomicU64::new(0),
            rendered: AtomicU64::new(0),
            error: AtomicI32::new(0),
        }),
    );
    id
}
#[unsafe(no_mangle)]
pub extern "C" fn kite_apple_media_detach(id: u64) {
    // Never run native release under the registry lock.
    let sink = targets().lock().ok().and_then(|mut map| map.remove(&id));
    if let Some(sink) = sink {
        sink.set_takeover(false);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn kite_apple_media_frame_rendered(id: u64, generation: u64) {
    if let Some(sink) = get(id)
        && sink.generation.load(Ordering::Acquire) == generation
    {
        sink.rendered.fetch_add(1, Ordering::Relaxed);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn kite_apple_media_decoder_failed(id: u64, generation: u64, error: i32) {
    if let Some(sink) = get(id)
        && sink.generation.load(Ordering::Acquire) == generation
    {
        sink.error
            .store(if error == 0 { 1 } else { error }, Ordering::Relaxed);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn kite_apple_media_mouse_position(
    id: u64,
    x: i16,
    y: i16,
    width: i16,
    height: i16,
) -> u8 {
    u8::from(
        width > 0
            && height > 0
            && get(id).is_some_and(|s| {
                s.input(Input::Position {
                    x,
                    y,
                    width,
                    height,
                })
            }),
    )
}
#[unsafe(no_mangle)]
pub extern "C" fn kite_apple_media_mouse_button(id: u64, button: u8, down: u8) -> u8 {
    u8::from(
        (1..=5).contains(&button)
            && get(id).is_some_and(|s| {
                s.input(Input::MouseButton {
                    button,
                    down: down != 0,
                })
            }),
    )
}
#[unsafe(no_mangle)]
pub extern "C" fn kite_apple_media_scroll(id: u64, horizontal: i16, vertical: i16) -> u8 {
    u8::from(get(id).is_some_and(|s| {
        s.input(Input::Scroll {
            horizontal,
            vertical,
        })
    }))
}
#[unsafe(no_mangle)]
pub extern "C" fn kite_apple_media_key(id: u64, key: i16, down: u8, modifiers: u8) -> u8 {
    u8::from(get(id).is_some_and(|s| {
        s.input(Input::Key {
            key,
            down: down != 0,
            modifiers: modifiers & 31,
        })
    }))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kite_apple_media_text(id: u64, text: *const u8, length: u32) -> u8 {
    if text.is_null() || length == 0 || length > 4096 {
        return 0;
    }
    let bytes = unsafe { std::slice::from_raw_parts(text, length as usize) };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return 0;
    };
    u8::from(get(id).is_some_and(|s| s.input(Input::Text(text.to_owned()))))
}
#[unsafe(no_mangle)]
pub extern "C" fn kite_apple_media_release_input(id: u64) {
    if let Some(sink) = get(id) {
        sink.release_input();
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn kite_apple_media_request_idr(id: u64) {
    if let Some(sink) = get(id)
        && let Ok(state) = sink.input.lock()
        && let Some(sender) = &state.sender
    {
        let _ = sender.send(Input::RequestIdr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    static RELEASED: AtomicUsize = AtomicUsize::new(0);
    unsafe extern "C" fn release(_: *mut c_void) {
        RELEASED.fetch_add(1, Ordering::SeqCst);
    }
    unsafe extern "C" fn supports(_: *mut c_void, _: u32, _: u32, _: u32) -> u8 {
        1
    }
    unsafe extern "C" fn configure(_: *mut c_void, _: u32, _: u32, _: u32, _: u64) -> u8 {
        1
    }
    unsafe extern "C" fn frame(_: *mut c_void, _: *const u8, _: u32, _: u64, _: u32, _: u64) -> u8 {
        1
    }
    unsafe extern "C" fn audio_configure(_: *mut c_void, _: u32, _: u32, _: u32, _: u64) {}
    unsafe extern "C" fn audio_frame(_: *mut c_void, _: *const u8, _: u32, _: u64) {}
    unsafe extern "C" fn stopped(_: *mut c_void, _: u64) {}
    struct Sender(Mutex<Vec<Input>>);
    impl InputSender for Sender {
        fn send(&self, event: Input) -> Result<(), kite_app::StreamError> {
            self.0.lock().unwrap().push(event);
            Ok(())
        }
    }

    #[test]
    fn detach_keeps_inflight_owner_and_rejects_stale_frames() {
        let callbacks = Callbacks {
            release: Some(release),
            supports: Some(supports),
            configure: Some(configure),
            frame: Some(frame),
            audio_configure: Some(audio_configure),
            audio_frame: Some(audio_frame),
            stopped: Some(stopped),
        };
        let before = RELEASED.load(Ordering::SeqCst);
        let id =
            unsafe { kite_apple_media_register(std::ptr::dangling_mut::<c_void>(), &callbacks) };
        let sink = get(id).unwrap();
        assert!(sink.configure(VideoFormat {
            width: 1920,
            height: 1080,
            fps: 60
        }));
        let sender = Arc::new(Sender(Mutex::new(Vec::new())));
        sink.set_input(Some(sender.clone()));
        assert_eq!(kite_apple_media_key(id, 65, 1, 0), 0);
        sink.set_takeover(true);
        assert_eq!(kite_apple_media_key(id, 65, 1, 0), 1);
        sink.set_takeover(false);
        assert_eq!(kite_apple_media_key(id, 66, 1, 0), 0);
        let events = sender.0.lock().unwrap();
        assert!(matches!(
            events.as_slice(),
            [
                Input::Key {
                    key: 65,
                    down: true,
                    ..
                },
                Input::ReleaseAll
            ]
        ));
        drop(events);
        kite_apple_media_frame_rendered(id, 1);
        assert_eq!(sink.rendered_frames(), 1);
        sink.stop();
        kite_apple_media_frame_rendered(id, 1);
        kite_apple_media_decoder_failed(id, 1, 9);
        assert_eq!(sink.rendered_frames(), 1);
        assert_eq!(sink.failure_code(), 0);
        kite_apple_media_detach(id);
        assert!(media_sink(id).is_none());
        assert_eq!(RELEASED.load(Ordering::SeqCst), before);
        assert!(sink.submit(
            &[1],
            FrameInfo {
                presentation_us: 0,
                flags: 0
            }
        ));
        drop(sink);
        assert_eq!(RELEASED.load(Ordering::SeqCst), before + 1);
        assert_eq!(kite_apple_media_mouse_button(id, 1, 1), 0);
    }
}
