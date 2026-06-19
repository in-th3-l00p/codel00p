use super::paths::default_memory_db;
use super::*;
use std::{
    env, fs,
    path::Path,
    sync::{Mutex, MutexGuard},
};

// Path resolution reads process env; serialize tests that touch it.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn lock_env() -> MutexGuard<'static, ()> {
    ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn with_home<T>(dir: &Path, test: impl FnOnce() -> T) -> T {
    let _guard = lock_env();
    let previous = env::var_os("CODEL00P_HOME");
    unsafe { env::set_var("CODEL00P_HOME", dir) };
    let result = test();
    unsafe {
        match previous {
            Some(value) => env::set_var("CODEL00P_HOME", value),
            None => env::remove_var("CODEL00P_HOME"),
        }
    }
    result
}

#[test]
fn home_honors_codel00p_home_override() {
    let dir = tempfile::tempdir().expect("tempdir");
    with_home(dir.path(), || {
        assert_eq!(home_dir(), dir.path());
        assert_eq!(user_config_path(), dir.path().join("config.toml"));
        assert_eq!(default_memory_db(), dir.path().join("memory.sqlite"));
    });
}

#[test]
fn layers_merge_with_project_over_user() {
    let dir = tempfile::tempdir().expect("tempdir");
    let workspace = dir.path().join("workspace");
    fs::create_dir_all(workspace.join(".codel00p")).expect("workspace dir");

    with_home(dir.path(), || {
        fs::write(
            user_config_path(),
            "[workspace]\norganization_id = \"user-org\"\nproject_id = \"user-proj\"\n[agent]\nprovider = \"openai\"\n",
        )
        .expect("write user config");
        fs::write(
            workspace.join(".codel00p/config.toml"),
            "[workspace]\nproject_id = \"proj-from-project\"\n[agent]\nmodel = \"gpt-4o\"\n",
        )
        .expect("write project config");

        let resolved = load_layered(&workspace).expect("load layered");
        assert_eq!(resolved.organization_id(), "user-org");
        assert_eq!(resolved.project_id(), "proj-from-project");
        assert_eq!(resolved.agent().provider.as_deref(), Some("openai"));
        assert_eq!(resolved.agent().model.as_deref(), Some("gpt-4o"));
    });
}

#[test]
fn defaults_apply_without_any_config() {
    let dir = tempfile::tempdir().expect("tempdir");
    with_home(dir.path(), || {
        let resolved = load_layered(dir.path()).expect("load layered");
        assert_eq!(resolved.organization_id(), "default");
        assert_eq!(resolved.project_id(), "default");
        assert_eq!(resolved.memory_db(), dir.path().join("memory.sqlite"));
        assert!(resolved.agent().provider.is_none());
    });
}

#[test]
fn set_get_and_unset_round_trip_with_coercion() {
    let dir = tempfile::tempdir().expect("tempdir");
    with_home(dir.path(), || {
        let path = user_config_path();
        set_value(&path, "agent.provider", "openrouter").expect("set provider");
        set_value(&path, "agent.max_iterations", "12").expect("set iterations");
        set_value(&path, "agent.stream", "true").expect("set stream");
        set_value(&path, "agent.tool_sets", "read, edit").expect("set tool sets");

        let resolved = load_layered(dir.path()).expect("reload");
        assert_eq!(resolved.agent().provider.as_deref(), Some("openrouter"));
        assert_eq!(resolved.agent().max_iterations, Some(12));
        assert_eq!(resolved.agent().stream, Some(true));
        assert_eq!(
            resolved.agent().tool_sets.as_deref(),
            Some(["read".to_string(), "edit".to_string()].as_slice())
        );
        assert_eq!(
            effective_value(&resolved.merged, "agent.tool_sets").unwrap(),
            Some("read,edit".to_string())
        );

        assert!(unset_value(&path, "agent.stream").expect("unset"));
        let resolved = load_layered(dir.path()).expect("reload after unset");
        assert!(resolved.agent().stream.is_none());
    });
}

#[test]
fn execution_backend_key_round_trips() {
    let dir = tempfile::tempdir().expect("tempdir");
    with_home(dir.path(), || {
        let path = user_config_path();
        set_value(&path, "agent.execution_backend", "local").expect("set backend");
        let resolved = load_layered(dir.path()).expect("reload");
        assert_eq!(resolved.agent().execution_backend.as_deref(), Some("local"));
        assert_eq!(
            effective_value(&resolved.merged, "agent.execution_backend").unwrap(),
            Some("local".to_string())
        );
    });
}

#[test]
fn docker_nested_keys_round_trip() {
    let dir = tempfile::tempdir().expect("tempdir");
    with_home(dir.path(), || {
        let path = user_config_path();
        set_value(&path, "agent.execution_backend", "docker").expect("set backend");
        set_value(&path, "agent.docker.image", "rust:1").expect("set image");
        set_value(&path, "agent.docker.memory", "512m").expect("set memory");
        set_value(&path, "agent.docker.map_host_user", "false").expect("set map_host_user");
        set_value(&path, "agent.docker.reuse_container", "false").expect("set reuse_container");

        let resolved = load_layered(dir.path()).expect("reload");
        assert_eq!(
            resolved.agent().execution_backend.as_deref(),
            Some("docker")
        );
        assert_eq!(resolved.agent().docker.image.as_deref(), Some("rust:1"));
        assert_eq!(resolved.agent().docker.memory.as_deref(), Some("512m"));
        assert_eq!(resolved.agent().docker.map_host_user, Some(false));
        assert_eq!(resolved.agent().docker.reuse_container, Some(false));
        assert_eq!(
            effective_value(&resolved.merged, "agent.docker.image").unwrap(),
            Some("rust:1".to_string())
        );
        assert_eq!(
            effective_value(&resolved.merged, "agent.docker.reuse_container").unwrap(),
            Some("false".to_string())
        );

        // Unsetting the last nested key prunes the [agent.docker] table.
        assert!(unset_value(&path, "agent.docker.image").expect("unset image"));
        assert!(unset_value(&path, "agent.docker.memory").expect("unset memory"));
        assert!(unset_value(&path, "agent.docker.map_host_user").expect("unset user"));
        assert!(unset_value(&path, "agent.docker.reuse_container").expect("unset reuse"));
        let resolved = load_layered(dir.path()).expect("reload after unset");
        assert!(resolved.agent().docker.image.is_none());
        // The agent table still has execution_backend, so it survives.
        assert_eq!(
            resolved.agent().execution_backend.as_deref(),
            Some("docker")
        );
    });
}

#[test]
fn require_isolation_for_unattended_round_trips_as_bool() {
    let dir = tempfile::tempdir().expect("tempdir");
    with_home(dir.path(), || {
        let path = user_config_path();
        set_value(&path, "agent.require_isolation_for_unattended", "true")
            .expect("set isolation policy");
        let resolved = load_layered(dir.path()).expect("reload");
        assert_eq!(
            resolved.agent().require_isolation_for_unattended,
            Some(true)
        );
        assert_eq!(
            effective_value(&resolved.merged, "agent.require_isolation_for_unattended").unwrap(),
            Some("true".to_string())
        );
        assert!(
            unset_value(&path, "agent.require_isolation_for_unattended").expect("unset policy")
        );
        let resolved = load_layered(dir.path()).expect("reload after unset");
        assert!(resolved.agent().require_isolation_for_unattended.is_none());
    });
}

#[test]
fn set_rejects_unknown_key() {
    let dir = tempfile::tempdir().expect("tempdir");
    with_home(dir.path(), || {
        let error = set_value(&user_config_path(), "agent.nope", "x").unwrap_err();
        assert!(error.contains("unknown config key"));
    });
}

#[test]
fn atomic_write_leaves_a_backup() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.toml");
    write_file_atomic(&path, "config_version = 1\n").expect("first write");
    write_file_atomic(&path, "config_version = 1\n[agent]\n").expect("second write");

    let backups = fs::read_dir(dir.path())
        .expect("read dir")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .contains("config.toml.bak.")
        })
        .count();
    assert!(backups >= 1, "expected a .bak file");
}

#[test]
fn env_file_seeds_only_missing_vars() {
    let dir = tempfile::tempdir().expect("tempdir");
    with_home(dir.path(), || {
        fs::write(
            env_file_path(),
            "CODEL00P_TEST_SEEDED=from-file\n# comment\nCODEL00P_TEST_PRESENT=from-file\n",
        )
        .expect("write env file");
        unsafe { env::set_var("CODEL00P_TEST_PRESENT", "from-env") };

        load_env_file();
        assert_eq!(env::var("CODEL00P_TEST_SEEDED").as_deref(), Ok("from-file"));
        assert_eq!(env::var("CODEL00P_TEST_PRESENT").as_deref(), Ok("from-env"));

        unsafe {
            env::remove_var("CODEL00P_TEST_SEEDED");
            env::remove_var("CODEL00P_TEST_PRESENT");
        }
    });
}
