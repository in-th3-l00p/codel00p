use super::paths::default_memory_db;
use super::test_env::with_home;
use super::*;
use std::{env, fs};

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
fn behavior_self_awareness_toggles_round_trip_and_default_on() {
    let dir = tempfile::tempdir().expect("tempdir");
    with_home(dir.path(), || {
        // Default (no config): both toggles unset, which the helpers treat as on.
        let resolved = load_layered(dir.path()).expect("load layered");
        assert!(resolved.agent().behavior.self_knowledge.is_none());
        assert!(resolved.agent().behavior.self_state.is_none());
        assert!(resolved.agent().behavior.self_knowledge_enabled());
        assert!(resolved.agent().behavior.self_state_enabled());

        // Set both: round-trip the literal values and the effective view.
        let path = user_config_path();
        set_value(&path, "agent.behavior.self_knowledge", "false").expect("set self_knowledge");
        set_value(&path, "agent.behavior.self_state", "true").expect("set self_state");
        let resolved = load_layered(dir.path()).expect("reload");
        assert_eq!(resolved.agent().behavior.self_knowledge, Some(false));
        assert_eq!(resolved.agent().behavior.self_state, Some(true));
        assert!(!resolved.agent().behavior.self_knowledge_enabled());
        assert!(resolved.agent().behavior.self_state_enabled());
        assert_eq!(
            effective_value(&resolved.merged, "agent.behavior.self_knowledge").unwrap(),
            Some("false".to_string())
        );
        assert_eq!(
            effective_value(&resolved.merged, "agent.behavior.self_state").unwrap(),
            Some("true".to_string())
        );

        // Unsetting the last nested key prunes the [agent.behavior] table.
        assert!(unset_value(&path, "agent.behavior.self_knowledge").expect("unset self_knowledge"));
        assert!(unset_value(&path, "agent.behavior.self_state").expect("unset self_state"));
        let resolved = load_layered(dir.path()).expect("reload after unset");
        assert!(resolved.agent().behavior.self_knowledge.is_none());
        assert!(resolved.agent().behavior.self_state.is_none());
        assert!(resolved.agent().behavior.self_knowledge_enabled());
    });
}

#[test]
fn behavior_base_prompt_and_auto_plan_round_trip_and_default_on() {
    let dir = tempfile::tempdir().expect("tempdir");
    with_home(dir.path(), || {
        // Default (no config): both toggles unset, which the helpers treat as on.
        let resolved = load_layered(dir.path()).expect("load layered");
        assert!(resolved.agent().behavior.base_prompt.is_none());
        assert!(resolved.agent().behavior.auto_plan.is_none());
        assert!(resolved.agent().behavior.base_prompt_enabled());
        assert!(resolved.agent().behavior.auto_plan_enabled());

        // Set both: round-trip the literal values and the effective view.
        let path = user_config_path();
        set_value(&path, "agent.behavior.base_prompt", "false").expect("set base_prompt");
        set_value(&path, "agent.behavior.auto_plan", "false").expect("set auto_plan");
        let resolved = load_layered(dir.path()).expect("reload");
        assert_eq!(resolved.agent().behavior.base_prompt, Some(false));
        assert_eq!(resolved.agent().behavior.auto_plan, Some(false));
        assert!(!resolved.agent().behavior.base_prompt_enabled());
        assert!(!resolved.agent().behavior.auto_plan_enabled());
        assert_eq!(
            effective_value(&resolved.merged, "agent.behavior.base_prompt").unwrap(),
            Some("false".to_string())
        );
        assert_eq!(
            effective_value(&resolved.merged, "agent.behavior.auto_plan").unwrap(),
            Some("false".to_string())
        );

        // Unsetting the nested keys prunes the [agent.behavior] table.
        assert!(unset_value(&path, "agent.behavior.base_prompt").expect("unset base_prompt"));
        assert!(unset_value(&path, "agent.behavior.auto_plan").expect("unset auto_plan"));
        let resolved = load_layered(dir.path()).expect("reload after unset");
        assert!(resolved.agent().behavior.base_prompt.is_none());
        assert!(resolved.agent().behavior.auto_plan.is_none());
        assert!(resolved.agent().behavior.base_prompt_enabled());
        assert!(resolved.agent().behavior.auto_plan_enabled());
    });
}

#[test]
fn behavior_workspace_context_round_trips_and_defaults_on() {
    let dir = tempfile::tempdir().expect("tempdir");
    with_home(dir.path(), || {
        // Default (no config): unset, which the helper treats as on.
        let resolved = load_layered(dir.path()).expect("load layered");
        assert!(resolved.agent().behavior.workspace_context.is_none());
        assert!(resolved.agent().behavior.workspace_context_enabled());

        // Set false: round-trips the literal value and effective view.
        let path = user_config_path();
        set_value(&path, "agent.behavior.workspace_context", "false")
            .expect("set workspace_context");
        let resolved = load_layered(dir.path()).expect("reload");
        assert_eq!(resolved.agent().behavior.workspace_context, Some(false));
        assert!(!resolved.agent().behavior.workspace_context_enabled());
        assert_eq!(
            effective_value(&resolved.merged, "agent.behavior.workspace_context").unwrap(),
            Some("false".to_string())
        );

        // Unsetting prunes back to the default (on).
        assert!(
            unset_value(&path, "agent.behavior.workspace_context")
                .expect("unset workspace_context")
        );
        let resolved = load_layered(dir.path()).expect("reload after unset");
        assert!(resolved.agent().behavior.workspace_context.is_none());
        assert!(resolved.agent().behavior.workspace_context_enabled());
    });
}

#[test]
fn behavior_proactive_memory_round_trips_and_defaults_on() {
    let dir = tempfile::tempdir().expect("tempdir");
    with_home(dir.path(), || {
        // Default (no config): unset, which the helper treats as on.
        let resolved = load_layered(dir.path()).expect("load layered");
        assert!(resolved.agent().behavior.proactive_memory.is_none());
        assert!(resolved.agent().behavior.proactive_memory_enabled());

        // Set false: round-trips the literal value and effective view.
        let path = user_config_path();
        set_value(&path, "agent.behavior.proactive_memory", "false").expect("set proactive_memory");
        let resolved = load_layered(dir.path()).expect("reload");
        assert_eq!(resolved.agent().behavior.proactive_memory, Some(false));
        assert!(!resolved.agent().behavior.proactive_memory_enabled());
        assert_eq!(
            effective_value(&resolved.merged, "agent.behavior.proactive_memory").unwrap(),
            Some("false".to_string())
        );

        // Unsetting prunes back to the default (on).
        assert!(
            unset_value(&path, "agent.behavior.proactive_memory").expect("unset proactive_memory")
        );
        let resolved = load_layered(dir.path()).expect("reload after unset");
        assert!(resolved.agent().behavior.proactive_memory.is_none());
        assert!(resolved.agent().behavior.proactive_memory_enabled());
    });
}

#[test]
fn behavior_persona_round_trips_and_defaults_on() {
    let dir = tempfile::tempdir().expect("tempdir");
    with_home(dir.path(), || {
        // Default (no config): unset, which the helper treats as on.
        let resolved = load_layered(dir.path()).expect("load layered");
        assert!(resolved.agent().behavior.persona.is_none());
        assert!(resolved.agent().behavior.persona_enabled());

        // Set false: round-trips the literal value and effective view.
        let path = user_config_path();
        set_value(&path, "agent.behavior.persona", "false").expect("set persona");
        let resolved = load_layered(dir.path()).expect("reload");
        assert_eq!(resolved.agent().behavior.persona, Some(false));
        assert!(!resolved.agent().behavior.persona_enabled());
        assert_eq!(
            effective_value(&resolved.merged, "agent.behavior.persona").unwrap(),
            Some("false".to_string())
        );

        // Unsetting prunes back to the default (on).
        assert!(unset_value(&path, "agent.behavior.persona").expect("unset persona"));
        let resolved = load_layered(dir.path()).expect("reload after unset");
        assert!(resolved.agent().behavior.persona.is_none());
        assert!(resolved.agent().behavior.persona_enabled());
    });
}

#[test]
fn behavior_curated_memory_round_trips_and_defaults_on() {
    let dir = tempfile::tempdir().expect("tempdir");
    with_home(dir.path(), || {
        // Default (no config): unset, which the helper treats as on.
        let resolved = load_layered(dir.path()).expect("load layered");
        assert!(resolved.agent().behavior.curated_memory.is_none());
        assert!(resolved.agent().behavior.curated_memory_enabled());

        // Set false: round-trips the literal value and effective view.
        let path = user_config_path();
        set_value(&path, "agent.behavior.curated_memory", "false").expect("set curated_memory");
        let resolved = load_layered(dir.path()).expect("reload");
        assert_eq!(resolved.agent().behavior.curated_memory, Some(false));
        assert!(!resolved.agent().behavior.curated_memory_enabled());
        assert_eq!(
            effective_value(&resolved.merged, "agent.behavior.curated_memory").unwrap(),
            Some("false".to_string())
        );

        // Unsetting prunes back to the default (on).
        assert!(unset_value(&path, "agent.behavior.curated_memory").expect("unset curated_memory"));
        let resolved = load_layered(dir.path()).expect("reload after unset");
        assert!(resolved.agent().behavior.curated_memory.is_none());
        assert!(resolved.agent().behavior.curated_memory_enabled());
    });
}

#[test]
fn behavior_verify_toggles_round_trip_and_defaults() {
    let dir = tempfile::tempdir().expect("tempdir");
    with_home(dir.path(), || {
        // Defaults: verify/test/critique default ON, lint_and_fix OFF,
        // verify_iterations 3, test_command unset.
        let resolved = load_layered(dir.path()).expect("load layered");
        let behavior = &resolved.agent().behavior;
        assert!(behavior.self_verify.is_none());
        assert!(behavior.auto_test.is_none());
        assert!(behavior.lint_and_fix.is_none());
        assert!(behavior.self_critique.is_none());
        assert!(behavior.verify_iterations.is_none());
        assert!(behavior.test_command.is_none());
        assert!(behavior.self_verify_enabled());
        assert!(behavior.auto_test_enabled());
        assert!(!behavior.lint_and_fix_enabled());
        assert!(behavior.self_critique_enabled());
        assert_eq!(behavior.verify_iterations_value(), 3);
        assert_eq!(behavior.test_command_value(), None);

        // Round-trip explicit values.
        let path = user_config_path();
        set_value(&path, "agent.behavior.self_verify", "false").expect("set self_verify");
        set_value(&path, "agent.behavior.auto_test", "false").expect("set auto_test");
        set_value(&path, "agent.behavior.lint_and_fix", "true").expect("set lint_and_fix");
        set_value(&path, "agent.behavior.self_critique", "false").expect("set self_critique");
        set_value(&path, "agent.behavior.verify_iterations", "5").expect("set verify_iterations");
        set_value(&path, "agent.behavior.test_command", "cargo test -p x")
            .expect("set test_command");
        let resolved = load_layered(dir.path()).expect("reload");
        let behavior = &resolved.agent().behavior;
        assert_eq!(behavior.self_verify, Some(false));
        assert_eq!(behavior.auto_test, Some(false));
        assert_eq!(behavior.lint_and_fix, Some(true));
        assert_eq!(behavior.self_critique, Some(false));
        assert_eq!(behavior.verify_iterations, Some(5));
        assert_eq!(behavior.test_command.as_deref(), Some("cargo test -p x"));
        assert!(!behavior.self_verify_enabled());
        assert!(!behavior.auto_test_enabled());
        assert!(behavior.lint_and_fix_enabled());
        assert!(!behavior.self_critique_enabled());
        assert_eq!(behavior.verify_iterations_value(), 5);
        assert_eq!(
            behavior.test_command_value().as_deref(),
            Some("cargo test -p x")
        );
        assert_eq!(
            effective_value(&resolved.merged, "agent.behavior.verify_iterations").unwrap(),
            Some("5".to_string())
        );
        assert_eq!(
            effective_value(&resolved.merged, "agent.behavior.test_command").unwrap(),
            Some("cargo test -p x".to_string())
        );

        // Unsetting prunes back to defaults.
        for key in [
            "agent.behavior.self_verify",
            "agent.behavior.auto_test",
            "agent.behavior.lint_and_fix",
            "agent.behavior.self_critique",
            "agent.behavior.verify_iterations",
            "agent.behavior.test_command",
        ] {
            unset_value(&path, key).expect("unset key");
        }
        let resolved = load_layered(dir.path()).expect("reload after unset");
        let behavior = &resolved.agent().behavior;
        assert!(behavior.self_verify.is_none());
        assert!(behavior.self_verify_enabled());
        assert_eq!(behavior.verify_iterations_value(), 3);
    });
}

#[test]
fn behavior_self_correction_toggles_round_trip_and_defaults() {
    let dir = tempfile::tempdir().expect("tempdir");
    with_home(dir.path(), || {
        // Defaults: error_hints + replan_on_failure ON, failure_budget 3.
        let resolved = load_layered(dir.path()).expect("load layered");
        let behavior = &resolved.agent().behavior;
        assert!(behavior.error_hints.is_none());
        assert!(behavior.replan_on_failure.is_none());
        assert!(behavior.failure_budget.is_none());
        assert!(behavior.error_hints_enabled());
        assert!(behavior.replan_on_failure_enabled());
        assert_eq!(behavior.failure_budget_value(), 3);

        // Round-trip explicit values.
        let path = user_config_path();
        set_value(&path, "agent.behavior.error_hints", "false").expect("set error_hints");
        set_value(&path, "agent.behavior.replan_on_failure", "false")
            .expect("set replan_on_failure");
        set_value(&path, "agent.behavior.failure_budget", "5").expect("set failure_budget");
        let resolved = load_layered(dir.path()).expect("reload");
        let behavior = &resolved.agent().behavior;
        assert_eq!(behavior.error_hints, Some(false));
        assert_eq!(behavior.replan_on_failure, Some(false));
        assert_eq!(behavior.failure_budget, Some(5));
        assert!(!behavior.error_hints_enabled());
        assert!(!behavior.replan_on_failure_enabled());
        assert_eq!(behavior.failure_budget_value(), 5);
        assert_eq!(
            effective_value(&resolved.merged, "agent.behavior.failure_budget").unwrap(),
            Some("5".to_string())
        );
        assert_eq!(
            effective_value(&resolved.merged, "agent.behavior.error_hints").unwrap(),
            Some("false".to_string())
        );

        // Unsetting prunes back to defaults.
        for key in [
            "agent.behavior.error_hints",
            "agent.behavior.replan_on_failure",
            "agent.behavior.failure_budget",
        ] {
            unset_value(&path, key).expect("unset key");
        }
        let resolved = load_layered(dir.path()).expect("reload after unset");
        let behavior = &resolved.agent().behavior;
        assert!(behavior.error_hints.is_none());
        assert!(behavior.error_hints_enabled());
        assert!(behavior.replan_on_failure_enabled());
        assert_eq!(behavior.failure_budget_value(), 3);
    });
}

#[test]
fn tui_show_advanced_round_trips_and_defaults_unset() {
    let dir = tempfile::tempdir().expect("tempdir");
    with_home(dir.path(), || {
        // Default (no config) leaves it unset, which the TUI treats as hidden.
        let resolved = load_layered(dir.path()).expect("load layered");
        assert!(resolved.merged.tui.show_advanced.is_none());
        assert_eq!(
            effective_value(&resolved.merged, "tui.show_advanced").unwrap(),
            None
        );

        let path = user_config_path();
        set_value(&path, "tui.show_advanced", "true").expect("set show_advanced");
        let resolved = load_layered(dir.path()).expect("reload");
        assert_eq!(resolved.merged.tui.show_advanced, Some(true));
        assert_eq!(
            effective_value(&resolved.merged, "tui.show_advanced").unwrap(),
            Some("true".to_string())
        );

        set_value(&path, "tui.show_advanced", "false").expect("toggle off");
        let resolved = load_layered(dir.path()).expect("reload after toggle");
        assert_eq!(resolved.merged.tui.show_advanced, Some(false));

        assert!(unset_value(&path, "tui.show_advanced").expect("unset"));
        let resolved = load_layered(dir.path()).expect("reload after unset");
        assert!(resolved.merged.tui.show_advanced.is_none());
    });
}

#[test]
fn tui_check_updates_round_trips_and_defaults_true() {
    let dir = tempfile::tempdir().expect("tempdir");
    with_home(dir.path(), || {
        // Default (no config) leaves it unset; the TUI treats unset as enabled.
        let resolved = load_layered(dir.path()).expect("load layered");
        assert!(resolved.merged.tui.check_updates.is_none());
        assert_eq!(
            effective_value(&resolved.merged, "tui.check_updates").unwrap(),
            None
        );

        let path = user_config_path();
        set_value(&path, "tui.check_updates", "false").expect("set check_updates");
        let resolved = load_layered(dir.path()).expect("reload");
        assert_eq!(resolved.merged.tui.check_updates, Some(false));
        assert_eq!(
            effective_value(&resolved.merged, "tui.check_updates").unwrap(),
            Some("false".to_string())
        );

        set_value(&path, "tui.check_updates", "true").expect("toggle on");
        let resolved = load_layered(dir.path()).expect("reload after toggle");
        assert_eq!(resolved.merged.tui.check_updates, Some(true));

        assert!(unset_value(&path, "tui.check_updates").expect("unset"));
        let resolved = load_layered(dir.path()).expect("reload after unset");
        assert!(resolved.merged.tui.check_updates.is_none());
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

// ---- [agent.profiles] custom-profiles layer (#12) ----

#[test]
fn profile_table_round_trips_through_toml() {
    use super::schema::Settings;
    let toml = "\
[agent.profiles.tdd]
description = \"Test-driven\"
tool_sets = [\"read\", \"edit\", \"command\"]
execution_backend = \"docker\"
permission_mode = \"ask\"
self_verify = true
auto_plan = false
verify_iterations = 4
";
    let settings: Settings = toml::from_str(toml).expect("deserialize profile");
    let profile = settings
        .agent
        .profiles
        .get("tdd")
        .expect("tdd profile present");
    assert_eq!(profile.description.as_deref(), Some("Test-driven"));
    assert_eq!(
        profile.tool_sets.as_deref(),
        Some(
            [
                "read".to_string(),
                "edit".to_string(),
                "command".to_string()
            ]
            .as_slice()
        )
    );
    assert_eq!(profile.execution_backend.as_deref(), Some("docker"));
    assert_eq!(profile.permission_mode.as_deref(), Some("ask"));
    assert_eq!(profile.self_verify, Some(true));
    assert_eq!(profile.auto_plan, Some(false));
    assert_eq!(profile.verify_iterations, Some(4));

    // Re-serialize and re-parse: the bundle survives a round trip as
    // `[agent.profiles.tdd]`.
    let serialized = toml::to_string(&settings).expect("serialize");
    assert!(
        serialized.contains("[agent.profiles.tdd]"),
        "expected an [agent.profiles.tdd] table, got:\n{serialized}"
    );
    let reparsed: Settings = toml::from_str(&serialized).expect("re-deserialize");
    assert_eq!(reparsed.agent.profiles, settings.agent.profiles);
}

#[test]
fn profile_merge_project_overrides_user_and_distinct_profiles_coexist() {
    use super::schema::{AgentSettings, Settings};
    let user: Settings = toml::from_str(
        "[agent.profiles.tdd]\npermission_mode = \"allow\"\nself_verify = true\n\
         [agent.profiles.review]\ntool_sets = [\"read\"]\n",
    )
    .expect("user");
    let project: Settings = toml::from_str(
        "[agent.profiles.tdd]\npermission_mode = \"ask\"\n\
         [agent.profiles.docs]\ntool_sets = [\"read\", \"edit\"]\n",
    )
    .expect("project");

    let mut merged = user;
    merged.merge(project);
    let agent: &AgentSettings = &merged.agent;

    // Project wins per field on the shared `tdd` profile; the user-only field
    // (self_verify) is preserved.
    let tdd = agent.profiles.get("tdd").expect("tdd");
    assert_eq!(tdd.permission_mode.as_deref(), Some("ask"));
    assert_eq!(tdd.self_verify, Some(true));
    // Distinct profiles from both layers coexist.
    assert!(agent.profiles.contains_key("review"));
    assert!(agent.profiles.contains_key("docs"));
}

#[test]
fn resolve_profile_prefers_user_over_preset_and_errors_on_unknown() {
    use super::schema::AgentSettings;
    let mut agent = AgentSettings::default();
    // Shadow the built-in `careful` preset with a user profile.
    agent.profiles.insert(
        "careful".to_string(),
        ProfileSettings {
            permission_mode: Some("allow".to_string()),
            ..ProfileSettings::default()
        },
    );

    // User profile shadows the preset.
    let careful = agent.resolve_profile("careful").expect("careful");
    assert_eq!(careful.permission_mode.as_deref(), Some("allow"));
    // A built-in still resolves when not shadowed.
    let autonomous = agent.resolve_profile("autonomous").expect("autonomous");
    assert_eq!(
        autonomous.tool_sets.as_deref(),
        Some(["all".to_string()].as_slice())
    );
    // Unknown name errors and lists the available profiles.
    let error = agent.resolve_profile("nope").expect_err("unknown errors");
    assert!(
        error.contains("nope"),
        "error names the bad profile: {error}"
    );
    assert!(error.contains("autonomous"), "error lists presets: {error}");
    assert!(error.contains("careful"));
    assert!(error.contains("manual"));
}

#[test]
fn apply_profile_fills_gaps_but_config_scalars_win() {
    use super::schema::AgentSettings;
    let mut agent = AgentSettings {
        // Config already pins the permission mode; the profile must not override.
        permission_mode: Some("deny".to_string()),
        ..AgentSettings::default()
    };
    let profile = ProfileSettings {
        permission_mode: Some("ask".to_string()),
        tool_sets: Some(vec!["read".to_string()]),
        self_verify: Some(false),
        ..ProfileSettings::default()
    };
    agent.apply_profile(&profile);
    // Config scalar wins over the profile.
    assert_eq!(agent.permission_mode.as_deref(), Some("deny"));
    // Profile fills fields config left unset.
    assert_eq!(
        agent.tool_sets.as_deref(),
        Some(["read".to_string()].as_slice())
    );
    assert_eq!(agent.behavior.self_verify, Some(false));
}

#[test]
fn agent_profile_default_and_nested_profile_key_round_trip() {
    let dir = tempfile::tempdir().expect("tempdir");
    with_home(dir.path(), || {
        let path = user_config_path();
        // The active-default selector.
        set_value(&path, "agent.profile", "careful").expect("set agent.profile");
        // A nested profile field via the dynamic key machinery.
        set_value(&path, "agent.profiles.review.tool_sets", "read, edit")
            .expect("set nested profile tool_sets");
        set_value(&path, "agent.profiles.review.self_verify", "false")
            .expect("set nested profile bool");

        let resolved = load_layered(dir.path()).expect("reload");
        assert_eq!(resolved.agent().profile.as_deref(), Some("careful"));
        assert_eq!(
            effective_value(&resolved.merged, "agent.profile").unwrap(),
            Some("careful".to_string())
        );
        assert_eq!(
            effective_value(&resolved.merged, "agent.profiles.review.tool_sets").unwrap(),
            Some("read,edit".to_string())
        );
        assert_eq!(
            effective_value(&resolved.merged, "agent.profiles.review.self_verify").unwrap(),
            Some("false".to_string())
        );

        // Unset prunes the nested key.
        assert!(unset_value(&path, "agent.profiles.review.self_verify").expect("unset nested"));
        let resolved = load_layered(dir.path()).expect("reload after unset");
        assert!(
            effective_value(&resolved.merged, "agent.profiles.review.self_verify")
                .unwrap()
                .is_none()
        );
    });
}

#[test]
fn memory_ranker_keys_round_trip_and_gate_is_fail_closed() {
    let dir = tempfile::tempdir().expect("tempdir");
    with_home(dir.path(), || {
        let path = user_config_path();
        set_value(&path, "memory.ranker", "external").expect("set ranker");
        set_value(&path, "memory.external_url", "https://ranker.internal/rank")
            .expect("set url");

        // Ranker + URL alone do NOT enable external ranking — the governance gate
        // is still off, so it stays fail-closed on offline BM25.
        let resolved = load_layered(dir.path()).expect("reload");
        assert_eq!(resolved.memory().ranker.as_deref(), Some("external"));
        assert_eq!(
            effective_value(&resolved.merged, "memory.external_url").unwrap(),
            Some("https://ranker.internal/rank".to_string())
        );
        assert!(
            !resolved.memory().external_ranking_enabled(),
            "external ranking must stay off until the gate is explicitly enabled"
        );

        // Flip the gate: now all three conditions hold and external ranking is on.
        set_value(&path, "memory.allow_external_ranking", "true").expect("set gate");
        let resolved = load_layered(dir.path()).expect("reload after gate");
        assert!(resolved.memory().external_ranking_enabled());

        // Unsetting the gate returns to fail-closed.
        assert!(unset_value(&path, "memory.allow_external_ranking").expect("unset gate"));
        let resolved = load_layered(dir.path()).expect("reload after unset");
        assert!(!resolved.memory().external_ranking_enabled());
    });
}

#[test]
fn unknown_profile_field_key_is_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    with_home(dir.path(), || {
        let path = user_config_path();
        let error = set_value(&path, "agent.profiles.review.bogus", "x").expect_err("rejected");
        assert!(error.contains("unknown config key"), "got: {error}");
    });
}
