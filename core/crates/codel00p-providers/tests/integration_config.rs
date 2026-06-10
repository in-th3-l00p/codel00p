mod support;

use codel00p_providers::Credential;
use support::IntegrationConfig;

fn with_env_lock(test: impl FnOnce()) {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let keys = [
        "CODEL00P_INTEGRATION_TESTS",
        "CODEL00P_PROVIDER_GITHUB_TOKEN",
        "COPILOT_GITHUB_TOKEN",
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "CODEL00P_PROVIDER_OPENROUTER_API_KEY",
        "CODEL00P_PROVIDER_OPENAI_API_KEY",
        "OPENAI_API_KEY",
        "CODEL00P_PROVIDER_ANTHROPIC_API_KEY",
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_TOKEN",
        "CODEL00P_PROVIDER_AWS_ACCESS_KEY_ID",
        "CODEL00P_PROVIDER_AWS_SECRET_ACCESS_KEY",
        "CODEL00P_PROVIDER_AWS_SESSION_TOKEN",
        "CODEL00P_PROVIDER_AWS_REGION",
        "AWS_ACCESS_KEY_ID",
        "AWS_SECRET_ACCESS_KEY",
        "AWS_SESSION_TOKEN",
        "AWS_REGION",
        "AWS_DEFAULT_REGION",
    ];
    for key in keys {
        unsafe {
            std::env::remove_var(key);
        }
    }
    test();
    for key in keys {
        unsafe {
            std::env::remove_var(key);
        }
    }
}

#[test]
fn integration_tests_are_disabled_by_default() {
    with_env_lock(|| {
        let config = IntegrationConfig::from_env();

        assert!(!config.enabled());
        assert!(config.credential("github").is_none());
    });
}

#[test]
fn integration_tests_can_be_enabled_explicitly() {
    with_env_lock(|| {
        unsafe {
            std::env::set_var("CODEL00P_INTEGRATION_TESTS", "1");
        }

        let config = IntegrationConfig::from_env();

        assert!(config.enabled());
    });
}

#[test]
fn github_credential_prefers_codel00p_specific_variable() {
    with_env_lock(|| {
        unsafe {
            std::env::set_var("CODEL00P_INTEGRATION_TESTS", "true");
            std::env::set_var("CODEL00P_PROVIDER_GITHUB_TOKEN", "preferred");
            std::env::set_var("COPILOT_GITHUB_TOKEN", "fallback");
        }

        let config = IntegrationConfig::from_env();

        assert_eq!(
            config.credential("github"),
            Some(Credential::api_key("preferred"))
        );
    });
}

#[test]
fn github_credential_falls_back_to_copilot_environment_order() {
    with_env_lock(|| {
        unsafe {
            std::env::set_var("CODEL00P_INTEGRATION_TESTS", "true");
            std::env::set_var("GH_TOKEN", "gh-token");
            std::env::set_var("GITHUB_TOKEN", "github-token");
        }

        let config = IntegrationConfig::from_env();

        assert_eq!(
            config.credential("github"),
            Some(Credential::api_key("gh-token"))
        );
    });
}

#[test]
fn anthropic_credential_prefers_codel00p_specific_variable() {
    with_env_lock(|| {
        unsafe {
            std::env::set_var("CODEL00P_INTEGRATION_TESTS", "true");
            std::env::set_var("CODEL00P_PROVIDER_ANTHROPIC_API_KEY", "preferred");
            std::env::set_var("ANTHROPIC_API_KEY", "fallback");
        }

        let config = IntegrationConfig::from_env();

        assert_eq!(
            config.credential("anthropic"),
            Some(Credential::api_key("preferred"))
        );
    });
}

#[test]
fn openai_credential_prefers_codel00p_specific_variable() {
    with_env_lock(|| {
        unsafe {
            std::env::set_var("CODEL00P_INTEGRATION_TESTS", "true");
            std::env::set_var("CODEL00P_PROVIDER_OPENAI_API_KEY", "preferred");
            std::env::set_var("OPENAI_API_KEY", "fallback");
        }

        let config = IntegrationConfig::from_env();

        assert_eq!(
            config.credential("openai"),
            Some(Credential::api_key("preferred"))
        );
    });
}

#[test]
fn bedrock_credential_prefers_codel00p_specific_aws_variables() {
    with_env_lock(|| {
        unsafe {
            std::env::set_var("CODEL00P_INTEGRATION_TESTS", "true");
            std::env::set_var("CODEL00P_PROVIDER_AWS_ACCESS_KEY_ID", "preferred-access");
            std::env::set_var(
                "CODEL00P_PROVIDER_AWS_SECRET_ACCESS_KEY",
                "preferred-secret",
            );
            std::env::set_var("CODEL00P_PROVIDER_AWS_SESSION_TOKEN", "preferred-session");
            std::env::set_var("CODEL00P_PROVIDER_AWS_REGION", "eu-central-1");
            std::env::set_var("AWS_ACCESS_KEY_ID", "fallback-access");
            std::env::set_var("AWS_SECRET_ACCESS_KEY", "fallback-secret");
            std::env::set_var("AWS_REGION", "us-east-1");
        }

        let config = IntegrationConfig::from_env();

        assert_eq!(
            config.credential("bedrock"),
            Some(Credential::aws_sigv4(
                "preferred-access",
                "preferred-secret",
                Some("preferred-session"),
                "eu-central-1",
            ))
        );
    });
}

#[test]
#[should_panic(expected = "missing integration credential for provider `openrouter`")]
fn require_credential_panics_with_actionable_provider_name() {
    with_env_lock(|| {
        unsafe {
            std::env::set_var("CODEL00P_INTEGRATION_TESTS", "true");
        }

        IntegrationConfig::from_env().require_credential("openrouter");
    });
}

#[test]
fn skip_message_explains_disabled_integration_tests() {
    with_env_lock(|| {
        let message = IntegrationConfig::from_env()
            .skip_message("github")
            .expect("disabled integration config should produce skip message");

        assert!(message.contains("CODEL00P_INTEGRATION_TESTS"));
    });
}
