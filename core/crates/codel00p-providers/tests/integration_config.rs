mod support;

use codel00p_providers::{AzureManagedIdentitySelector, Credential};
use support::IntegrationConfig;

fn with_env_lock(test: impl FnOnce()) {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let keys = [
        "CODEL00P_INTEGRATION_TESTS",
        "CODEL00P_PROVIDER_GITHUB_TOKEN",
        "CODEL00P_PROVIDER_GITHUB_MODELS_TOKEN",
        "CODEL00P_PROVIDER_GITHUB_MODELS_MODEL",
        "COPILOT_GITHUB_TOKEN",
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "CODEL00P_PROVIDER_OPENROUTER_API_KEY",
        "CODEL00P_PROVIDER_OPENAI_API_KEY",
        "OPENAI_API_KEY",
        "CODEL00P_PROVIDER_ANTHROPIC_API_KEY",
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_TOKEN",
        "CODEL00P_PROVIDER_AZURE_FOUNDRY_API_KEY",
        "AZURE_FOUNDRY_API_KEY",
        "CODEL00P_PROVIDER_AZURE_FOUNDRY_ENDPOINT",
        "AZURE_FOUNDRY_ENDPOINT",
        "AZURE_OPENAI_ENDPOINT",
        "CODEL00P_PROVIDER_AZURE_FOUNDRY_DEPLOYMENT",
        "AZURE_FOUNDRY_DEPLOYMENT",
        "AZURE_OPENAI_DEPLOYMENT",
        "CODEL00P_PROVIDER_AZURE_FOUNDRY_API_VERSION",
        "AZURE_FOUNDRY_API_VERSION",
        "AZURE_OPENAI_API_VERSION",
        "CODEL00P_PROVIDER_AZURE_MANAGED_IDENTITY_TESTS",
        "CODEL00P_PROVIDER_AZURE_MANAGED_IDENTITY_RESOURCE",
        "CODEL00P_PROVIDER_AZURE_MANAGED_IDENTITY_CLIENT_ID",
        "CODEL00P_PROVIDER_AZURE_MANAGED_IDENTITY_OBJECT_ID",
        "CODEL00P_PROVIDER_AZURE_MANAGED_IDENTITY_RESOURCE_ID",
        "CODEL00P_PROVIDER_AWS_MANAGED_IDENTITY_TESTS",
        "CODEL00P_PROVIDER_AWS_MANAGED_IDENTITY_ROLE",
        "CODEL00P_PROVIDER_GCP_MANAGED_IDENTITY_TESTS",
        "CODEL00P_PROVIDER_GCP_MANAGED_IDENTITY_SERVICE_ACCOUNT",
        "CODEL00P_PROVIDER_AWS_ACCESS_KEY_ID",
        "CODEL00P_PROVIDER_AWS_SECRET_ACCESS_KEY",
        "CODEL00P_PROVIDER_AWS_SESSION_TOKEN",
        "CODEL00P_PROVIDER_AWS_REGION",
        "AWS_ACCESS_KEY_ID",
        "AWS_SECRET_ACCESS_KEY",
        "AWS_SESSION_TOKEN",
        "AWS_REGION",
        "AWS_DEFAULT_REGION",
        "CODEL00P_PROVIDER_GEMINI_API_KEY",
        "GOOGLE_API_KEY",
        "GEMINI_API_KEY",
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
fn github_models_credential_prefers_models_specific_variable() {
    with_env_lock(|| {
        unsafe {
            std::env::set_var("CODEL00P_INTEGRATION_TESTS", "true");
            std::env::set_var("CODEL00P_PROVIDER_GITHUB_MODELS_TOKEN", "preferred");
            std::env::set_var("GITHUB_TOKEN", "fallback");
        }

        let config = IntegrationConfig::from_env();

        assert_eq!(
            config.credential("github-models"),
            Some(Credential::api_key("preferred"))
        );
    });
}

#[test]
fn github_models_credential_falls_back_to_github_token_before_gh_token() {
    with_env_lock(|| {
        unsafe {
            std::env::set_var("CODEL00P_INTEGRATION_TESTS", "true");
            std::env::set_var("GITHUB_TOKEN", "github-token");
            std::env::set_var("GH_TOKEN", "gh-token");
        }

        let config = IntegrationConfig::from_env();

        assert_eq!(
            config.credential("gh-models"),
            Some(Credential::api_key("github-token"))
        );
    });
}

#[test]
fn github_models_model_defaults_to_a_low_cost_catalog_model() {
    with_env_lock(|| {
        let config = IntegrationConfig::from_env();

        assert_eq!(config.github_models_model(), "openai/gpt-4o-mini");
    });
}

#[test]
fn github_models_model_reads_provider_specific_override() {
    with_env_lock(|| {
        unsafe {
            std::env::set_var(
                "CODEL00P_PROVIDER_GITHUB_MODELS_MODEL",
                "openai/gpt-4.1-mini",
            );
        }

        let config = IntegrationConfig::from_env();

        assert_eq!(config.github_models_model(), "openai/gpt-4.1-mini");
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
fn azure_credential_prefers_codel00p_specific_variable() {
    with_env_lock(|| {
        unsafe {
            std::env::set_var("CODEL00P_INTEGRATION_TESTS", "true");
            std::env::set_var("CODEL00P_PROVIDER_AZURE_FOUNDRY_API_KEY", "preferred");
            std::env::set_var("AZURE_FOUNDRY_API_KEY", "fallback");
        }

        let config = IntegrationConfig::from_env();

        assert_eq!(
            config.credential("azure"),
            Some(Credential::api_key("preferred"))
        );
    });
}

#[test]
fn azure_foundry_config_reads_endpoint_deployment_and_api_version() {
    with_env_lock(|| {
        unsafe {
            std::env::set_var("CODEL00P_INTEGRATION_TESTS", "true");
            std::env::set_var("CODEL00P_PROVIDER_AZURE_FOUNDRY_API_KEY", "azure-key");
            std::env::set_var(
                "CODEL00P_PROVIDER_AZURE_FOUNDRY_ENDPOINT",
                "https://team.openai.azure.com",
            );
            std::env::set_var("CODEL00P_PROVIDER_AZURE_FOUNDRY_DEPLOYMENT", "team-chat");
            std::env::set_var(
                "CODEL00P_PROVIDER_AZURE_FOUNDRY_API_VERSION",
                "2024-08-01-preview",
            );
        }

        let azure = IntegrationConfig::from_env()
            .azure_foundry()
            .expect("complete Azure config should be present");

        assert_eq!(azure.credential, Credential::api_key("azure-key"));
        assert_eq!(azure.endpoint, "https://team.openai.azure.com");
        assert_eq!(azure.deployment, "team-chat");
        assert_eq!(azure.api_version, "2024-08-01-preview");
    });
}

#[test]
fn azure_foundry_config_defaults_api_version() {
    with_env_lock(|| {
        unsafe {
            std::env::set_var("CODEL00P_INTEGRATION_TESTS", "true");
            std::env::set_var("AZURE_FOUNDRY_API_KEY", "azure-key");
            std::env::set_var("AZURE_OPENAI_ENDPOINT", "https://team.openai.azure.com");
            std::env::set_var("AZURE_OPENAI_DEPLOYMENT", "team-chat");
        }

        let azure = IntegrationConfig::from_env()
            .azure_foundry()
            .expect("complete Azure config should be present");

        assert_eq!(azure.api_version, "2024-10-21");
    });
}

#[test]
fn azure_foundry_skip_message_reports_missing_endpoint_without_secrets() {
    with_env_lock(|| {
        unsafe {
            std::env::set_var("CODEL00P_INTEGRATION_TESTS", "true");
            std::env::set_var("CODEL00P_PROVIDER_AZURE_FOUNDRY_API_KEY", "secret-key");
            std::env::set_var("CODEL00P_PROVIDER_AZURE_FOUNDRY_DEPLOYMENT", "team-chat");
        }

        let message = IntegrationConfig::from_env()
            .skip_azure_foundry_message()
            .expect("missing endpoint should produce skip message");

        assert!(message.contains("endpoint"));
        assert!(message.contains("CODEL00P_PROVIDER_AZURE_FOUNDRY_ENDPOINT"));
        assert!(!message.contains("secret-key"));
    });
}

#[test]
fn managed_identity_live_tests_require_cloud_opt_in() {
    with_env_lock(|| {
        unsafe {
            std::env::set_var("CODEL00P_INTEGRATION_TESTS", "true");
        }

        let message = IntegrationConfig::from_env()
            .skip_azure_managed_identity_message()
            .expect("missing Azure managed identity opt-in should produce skip message");

        assert!(message.contains("CODEL00P_PROVIDER_AZURE_MANAGED_IDENTITY_TESTS"));
    });
}

#[test]
fn azure_managed_identity_config_reads_resource_and_client_id_selector() {
    with_env_lock(|| {
        unsafe {
            std::env::set_var("CODEL00P_INTEGRATION_TESTS", "true");
            std::env::set_var("CODEL00P_PROVIDER_AZURE_MANAGED_IDENTITY_TESTS", "true");
            std::env::set_var(
                "CODEL00P_PROVIDER_AZURE_MANAGED_IDENTITY_RESOURCE",
                "https://cognitiveservices.azure.com/",
            );
            std::env::set_var(
                "CODEL00P_PROVIDER_AZURE_MANAGED_IDENTITY_CLIENT_ID",
                "client-123",
            );
        }

        let azure = IntegrationConfig::from_env()
            .azure_managed_identity()
            .expect("Azure managed identity config should be present");

        assert_eq!(azure.resource, "https://cognitiveservices.azure.com/");
        assert_eq!(azure.identity_ref, "azure/managed-identity");
        assert!(
            matches!(azure.selector, AzureManagedIdentitySelector::ClientId(ref client_id)
                if client_id == "client-123")
        );
    });
}

#[test]
fn aws_managed_identity_config_reads_region_and_role_name() {
    with_env_lock(|| {
        unsafe {
            std::env::set_var("CODEL00P_INTEGRATION_TESTS", "true");
            std::env::set_var("CODEL00P_PROVIDER_AWS_MANAGED_IDENTITY_TESTS", "true");
            std::env::set_var("CODEL00P_PROVIDER_AWS_REGION", "eu-central-1");
            std::env::set_var(
                "CODEL00P_PROVIDER_AWS_MANAGED_IDENTITY_ROLE",
                "bedrock-role",
            );
        }

        let aws = IntegrationConfig::from_env()
            .aws_managed_identity()
            .expect("AWS managed identity config should be present");

        assert_eq!(aws.region, "eu-central-1");
        assert_eq!(aws.identity_ref, "aws/instance-profile");
        assert_eq!(aws.role_name.as_deref(), Some("bedrock-role"));
    });
}

#[test]
fn gcp_managed_identity_config_defaults_service_account() {
    with_env_lock(|| {
        unsafe {
            std::env::set_var("CODEL00P_INTEGRATION_TESTS", "true");
            std::env::set_var("CODEL00P_PROVIDER_GCP_MANAGED_IDENTITY_TESTS", "true");
        }

        let gcp = IntegrationConfig::from_env()
            .gcp_managed_identity()
            .expect("GCP managed identity config should be present");

        assert_eq!(gcp.service_account, "default");
        assert_eq!(gcp.identity_ref, "gcp/default-service-account");
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
fn gemini_credential_prefers_codel00p_specific_variable() {
    with_env_lock(|| {
        unsafe {
            std::env::set_var("CODEL00P_INTEGRATION_TESTS", "true");
            std::env::set_var("CODEL00P_PROVIDER_GEMINI_API_KEY", "preferred");
            std::env::set_var("GOOGLE_API_KEY", "fallback");
        }

        let config = IntegrationConfig::from_env();

        assert_eq!(
            config.credential("gemini"),
            Some(Credential::api_key("preferred"))
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
