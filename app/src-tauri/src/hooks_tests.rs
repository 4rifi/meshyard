#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
fn fixture() -> (tempfile::TempDir, Installer) {
    let root = PathBuf::from(std::env::var_os("TMPDIR").expect("USB TMPDIR"));
    assert!(root.starts_with("/Volumes/TokensAbuse/"));
    let dir = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(root)
        .unwrap();
    let binary = dir.path().join("hook's binary");
    fs::write(&binary, b"test executable placeholder; never executed").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let installer = Installer::new(
        dir.path().join(".claude/settings.json"),
        Some(binary),
        dir.path().join("socket's path"),
    );
    (dir, installer)
}
fn write(installer: &Installer, bytes: &[u8]) {
    fs::create_dir_all(installer.settings.parent().unwrap()).unwrap();
    fs::set_permissions(
        installer.settings.parent().unwrap(),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    fs::write(&installer.settings, bytes).unwrap();
    fs::set_permissions(&installer.settings, fs::Permissions::from_mode(0o600)).unwrap();
}
#[test]
fn exact_review_plan_apply_backup_idempotence_and_reverse_preserve_unrelated_bytes() {
    let (_dir, mut installer) = fixture();
    let before = b"{\n  \"z\" : [1, 2], \"hooks\": {\"PreToolUse\":[{\"matcher\":\"Bash\",\"custom\":42,\"hooks\":[{\"type\":\"command\",\"command\":\"unrelated\",\"async\":false}]}]},\n  \"a\": { \"keep\": true }\n}\n";
    write(&installer, before);
    let plan = installer.plan(false).unwrap();
    assert_eq!(plan.action, HookAction::Install);
    assert_eq!(plan.base_sha256, hash(before));
    assert_eq!(
        plan.diff,
        diff(
            &Document::parse(before).unwrap().hooks,
            &Document::parse(&installer.pending.as_ref().unwrap().after)
                .unwrap()
                .hooks,
            &installer.display_path()
        )
        .unwrap()
    );
    assert!(
        plan.diff
            .contains(&serde_json::to_string(&installer.command().unwrap()).unwrap())
    );
    assert_eq!(fs::read(&installer.settings).unwrap(), before);
    assert_eq!(
        fs::read_dir(installer.settings.parent().unwrap())
            .unwrap()
            .count(),
        1
    );
    installer.apply(&plan.plan_id).unwrap();
    let after = fs::read(&installer.settings).unwrap();
    assert!(after.starts_with(b"{\n  \"z\" : [1, 2], \"hooks\": "));
    assert!(after.ends_with(b",\n  \"a\": { \"keep\": true }\n}\n"));
    let doc = Document::parse(&after).unwrap();
    assert!(installer.view().unwrap().installed);
    for event in EVENTS {
        let leaf = &doc.hooks[event].as_array().unwrap().last().unwrap()["hooks"][0];
        assert_eq!(
            leaf,
            &json!({"type":"command", "command":installer.command().unwrap(), "async":true})
        );
        assert!(
            doc.hooks[event]
                .as_array()
                .unwrap()
                .last()
                .unwrap()
                .get("matcher")
                .is_none()
        );
    }
    let backup = fs::read_dir(installer.settings.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("settings.json.meshyard-backup-")
        })
        .unwrap();
    assert_eq!(fs::read(&backup).unwrap(), before);
    assert_eq!(fs::metadata(backup).unwrap().mode() & 0o777, 0o600);
    assert_eq!(
        fs::metadata(&installer.settings).unwrap().mode() & 0o777,
        0o600
    );
    installer.apply(&plan.plan_id).unwrap();
    assert_eq!(
        fs::read_dir(installer.settings.parent().unwrap())
            .unwrap()
            .count(),
        2
    );
    let again = installer.plan(false).unwrap();
    assert_eq!(again.action, HookAction::NoChange);
    installer.apply(&again.plan_id).unwrap();
    let remove = installer.plan(true).unwrap();
    assert_eq!(remove.action, HookAction::Remove);
    assert_eq!(
        fs::read(&installer.settings).unwrap(),
        after,
        "remove is review only"
    );
    installer.apply(&remove.plan_id).unwrap();
    let removed = fs::read(&installer.settings).unwrap();
    assert_eq!(
        Document::parse(&removed).unwrap().hooks,
        Document::parse(before).unwrap().hooks
    );
    assert!(removed.starts_with(b"{\n  \"z\" : [1, 2], \"hooks\": "));
    assert!(!installer.view().unwrap().installed);
    assert_eq!(installer.plan(true).unwrap().action, HookAction::NoChange);
}
#[test]
fn absent_cancel_invalid_external_edit_missing_binary_and_exact_command_ownership() {
    let (_dir, mut installer) = fixture();
    let plan = installer.plan(false).unwrap();
    assert!(
        !installer.settings.parent().unwrap().exists(),
        "Cancel/planning creates nothing"
    );
    write(&installer, b"{\"external\":true}");
    assert_eq!(installer.apply(&plan.plan_id), Err(AppError::HookPlanStale));
    assert_eq!(
        fs::read(&installer.settings).unwrap(),
        b"{\"external\":true}"
    );
    for invalid in [
        b"invalid".as_slice(),
        b"[]",
        b"{\"hooks\":null}",
        b"{\"hooks\":{},\"hooks\":{}}",
        b"{\"hooks\":{\"Stop\":{}}}",
    ] {
        write(&installer, invalid);
        assert_eq!(installer.plan(false), Err(AppError::HookConfigInvalid));
        assert_eq!(fs::read(&installer.settings).unwrap(), invalid);
    }
    let prefix_command = format!("{} --other", installer.command().unwrap());
    write(
        &installer,
        &serde_json::to_vec(
            &json!({"hooks":{"Stop":[{"hooks":[{"type":"command","command":prefix_command}]}]}}),
        )
        .unwrap(),
    );
    assert_eq!(installer.plan(true).unwrap().action, HookAction::NoChange);
    let plan = installer.plan(false).unwrap();
    fs::set_permissions(
        installer.binary.as_ref().unwrap(),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    assert!(!installer.view().unwrap().binary_available);
    assert_eq!(
        installer.apply(&plan.plan_id),
        Err(AppError::HookBinaryMissing)
    );
    fs::remove_file(installer.binary.as_ref().unwrap()).unwrap();
    assert_eq!(
        installer.apply(&plan.plan_id),
        Err(AppError::HookBinaryMissing)
    );
    assert!(!installer.view().unwrap().binary_available);
    assert_eq!(installer.plan(false), Err(AppError::HookBinaryMissing));
}
#[test]
fn shell_quote_each_argument_and_reject_symlink_settings() {
    let (_dir, mut installer) = fixture();
    let command = installer.command().unwrap();
    assert_eq!(
        quote(Path::new("/USB/hook's binary")).unwrap(),
        "'/USB/hook'\"'\"'s binary'"
    );
    assert_eq!(
        command,
        format!(
            "'{}' claude-code --socket '{}'",
            installer
                .binary
                .as_ref()
                .unwrap()
                .to_str()
                .unwrap()
                .replace('\'', "'\"'\"'"),
            installer.socket.to_str().unwrap().replace('\'', "'\"'\"'")
        )
    );
    let target = installer.settings.with_extension("target");
    fs::create_dir(installer.settings.parent().unwrap()).unwrap();
    fs::write(&target, b"{}").unwrap();
    std::os::unix::fs::symlink(&target, &installer.settings).unwrap();
    assert_eq!(installer.plan(false), Err(AppError::UnsafePermissions));
    assert_eq!(fs::read(target).unwrap(), b"{}");
}

#[test]
fn readable_settings_parent_installs_with_private_backup_without_chmod() {
    for mode in [0o755, 0o750] {
        let (_dir, mut installer) = fixture();
        let before = b"{\"keep\":true}\n";
        write(&installer, before);
        let parent = installer.settings.parent().unwrap().to_owned();
        fs::set_permissions(&parent, fs::Permissions::from_mode(mode)).unwrap();
        let plan = installer.plan(false).unwrap();
        installer.apply(&plan.plan_id).unwrap();
        assert!(installer.view().unwrap().installed);
        assert_eq!(fs::metadata(&parent).unwrap().mode() & 0o777, mode);
        assert_eq!(
            fs::metadata(&installer.settings).unwrap().mode() & 0o777,
            0o600
        );
        let backups: Vec<_> = fs::read_dir(parent)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path != &installer.settings)
            .collect();
        assert_eq!(backups.len(), 1, "only the backup remains beside settings");
        assert!(
            backups[0]
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("settings.json.meshyard-backup-")
        );
        assert_eq!(fs::read(&backups[0]).unwrap(), before);
        assert_eq!(fs::metadata(&backups[0]).unwrap().mode() & 0o777, 0o600);
    }
}

#[test]
fn writable_settings_parent_is_rejected_without_changes() {
    for mode in [0o775, 0o777] {
        let (_dir, mut installer) = fixture();
        write(&installer, b"{}");
        let parent = installer.settings.parent().unwrap().to_owned();
        fs::set_permissions(&parent, fs::Permissions::from_mode(mode)).unwrap();
        let plan = installer.plan(false).unwrap();
        assert_eq!(
            installer.apply(&plan.plan_id),
            Err(AppError::UnsafePermissions)
        );
        assert_eq!(fs::read(&installer.settings).unwrap(), b"{}");
        assert_eq!(fs::metadata(&parent).unwrap().mode() & 0o777, mode);
        assert_eq!(fs::read_dir(parent).unwrap().count(), 1);
    }
}

#[test]
fn symlink_settings_parent_is_rejected_even_after_planning() {
    let (dir, mut installer) = fixture();
    write(&installer, b"{}");
    let plan = installer.plan(false).unwrap();
    let parent = installer.settings.parent().unwrap().to_owned();
    let target = dir.path().join("original-settings-parent");
    fs::rename(&parent, &target).unwrap();
    std::os::unix::fs::symlink(&target, &parent).unwrap();
    assert_eq!(installer.plan(false), Err(AppError::UnsafePermissions));
    assert_eq!(
        installer.apply(&plan.plan_id),
        Err(AppError::UnsafePermissions)
    );
    assert_eq!(fs::read(target.join("settings.json")).unwrap(), b"{}");
    assert_eq!(fs::read_dir(target).unwrap().count(), 1);
}

#[test]
fn missing_settings_parent_is_created_private_only_on_apply() {
    let (_dir, mut installer) = fixture();
    let plan = installer.plan(false).unwrap();
    let parent = installer.settings.parent().unwrap().to_owned();
    assert!(!parent.exists());
    installer.apply(&plan.plan_id).unwrap();
    assert!(installer.view().unwrap().installed);
    assert_eq!(fs::metadata(&parent).unwrap().mode() & 0o777, 0o700);
    assert_eq!(
        fs::metadata(&installer.settings).unwrap().mode() & 0o777,
        0o600
    );
    assert_eq!(fs::read_dir(parent).unwrap().count(), 1);
}

#[test]
fn codex_plan_is_explicit_preserves_backup_and_rejects_stale_changes() {
    let (_dir, mut installer) = fixture();
    installer.kind = AgentKind::Codex;
    installer.settings = installer.settings.with_file_name("hooks.json");
    let before = b"{\"description\": \"keep exact bytes\", \"hooks\":{}}\n";
    write(&installer, before);
    let plan = installer.plan(false).unwrap();
    assert_eq!(
        plan.settings_path_display,
        installer.settings.to_string_lossy()
    );
    assert!(plan.diff.contains(" codex --socket "));
    assert!(!plan.diff.contains("StopFailure"));
    assert_eq!(fs::read(&installer.settings).unwrap(), before);
    installer.apply(&plan.plan_id).unwrap();
    assert!(installer.view().unwrap().installed);
    let after = fs::read(&installer.settings).unwrap();
    let document = Document::parse(&after).unwrap();
    assert_eq!(document.hooks.as_object().unwrap().len(), 6);
    assert!(after.starts_with(b"{\"description\": \"keep exact bytes\", "));
    let backup = fs::read_dir(installer.settings.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("hooks.json.meshyard-backup-")
        })
        .unwrap();
    assert_eq!(fs::read(&backup).unwrap(), before);
    assert_eq!(installer.plan(false).unwrap().action, HookAction::NoChange);
    let removal = installer.plan(true).unwrap();
    let edited = [after.as_slice(), b" "].concat();
    fs::write(&installer.settings, &edited).unwrap();
    assert_eq!(
        installer.apply(&removal.plan_id),
        Err(AppError::HookPlanStale)
    );
    assert_eq!(fs::read(&installer.settings).unwrap(), edited);
    assert_eq!(fs::read(&backup).unwrap(), before);
    let removal = installer.plan(true).unwrap();
    installer.apply(&removal.plan_id).unwrap();
    assert!(!installer.view().unwrap().installed);
    assert_eq!(
        Document::parse(&fs::read(&installer.settings).unwrap())
            .unwrap()
            .hooks,
        json!({})
    );
}

#[test]
fn app_translocation_dmg_and_temporary_bundle_paths_cannot_be_installed() {
    let (_dir, mut installer) = fixture();
    for binary in [
        "/private/var/folders/AppTranslocation/uuid/d/Meshyard.app/Contents/MacOS/meshyard-hook",
        "/Volumes/Meshyard/Meshyard.app/Contents/MacOS/meshyard-hook",
        "/temporary/Meshyard.app/Contents/MacOS/meshyard-hook",
    ] {
        installer.binary = Some(PathBuf::from(binary));
        assert_eq!(installer.plan(false), Err(AppError::HookInstallLocation));
        assert!(!installer.settings.exists());
    }
    installer.binary = Some(PathBuf::from(
        "/Applications/Meshyard.app/Contents/MacOS/meshyard-hook",
    ));
    assert!(
        installer
            .command()
            .unwrap()
            .starts_with("'/Applications/Meshyard.app/")
    );
}
