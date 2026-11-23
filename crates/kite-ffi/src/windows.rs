//! In-process Windows commands. No listener, host runtime, or second Tokio runtime.
use crate::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::c_void,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Callbacks {
    release: Option<unsafe extern "C" fn(*mut c_void)>,
    changed: Option<unsafe extern "C" fn(*mut c_void, u64)>,
    read: Option<unsafe extern "C" fn(*mut c_void, *const u8, u32, *mut u8, u32) -> i32>,
    write: Option<unsafe extern "C" fn(*mut c_void, *const u8, u32, *const u8, u32) -> i32>,
}
struct Owner {
    context: usize,
    callbacks: Callbacks,
}
impl Drop for Owner {
    fn drop(&mut self) {
        // SAFETY: successful registration owns one native retain.
        unsafe { (self.callbacks.release.unwrap())(self.context as *mut c_void) }
    }
}
struct Secrets(Arc<Owner>);
impl SecretStore for Secrets {
    fn read(&self, name: String) -> Result<Option<Vec<u8>>, SecretStoreError> {
        let mut bytes = vec![0; 64 * 1024];
        let length = unsafe {
            (self.0.callbacks.read.unwrap())(
                self.0.context as *mut c_void,
                name.as_ptr(),
                name.len() as u32,
                bytes.as_mut_ptr(),
                bytes.len() as u32,
            )
        };
        if length == -1 {
            return Ok(None);
        }
        if length < 0 || length as usize > bytes.len() {
            return Err(SecretStoreError::Failure {
                details: "Windows secret read failed".into(),
            });
        }
        bytes.truncate(length as usize);
        Ok(Some(bytes))
    }
    fn write(&self, name: String, value: Vec<u8>) -> Result<(), SecretStoreError> {
        let result = unsafe {
            (self.0.callbacks.write.unwrap())(
                self.0.context as *mut c_void,
                name.as_ptr(),
                name.len() as u32,
                value.as_ptr(),
                value.len() as u32,
            )
        };
        if result != 0 {
            return Err(SecretStoreError::Failure {
                details: "Windows secret write failed".into(),
            });
        }
        Ok(())
    }
}
struct Observer(Arc<Owner>);
impl CoreObserver for Observer {
    fn state_changed(&self, snapshot: CoreSnapshot) {
        unsafe {
            (self.0.callbacks.changed.unwrap())(self.0.context as *mut c_void, snapshot.revision)
        }
    }
}
fn clients() -> &'static Mutex<BTreeMap<u64, Arc<KiteCore>>> {
    static CLIENTS: OnceLock<Mutex<BTreeMap<u64, Arc<KiteCore>>>> = OnceLock::new();
    CLIENTS.get_or_init(Default::default)
}

/// Takes one native retain on success; none on failure.
/// # Safety
/// Callbacks/context must remain valid until the final release callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kite_windows_create(
    context: *mut c_void,
    callbacks: *const Callbacks,
) -> u64 {
    catch_unwind(AssertUnwindSafe(|| {
        if context.is_null() || callbacks.is_null() {
            return 0;
        }
        let callbacks = unsafe { *callbacks };
        if callbacks.release.is_none()
            || callbacks.changed.is_none()
            || callbacks.read.is_none()
            || callbacks.write.is_none()
        {
            return 0;
        }
        let Ok(mut clients) = clients().lock() else {
            return 0;
        };
        let owner = Arc::new(Owner {
            context: context as usize,
            callbacks,
        });
        let Ok(core) = KiteCore::new(Box::new(Secrets(owner.clone())), Box::new(Observer(owner)))
        else {
            return 0;
        };
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        clients.insert(id, core);
        id
    }))
    .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn kite_windows_destroy(id: u64) {
    let removed = clients()
        .lock()
        .ok()
        .and_then(|mut clients| clients.remove(&id));
    // Native release must run outside the registry lock. In-flight requests own an Arc.
    drop(removed);
}

#[repr(C)]
pub struct Buffer {
    data: *mut u8,
    length: usize,
}
impl Buffer {
    fn new(bytes: Vec<u8>) -> Self {
        let mut bytes = bytes.into_boxed_slice();
        let result = Self {
            data: bytes.as_mut_ptr(),
            length: bytes.len(),
        };
        std::mem::forget(bytes);
        result
    }
}
/// # Safety
/// Pass the exact, still-owned allocation returned by kite_windows_request once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kite_windows_buffer_free(buffer: Buffer) {
    if !buffer.data.is_null() {
        drop(unsafe {
            Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                buffer.data,
                buffer.length,
            ))
        });
    }
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    Initialize {
        data_directory: String,
        device_name: String,
    },
    Snapshot {
        after_revision: Option<u64>,
    },
    Conversation {
        context: String,
        revision: u64,
    },
    SaveHost {
        host: HostRecord,
    },
    Connect {
        host_id: String,
    },
    SelectHost {
        host_id: String,
    },
    DisconnectHost {
        host_id: String,
    },
    DisconnectAll {},
    RestoreConnection {},
    NetworkChanged {
        available: bool,
    },
    TrustHost {
        host_id: String,
        fingerprint: String,
    },
    ForgetHost {
        host_id: String,
    },
    RemoveHost {
        host_id: String,
    },
    RefreshHostResources {
        host_id: String,
    },
    SetAgentMemoryLimit {
        host_id: String,
        bytes: u64,
    },
    SetSessionSort {
        host_id: String,
        order: SessionSortOrder,
    },
    SetWorkspaceSort {
        order: SessionSortOrder,
    },
    CreateSession {
        host_id: String,
        provider: String,
        cwd: String,
        title: String,
    },
    OpenSession {
        host_id: String,
        session_id: String,
    },
    InspectSubagent {
        host_id: String,
        parent_id: String,
        child_id: String,
    },
    ResumeSession {
        host_id: String,
        session_id: String,
    },
    ArchiveSession {
        host_id: String,
        session_id: String,
        archived: bool,
    },
    DeleteSession {
        host_id: String,
        session_id: String,
    },
    EnsureAgent {
        host_id: String,
    },
    PromptAgent {
        host_id: String,
        agent_id: String,
        text: String,
    },
    InterruptAgent {
        host_id: String,
        agent_id: String,
    },
    SetAgentPaused {
        host_id: String,
        agent_id: String,
        paused: bool,
    },
    RespondApproval {
        host_id: String,
        agent_id: String,
        request_id: String,
        choice: u32,
    },
    AnswerQuestion {
        host_id: String,
        agent_id: String,
        request_id: String,
        answers: Vec<QuestionAnswerInfo>,
    },
    SetChatModel {
        host_id: String,
        agent_id: String,
        model: String,
        effort: String,
    },
    SetChatSetting {
        host_id: String,
        agent_id: String,
        setting: String,
        value: String,
    },
    RefreshSkills {
        agent_id: String,
    },
    SelectSkill {
        host_id: String,
        agent_id: String,
        path: String,
        selected: bool,
    },
    UploadAttachment {
        host_id: String,
        agent_id: String,
        local_path: String,
        name: String,
        mime: String,
        retry_id: Option<String>,
    },
    RemoveAttachment {
        host_id: String,
        agent_id: String,
        upload_id: String,
    },
    Attachment {
        context: String,
        id: String,
    },
    RefreshAccountUsage {},
    ReloadComposer {},
    RetryHistory {},
    StartDesktop {
        host_id: String,
        media_target: u64,
    },
    StopDesktop {
        host_id: String,
    },
    ApprovePairing {
        host_id: String,
        pin: String,
    },
    SetTakeover {
        host_id: String,
        enabled: bool,
    },
    SynchronizeClipboard {
        host_id: String,
        local: Option<String>,
    },
    ClearFailure {},
}
fn value(value: impl serde::Serialize) -> Result<Value, KiteError> {
    serde_json::to_value(value).map_err(|_| KiteError::InvalidState {
        details: "native serialization failed".into(),
    })
}
async fn run(core: Arc<KiteCore>, command: Command) -> Result<Value, KiteError> {
    use Command::*;
    match command {
        Initialize {
            data_directory,
            device_name,
        } => {
            core.initialize(
                data_directory,
                device_name,
                LegacyData {
                    source: "windows".into(),
                    host: None,
                    trusted_keys: vec![],
                },
            )
            .await?
        }
        Snapshot { after_revision } => {
            let snapshot = core.snapshot()?;
            return if after_revision == Some(snapshot.revision) {
                Ok(Value::Null)
            } else {
                value(snapshot)
            };
        }
        Conversation { context, revision } => {
            return value(core.conversation_since(context, revision));
        }
        SaveHost { host } => return value(core.save_host(host).await?),
        Connect { host_id } => core.connect(host_id).await?,
        SelectHost { host_id } => core.select_host(host_id).await?,
        DisconnectHost { host_id } => core.disconnect_host(host_id).await?,
        DisconnectAll {} => core.disconnect_all().await?,
        RestoreConnection {} => core.restore_connection().await?,
        NetworkChanged { available } => core.network_changed(available)?,
        TrustHost {
            host_id,
            fingerprint,
        } => core.trust_host(host_id, fingerprint).await?,
        ForgetHost { host_id } => core.forget_host(host_id).await?,
        RemoveHost { host_id } => core.remove_host(host_id).await?,
        RefreshHostResources { host_id } => core.refresh_host_resources(host_id).await?,
        SetAgentMemoryLimit { host_id, bytes } => {
            core.set_agent_memory_limit(host_id, bytes).await?
        }
        SetSessionSort { host_id, order } => core.set_session_sort(host_id, order).await?,
        SetWorkspaceSort { order } => core.set_workspace_sort(order).await?,
        CreateSession {
            host_id,
            provider,
            cwd,
            title,
        } => core.create_session(host_id, provider, cwd, title).await?,
        OpenSession {
            host_id,
            session_id,
        } => core.open_workspace_session(host_id, session_id).await?,
        InspectSubagent {
            host_id,
            parent_id,
            child_id,
        } => {
            return value(core.inspect_subagent(host_id, parent_id, child_id).await?);
        }
        ResumeSession {
            host_id,
            session_id,
        } => core.resume_session(host_id, session_id).await?,
        ArchiveSession {
            host_id,
            session_id,
            archived,
        } => core.archive_session(host_id, session_id, archived).await?,
        DeleteSession {
            host_id,
            session_id,
        } => core.delete_session(host_id, session_id).await?,
        EnsureAgent { host_id } => core.ensure_agent(host_id).await?,
        PromptAgent {
            host_id,
            agent_id,
            text,
        } => core.prompt_agent(host_id, agent_id, text).await?,
        InterruptAgent { host_id, agent_id } => core.interrupt_agent(host_id, agent_id).await?,
        SetAgentPaused {
            host_id,
            agent_id,
            paused,
        } => core.set_agent_paused(host_id, agent_id, paused).await?,
        RespondApproval {
            host_id,
            agent_id,
            request_id,
            choice,
        } => {
            core.respond_approval(host_id, agent_id, request_id, choice)
                .await?
        }
        AnswerQuestion {
            host_id,
            agent_id,
            request_id,
            answers,
        } => {
            core.answer_question(host_id, agent_id, request_id, answers)
                .await?
        }
        SetChatModel {
            host_id,
            agent_id,
            model,
            effort,
        } => {
            core.set_chat_model(host_id, agent_id, model, effort)
                .await?
        }
        SetChatSetting {
            host_id,
            agent_id,
            setting,
            value: selection,
        } => {
            core.set_chat_setting(host_id, agent_id, setting, selection)
                .await?
        }
        RefreshSkills { agent_id } => core.refresh_skills(agent_id).await?,
        SelectSkill {
            host_id,
            agent_id,
            path,
            selected,
        } => core.select_skill(host_id, agent_id, path, selected).await?,
        UploadAttachment {
            host_id,
            agent_id,
            local_path,
            name,
            mime,
            retry_id,
        } => {
            core.upload_attachment(host_id, agent_id, local_path, name, mime, retry_id)
                .await?
        }
        RemoveAttachment {
            host_id,
            agent_id,
            upload_id,
        } => core.remove_attachment(host_id, agent_id, upload_id).await?,
        Attachment { context, id } => {
            return value(STANDARD.encode(core.chat_attachment(context, id).await?));
        }
        RefreshAccountUsage {} => core.refresh_account_usage().await?,
        ReloadComposer {} => core.reload_composer().await?,
        RetryHistory {} => core.retry_history().await?,
        StartDesktop {
            host_id,
            media_target,
        } => core.start_desktop(host_id, media_target).await?,
        StopDesktop { host_id } => core.stop_desktop(host_id).await?,
        ApprovePairing { host_id, pin } => core.approve_pairing(host_id, pin).await?,
        SetTakeover { host_id, enabled } => core.set_takeover(host_id, enabled).await?,
        SynchronizeClipboard { host_id, local } => {
            return value(core.synchronize_clipboard(host_id, local).await?);
        }
        ClearFailure {} => core.clear_failure().await?,
    }
    Ok(Value::Null)
}

fn request(id: u64, bytes: &[u8]) -> Value {
    let result = (|| {
        let core = clients()
            .lock()
            .ok()
            .and_then(|clients| clients.get(&id).cloned())
            .ok_or_else(|| "client is closed".to_owned())?;
        let command = serde_json::from_slice::<Command>(bytes).map_err(|e| e.to_string())?;
        // Blocks the caller's worker, but never creates a Tokio runtime here.
        pollster::block_on(run(core, command)).map_err(|e| e.to_string())
    })();
    match result {
        Ok(value) => json!({"ok": true, "value": value}),
        Err(error) => json!({"ok": false, "error": error}),
    }
}
/// # Safety
/// bytes must reference length readable bytes until this call returns.
/// Call on a native worker thread; release the returned buffer exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kite_windows_request(id: u64, bytes: *const u8, length: usize) -> Buffer {
    let response = catch_unwind(AssertUnwindSafe(|| {
        if bytes.is_null() || length == 0 || length > 1024 * 1024 {
            json!({"ok": false, "error": "invalid command size"})
        } else {
            request(id, unsafe { std::slice::from_raw_parts(bytes, length) })
        }
    }))
    .unwrap_or_else(|_| json!({"ok": false, "error": "native command failed"}));
    Buffer::new(
        serde_json::to_vec(&response)
            .unwrap_or_else(|_| br#"{"ok":false,"error":"serialization failed"}"#.to_vec()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unknown_commands_and_wrong_field_types() {
        assert!(serde_json::from_str::<Command>(r#"{"op":"start_local_node"}"#).is_err());
        assert!(serde_json::from_str::<Command>(r#"{"op":"connect","host_id":17}"#).is_err());
        assert!(
            serde_json::from_str::<Command>(r#"{"op":"disconnect_all","shell":"echo test"}"#)
                .is_err()
        );
    }
    #[test]
    fn closed_client_and_invalid_pointer_are_structured_errors() {
        assert_eq!(request(u64::MAX, b"{}")["ok"], false);
        let buffer = unsafe { kite_windows_request(0, std::ptr::null(), 1) };
        let bytes = unsafe { std::slice::from_raw_parts(buffer.data, buffer.length) };
        let error: Value = serde_json::from_slice(bytes).unwrap();
        assert_eq!(error["ok"], false);
        unsafe { kite_windows_buffer_free(buffer) };
    }
}
