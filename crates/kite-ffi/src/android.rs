//! Android media only: preallocated direct buffers, decoder callbacks and input.
use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JByteBuffer, JClass, JObject, JObjectArray, JString, JValue},
    sys::{jboolean, jint, jlong},
};
use kite_app::{FrameInfo, Input, InputSender, MediaSink, VideoFormat};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering},
    },
};

const CAPACITY: usize = 8 * 1024 * 1024;
static NEXT: AtomicU64 = AtomicU64::new(1);
type Targets = BTreeMap<u64, Arc<AndroidSink>>;
fn targets() -> &'static Mutex<Targets> {
    static TARGETS: OnceLock<Mutex<Targets>> = OnceLock::new();
    TARGETS.get_or_init(|| Mutex::new(BTreeMap::new()))
}
fn get(id: jlong) -> Option<Arc<AndroidSink>> {
    targets().lock().ok()?.get(&(id as u64)).cloned()
}
pub(crate) fn media_sink(id: u64) -> Option<Arc<dyn MediaSink>> {
    get(id as jlong).map(|v| v as Arc<dyn MediaSink>)
}
struct Slot {
    address: usize,
    capacity: usize,
    _owner: GlobalRef,
    busy: AtomicBool,
}
struct AndroidSink {
    vm: JavaVM,
    renderer: GlobalRef,
    slots: Vec<Slot>,
    input: Mutex<Option<Arc<dyn InputSender>>>,
    enabled: AtomicBool,
    rendered: AtomicU64,
    error: AtomicI32,
    generation: AtomicU64,
}

impl AndroidSink {
    fn env(&self) -> jni::errors::Result<JNIEnv<'_>> {
        match self.vm.get_env() {
            Ok(env) => Ok(env),
            Err(jni::errors::Error::JniCall(jni::errors::JniError::ThreadDetached)) => {
                // UniFFI's JNA callbacks can detach a Tokio worker after a prior
                // media call. jni-rs 0.21 still has that thread's TLS guard. Clear
                // it BEFORE attaching again: replacing it afterward drops the old
                // guard and detaches the newly returned JNIEnv (ART aborts).
                // SAFETY: GetEnv confirmed that this thread is already detached;
                // there is no valid JNIEnv or attachment for this call to invalidate.
                unsafe {
                    self.vm.detach_current_thread();
                }
                self.vm.attach_current_thread_permanently()
            }
            Err(error) => Err(error),
        }
    }
    fn call(&self, name: &str, signature: &str, args: &[JValue<'_, '_>]) -> bool {
        let Ok(mut env) = self.env() else {
            return false;
        };
        match env.call_method(self.renderer.as_obj(), name, signature, args) {
            Ok(v) => v.z().unwrap_or(false),
            Err(_) => {
                let _ = env.exception_clear();
                self.error.store(1, Ordering::Relaxed);
                false
            }
        }
    }
    fn sender(&self) -> Option<Arc<dyn InputSender>> {
        self.input.lock().ok()?.clone()
    }
    fn input(&self, event: Input) -> bool {
        self.enabled.load(Ordering::Acquire) && self.sender().is_some_and(|s| s.send(event).is_ok())
    }
    fn release(&self, index: jint) {
        if index >= 0
            && let Some(slot) = self.slots.get(index as usize)
        {
            slot.busy.store(false, Ordering::Release);
        }
    }
}
impl Drop for AndroidSink {
    fn drop(&mut self) {
        // Field destructors release GlobalRefs on this thread too. Repair a stale
        // attachment before jni-rs attempts its own scoped attach for those drops.
        let _ = self.env();
    }
}
impl MediaSink for AndroidSink {
    fn supports(&self, f: VideoFormat) -> bool {
        self.call(
            "supportsFormat",
            "(III)Z",
            &[
                JValue::Int(f.width as jint),
                JValue::Int(f.height as jint),
                JValue::Int(f.fps as jint),
            ],
        )
    }
    fn configure(&self, f: VideoFormat) -> bool {
        self.rendered.store(0, Ordering::Relaxed);
        self.error.store(0, Ordering::Relaxed);
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.call(
            "onFormat",
            "(IIIJ)Z",
            &[
                JValue::Int(f.width as jint),
                JValue::Int(f.height as jint),
                JValue::Int(f.fps as jint),
                JValue::Long(generation as jlong),
            ],
        )
    }
    fn submit(&self, data: &[u8], info: FrameInfo) -> bool {
        if self.error.load(Ordering::Relaxed) != 0 {
            return false;
        }
        for (index, slot) in self.slots.iter().enumerate() {
            if data.len() > slot.capacity {
                continue;
            }
            if slot
                .busy
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
                .is_err()
            {
                continue;
            }
            // ByteBuffer owners stay global until the C transport has joined its callbacks.
            unsafe {
                std::ptr::copy_nonoverlapping(data.as_ptr(), slot.address as *mut u8, data.len());
            }
            let accepted = self.call(
                "onFrame",
                "(IIJI)Z",
                &[
                    JValue::Int(index as jint),
                    JValue::Int(data.len() as jint),
                    JValue::Long(info.presentation_us.min(i64::MAX as u64) as jlong),
                    JValue::Int(info.flags as jint),
                ],
            );
            if !accepted {
                slot.busy.store(false, Ordering::Release);
            }
            return accepted;
        }
        false
    }
    fn set_input(&self, sender: Option<Arc<dyn InputSender>>) {
        if sender.is_none() {
            self.set_takeover(false);
        }
        if let Ok(mut input) = self.input.lock() {
            *input = sender;
        }
    }
    fn rendered_frames(&self) -> u64 {
        self.rendered.load(Ordering::Relaxed)
    }
    fn failure_code(&self) -> i32 {
        self.error.load(Ordering::Relaxed)
    }
    fn set_takeover(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Release);
        if !enabled && let Some(sender) = self.sender() {
            let _ = sender.send(Input::ReleaseAll);
        }
    }
    fn stop(&self) {
        self.set_takeover(false);
        self.generation.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut env) = self.env() {
            if env
                .call_method(self.renderer.as_obj(), "onStopped", "()V", &[])
                .is_err()
            {
                let _ = env.exception_clear();
            }
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_wenshiqi0_kite_media_NativeMedia_register(
    mut env: JNIEnv,
    _class: JClass,
    renderer: JObject,
    buffers: JObjectArray,
) -> jlong {
    let result = (|| -> jni::errors::Result<Arc<AndroidSink>> {
        let count = env.get_array_length(&buffers)?;
        if count != 3 {
            return Err(jni::errors::Error::NullPtr("three direct buffers required"));
        }
        let mut slots = Vec::with_capacity(3);
        for index in 0..count {
            let object = env.get_object_array_element(&buffers, index)?;
            let buffer = JByteBuffer::from(object);
            let capacity = env.get_direct_buffer_capacity(&buffer)?;
            let address = env.get_direct_buffer_address(&buffer)? as usize;
            if capacity < CAPACITY || address == 0 {
                return Err(jni::errors::Error::NullPtr("direct buffer too small"));
            }
            slots.push(Slot {
                address,
                capacity,
                _owner: env.new_global_ref(buffer)?,
                busy: AtomicBool::new(false),
            });
        }
        Ok(Arc::new(AndroidSink {
            vm: env.get_java_vm()?,
            renderer: env.new_global_ref(renderer)?,
            slots,
            input: Mutex::new(None),
            enabled: AtomicBool::new(false),
            rendered: AtomicU64::new(0),
            error: AtomicI32::new(0),
            generation: AtomicU64::new(0),
        }))
    })();
    match result {
        Ok(sink) => {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            if let Ok(mut map) = targets().lock() {
                map.insert(id, sink);
                id as jlong
            } else {
                0
            }
        }
        Err(_) => {
            let _ = env.exception_clear();
            0
        }
    }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_wenshiqi0_kite_media_NativeMedia_detach(
    _env: JNIEnv,
    _class: JClass,
    id: jlong,
) {
    if let Some(sink) = get(id) {
        sink.set_takeover(false);
    }
    if let Ok(mut map) = targets().lock() {
        map.remove(&(id as u64));
    }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_wenshiqi0_kite_media_NativeMedia_releaseBuffer(
    _env: JNIEnv,
    _class: JClass,
    id: jlong,
    index: jint,
) {
    if let Some(sink) = get(id) {
        sink.release(index);
    }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_wenshiqi0_kite_media_NativeMedia_frameRendered(
    _env: JNIEnv,
    _class: JClass,
    id: jlong,
    generation: jlong,
) {
    if let Some(sink) = get(id)
        && sink.generation.load(Ordering::Acquire) == generation as u64
    {
        sink.rendered.fetch_add(1, Ordering::Relaxed);
    }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_wenshiqi0_kite_media_NativeMedia_decoderFailed(
    _env: JNIEnv,
    _class: JClass,
    id: jlong,
    generation: jlong,
) {
    if let Some(sink) = get(id)
        && sink.generation.load(Ordering::Acquire) == generation as u64
    {
        sink.error.store(1, Ordering::Relaxed);
    }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_wenshiqi0_kite_media_NativeMedia_requestIdr(
    _env: JNIEnv,
    _class: JClass,
    id: jlong,
) {
    if let Some(sink) = get(id)
        && let Some(sender) = sink.sender()
    {
        let _ = sender.send(Input::RequestIdr);
    }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_wenshiqi0_kite_media_NativeMedia_mousePosition(
    _env: JNIEnv,
    _class: JClass,
    id: jlong,
    x: jint,
    y: jint,
    width: jint,
    height: jint,
) -> jboolean {
    let Ok((x, y, width, height)) = (|| -> Result<_, std::num::TryFromIntError> {
        Ok((
            i16::try_from(x)?,
            i16::try_from(y)?,
            i16::try_from(width)?,
            i16::try_from(height)?,
        ))
    })() else {
        return 0;
    };
    u8::from(get(id).is_some_and(|s| {
        s.input(Input::Position {
            x,
            y,
            width,
            height,
        })
    }))
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_wenshiqi0_kite_media_NativeMedia_mouseMove(
    _env: JNIEnv,
    _class: JClass,
    id: jlong,
    x: jint,
    y: jint,
) -> jboolean {
    u8::from(get(id).is_some_and(|s| {
        s.input(Input::Relative {
            x: x.clamp(-32767, 32767) as i16,
            y: y.clamp(-32767, 32767) as i16,
        })
    }))
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_wenshiqi0_kite_media_NativeMedia_mouseButton(
    _env: JNIEnv,
    _class: JClass,
    id: jlong,
    button: jint,
    down: jboolean,
) -> jboolean {
    u8::from(get(id).is_some_and(|s| {
        s.input(Input::MouseButton {
            button: button as u8,
            down: down != 0,
        })
    }))
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_wenshiqi0_kite_media_NativeMedia_scroll(
    _env: JNIEnv,
    _class: JClass,
    id: jlong,
    horizontal: jint,
    vertical: jint,
) -> jboolean {
    u8::from(get(id).is_some_and(|s| {
        s.input(Input::Scroll {
            horizontal: horizontal.clamp(-32767, 32767) as i16,
            vertical: vertical.clamp(-32767, 32767) as i16,
        })
    }))
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_wenshiqi0_kite_media_NativeMedia_key(
    _env: JNIEnv,
    _class: JClass,
    id: jlong,
    key: jint,
    down: jboolean,
    modifiers: jint,
) -> jboolean {
    u8::from(get(id).is_some_and(|s| {
        s.input(Input::Key {
            key: key as i16,
            down: down != 0,
            modifiers: (modifiers & 31) as u8,
        })
    }))
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_wenshiqi0_kite_media_NativeMedia_text(
    mut env: JNIEnv,
    _class: JClass,
    id: jlong,
    text: JString,
) -> jboolean {
    let Ok(value) = env.get_string(&text) else {
        let _ = env.exception_clear();
        return 0;
    };
    let value: String = value.into();
    u8::from(get(id).is_some_and(|s| s.input(Input::Text(value))))
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_wenshiqi0_kite_media_NativeMedia_releaseInput(
    _env: JNIEnv,
    _class: JClass,
    id: jlong,
) {
    if let Some(s) = get(id)
        && let Some(sender) = s.sender()
    {
        let _ = sender.send(Input::ReleaseAll);
    }
}
