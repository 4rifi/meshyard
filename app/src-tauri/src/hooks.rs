//! Explicit, reviewed user-scope installer. Construction/planning never writes.
use crate::view::{AppError, HookAction, HookPlan, HookView};
use meshyard_store::process_lock::settings_directory;
use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, Visitor},
};
use serde_json::{Value, json, value::RawValue};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fmt,
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

const EVENTS: [&str; 7] = [
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "Stop",
    "StopFailure",
    "SessionEnd",
];
const MAX_SETTINGS: u64 = 1024 * 1024;

pub struct Installer {
    settings: PathBuf,
    binary: Option<PathBuf>,
    socket: PathBuf,
    pending: Option<Pending>,
}
struct Pending {
    view: HookPlan,
    before: Option<Vec<u8>>,
    after: Vec<u8>,
    applied: bool,
}

// Borrow raw root values so bytes outside the hooks value (including key order and spacing)
// survive exactly. Duplicate root keys are ambiguous and therefore rejected.
struct Root<'a>(BTreeMap<String, &'a RawValue>);
impl<'de> Deserialize<'de> for Root<'de> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct RootVisitor;
        impl<'de> Visitor<'de> for RootVisitor {
            type Value = Root<'de>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a settings object with unique keys")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut values = BTreeMap::new();
                while let Some((key, value)) = map.next_entry::<String, &'de RawValue>()? {
                    if values.insert(key, value).is_some() {
                        return Err(serde::de::Error::custom("duplicate settings key"));
                    }
                }
                Ok(Root(values))
            }
        }
        deserializer.deserialize_map(RootVisitor)
    }
}
struct Document<'a> {
    text: &'a str,
    hooks: Value,
    range: Option<std::ops::Range<usize>>,
}
impl<'a> Document<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Self, AppError> {
        let text = std::str::from_utf8(bytes).map_err(|_| AppError::HookConfigInvalid)?;
        let root: Root<'_> = serde_json::from_str(text).map_err(|_| AppError::HookConfigInvalid)?;
        let (hooks, range) = if let Some(raw) = root.0.get("hooks") {
            let start = raw.get().as_ptr() as usize - text.as_ptr() as usize;
            let hooks: Value =
                serde_json::from_str(raw.get()).map_err(|_| AppError::HookConfigInvalid)?;
            if !hooks.is_object() {
                return Err(AppError::HookConfigInvalid);
            }
            (hooks, Some(start..start + raw.get().len()))
        } else {
            (json!({}), None)
        };
        // Validate event groups we may modify; arbitrary unknown event values stay intact.
        for event in EVENTS {
            if let Some(groups) = hooks.get(event) {
                let groups = groups.as_array().ok_or(AppError::HookConfigInvalid)?;
                for group in groups {
                    if !group.is_object() || !group.get("hooks").is_some_and(Value::is_array) {
                        return Err(AppError::HookConfigInvalid);
                    }
                }
            }
        }
        Ok(Self { text, hooks, range })
    }
    fn replace(&self, hooks: &Value) -> Result<Vec<u8>, AppError> {
        let value = serde_json::to_string_pretty(hooks).map_err(|_| AppError::HookConfigInvalid)?;
        let mut text = self.text.to_owned();
        if let Some(range) = self.range.clone() {
            text.replace_range(range, &value);
        } else {
            let end = text.rfind('}').ok_or(AppError::HookConfigInvalid)?;
            let separator = if text[..end].trim_end().ends_with('{') {
                ""
            } else {
                ","
            };
            text.insert_str(end, &format!("{separator}\n\"hooks\": {value}\n"));
        }
        Ok(text.into_bytes())
    }
}
impl Installer {
    /// All paths are explicit. Library AppState constructors leave this disabled.
    pub fn new(settings: PathBuf, binary: Option<PathBuf>, socket: PathBuf) -> Self {
        Self {
            settings,
            binary,
            socket,
            pending: None,
        }
    }
    fn command(&self) -> Result<String, AppError> {
        let binary = self.binary.as_ref().ok_or(AppError::HookBinaryMissing)?;
        if !binary.is_absolute() || !self.socket.is_absolute() {
            return Err(AppError::HookBinaryMissing);
        }
        Ok(format!(
            "{} claude-code --socket {}",
            quote(binary)?,
            quote(&self.socket)?
        ))
    }
    pub(crate) fn binary_available(&self) -> bool {
        self.binary.as_ref().is_some_and(|path| {
            path.is_absolute()
                && path.to_str().is_some()
                && fs::metadata(path).is_ok_and(|m| m.is_file())
                && rustix::fs::accessat(
                    rustix::fs::CWD,
                    path,
                    rustix::fs::Access::EXEC_OK,
                    rustix::fs::AtFlags::EACCESS,
                )
                .is_ok()
        })
    }
    pub fn view(&self) -> Result<HookView, AppError> {
        let before = self.read()?;
        let installed = match (before.as_deref(), self.command()) {
            (Some(bytes), Ok(command)) => installed(&Document::parse(bytes)?.hooks, &command),
            (Some(bytes), Err(_)) => {
                Document::parse(bytes)?;
                false
            }
            _ => false,
        };
        Ok(HookView {
            installed,
            binary_available: self.binary_available(),
        })
    }
    pub fn plan(&mut self, remove: bool) -> Result<HookPlan, AppError> {
        let command = self.command()?;
        if !remove && !self.binary_available() {
            return Err(AppError::HookBinaryMissing);
        }
        let before = self.read()?;
        let document = Document::parse(before.as_deref().unwrap_or(b"{}\n"))?;
        let mut after_hooks = document.hooks.clone();
        for event in EVENTS {
            if remove {
                if let Some(groups) = after_hooks.get_mut(event).and_then(Value::as_array_mut) {
                    for group in groups.iter_mut() {
                        if let Some(handlers) = group.get_mut("hooks").and_then(Value::as_array_mut)
                        {
                            handlers.retain(|leaf| !owned(leaf, &command));
                        }
                    }
                    groups.retain(|group| {
                        !(group.as_object().is_some_and(|obj| obj.len() == 1)
                            && group["hooks"].as_array().is_some_and(Vec::is_empty))
                    });
                    if groups.is_empty() {
                        after_hooks
                            .as_object_mut()
                            .ok_or(AppError::HookConfigInvalid)?
                            .remove(event);
                    }
                }
            } else if !event_installed(&after_hooks, event, &command) {
                let groups = after_hooks
                    .as_object_mut()
                    .ok_or(AppError::HookConfigInvalid)?
                    .entry(event)
                    .or_insert_with(|| json!([]))
                    .as_array_mut()
                    .ok_or(AppError::HookConfigInvalid)?;
                groups.push(json!({"hooks":[{"type":"command", "command":command, "async":true}]}));
            }
        }
        let changed = after_hooks != document.hooks;
        let after = if changed {
            document.replace(&after_hooks)?
        } else {
            before.clone().unwrap_or_default()
        };
        if after.len() as u64 > MAX_SETTINGS {
            return Err(AppError::HookConfigInvalid);
        }
        let mut id = [0; 16];
        getrandom::fill(&mut id).map_err(|_| AppError::RandomUnavailable)?;
        let view = HookPlan {
            plan_id: id.iter().map(|b| format!("{b:02x}")).collect(),
            settings_path_display: "~/.claude/settings.json".into(),
            base_sha256: hash(before.as_deref().unwrap_or_default()),
            diff: diff(&document.hooks, &after_hooks)?,
            action: if !changed {
                HookAction::NoChange
            } else if remove {
                HookAction::Remove
            } else {
                HookAction::Install
            },
        };
        self.pending = Some(Pending {
            view: view.clone(),
            before,
            after,
            applied: false,
        });
        Ok(view)
    }
    pub fn requires_binding(&self) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|p| p.view.action == HookAction::Install)
    }
    pub fn apply(&mut self, plan_id: &str) -> Result<(), AppError> {
        let plan = self
            .pending
            .as_ref()
            .filter(|plan| plan.view.plan_id == plan_id)
            .ok_or(AppError::HookPlanStale)?;
        let current = self.read()?;
        if plan.applied && current.as_deref() == Some(plan.after.as_slice()) {
            return Ok(());
        }
        if current != plan.before {
            return Err(AppError::HookPlanStale);
        }
        if plan.view.action == HookAction::NoChange {
            return Ok(());
        }
        if plan.view.action == HookAction::Install && !self.binary_available() {
            return Err(AppError::HookBinaryMissing);
        }
        let parent = self.settings.parent().ok_or(AppError::Configuration)?;
        let parent = settings_directory(parent).map_err(|_| AppError::UnsafePermissions)?;
        let mut temp = tempfile::NamedTempFile::new_in(&parent).map_err(|_| AppError::Storage)?;
        temp.as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|_| AppError::Storage)?;
        temp.write_all(&plan.after)
            .and_then(|_| temp.as_file().sync_all())
            .map_err(|_| AppError::Storage)?;
        // Reject a second external edit before producing any backup or replacing the file.
        if self.read()? != current {
            return Err(AppError::HookPlanStale);
        }
        if let Some(bytes) = current {
            let now = crate::state::unix_ms()?;
            let mut backup_file = None;
            for offset in 0..16 {
                let backup = parent.join(format!(
                    "settings.json.meshyard-backup-{}",
                    now.saturating_add(offset)
                ));
                match OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .custom_flags(libc::O_NOFOLLOW)
                    .open(backup)
                {
                    Ok(file) => {
                        backup_file = Some(file);
                        break;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(_) => return Err(AppError::Storage),
                }
            }
            let mut file = backup_file.ok_or(AppError::Storage)?;
            file.write_all(&bytes)
                .and_then(|_| file.sync_all())
                .map_err(|_| AppError::Storage)?;
        }
        temp.persist(&self.settings)
            .map_err(|_| AppError::Storage)?;
        fs::File::open(parent)
            .and_then(|file| file.sync_all())
            .map_err(|_| AppError::Storage)?;
        if let Some(plan) = &mut self.pending {
            plan.applied = true;
        }
        Ok(())
    }
    fn read(&self) -> Result<Option<Vec<u8>>, AppError> {
        if !self.settings.is_absolute() {
            return Err(AppError::Configuration);
        }
        let parent = self.settings.parent().ok_or(AppError::Configuration)?;
        match fs::symlink_metadata(parent) {
            Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {}
            Ok(_) => return Err(AppError::UnsafePermissions),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(AppError::Storage),
        }
        let file = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&self.settings)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(AppError::UnsafePermissions),
        };
        let meta = file.metadata().map_err(|_| AppError::Storage)?;
        if !meta.is_file()
            || meta.uid() != rustix::process::geteuid().as_raw()
            || meta.nlink() != 1
            || meta.mode() & 0o022 != 0
        {
            return Err(AppError::UnsafePermissions);
        }
        let mut bytes = Vec::new();
        file.take(MAX_SETTINGS + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| AppError::Storage)?;
        if bytes.len() as u64 > MAX_SETTINGS {
            return Err(AppError::HookConfigInvalid);
        }
        Ok(Some(bytes))
    }
}
fn quote(path: &Path) -> Result<String, AppError> {
    let text = path.to_str().ok_or(AppError::HookBinaryMissing)?;
    Ok(format!("'{}'", text.replace('\'', "'\"'\"'")))
}
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn owned(leaf: &Value, command: &str) -> bool {
    leaf.get("type").and_then(Value::as_str) == Some("command")
        && leaf.get("command").and_then(Value::as_str) == Some(command)
}
fn event_installed(hooks: &Value, event: &str, command: &str) -> bool {
    hooks
        .get(event)
        .and_then(Value::as_array)
        .is_some_and(|groups| {
            groups.iter().any(|group| {
                group.get("matcher").is_none()
                    && group
                        .get("hooks")
                        .and_then(Value::as_array)
                        .is_some_and(|handlers| {
                            handlers.iter().any(|leaf| {
                                owned(leaf, command)
                                    && leaf.get("async").and_then(Value::as_bool) == Some(true)
                            })
                        })
            })
        })
}
fn installed(hooks: &Value, command: &str) -> bool {
    EVENTS
        .iter()
        .all(|event| event_installed(hooks, event, command))
}
fn diff(before: &Value, after: &Value) -> Result<String, AppError> {
    if before == after {
        return Ok(format!(
            "No changes to hooks.\n{}\n",
            serde_json::to_string_pretty(&json!({"hooks": after}))
                .map_err(|_| AppError::HookConfigInvalid)?
        ));
    }
    let before = serde_json::to_string_pretty(&json!({"hooks":before}))
        .map_err(|_| AppError::HookConfigInvalid)?;
    let after = serde_json::to_string_pretty(&json!({"hooks":after}))
        .map_err(|_| AppError::HookConfigInvalid)?;
    let mut out = format!(
        "--- ~/.claude/settings.json (hooks)\n+++ ~/.claude/settings.json (hooks)\n@@ -1,{} +1,{} @@\n",
        before.lines().count(),
        after.lines().count()
    );
    for (prefix, text) in [("-", before), ("+", after)] {
        for line in text.lines() {
            out.push_str(prefix);
            out.push_str(line);
            out.push('\n');
        }
    }
    Ok(out)
}
#[cfg(test)]
#[path = "hooks_tests.rs"]
mod tests;
