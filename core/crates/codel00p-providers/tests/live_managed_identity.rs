mod support;

use codel00p_providers::{
    AwsManagedIdentityCredentialResolver, AzureManagedIdentityCredentialResolver,
    AzureManagedIdentitySelector, CredentialKind, GcpManagedIdentityCredentialResolver,
};
use support::IntegrationConfig;

#[tokio::test]
#[ignore = "requires CODEL00P_INTEGRATION_TESTS=1, CODEL00P_PROVIDER_AZURE_MANAGED_IDENTITY_TESTS=1, and an Azure runtime with managed identity"]
async fn live_azure_managed_identity_resolver_smoke_test() {
    let config = IntegrationConfig::from_env();
    if let Some(message) = config.skip_azure_managed_identity_message() {
        eprintln!("{message}");
        return;
    }

    let live = config.require_azure_managed_identity();
    let resolver = match live.selector {
        AzureManagedIdentitySelector::SystemAssigned => {
            AzureManagedIdentityCredentialResolver::system_assigned(live.resource)
        }
        AzureManagedIdentitySelector::ClientId(client_id) => {
            AzureManagedIdentityCredentialResolver::user_assigned_client_id(
                live.resource,
                client_id,
            )
        }
        AzureManagedIdentitySelector::ObjectId(object_id) => {
            AzureManagedIdentityCredentialResolver::user_assigned_object_id(
                live.resource,
                object_id,
            )
        }
        AzureManagedIdentitySelector::ResourceId(resource_id) => {
            AzureManagedIdentityCredentialResolver::user_assigned_resource_id(
                live.resource,
                resource_id,
            )
        }
    };

    let credential = resolver
        .resolve("azure-foundry", live.identity_ref)
        .await
        .expect("Azure managed identity resolver should return a token");

    assert_eq!(credential.kind(), CredentialKind::ApiKey);
}

#[tokio::test]
#[ignore = "requires CODEL00P_INTEGRATION_TESTS=1, CODEL00P_PROVIDER_AWS_MANAGED_IDENTITY_TESTS=1, and an EC2 runtime with an instance profile"]
async fn live_aws_managed_identity_resolver_smoke_test() {
    let config = IntegrationConfig::from_env();
    if let Some(message) = config.skip_aws_managed_identity_message() {
        eprintln!("{message}");
        return;
    }

    let live = config.require_aws_managed_identity();
    let mut resolver = AwsManagedIdentityCredentialResolver::instance_profile(live.region);
    if let Some(role_name) = live.role_name {
        resolver = resolver.with_role_name(role_name);
    }

    let credential = resolver
        .resolve("bedrock", live.identity_ref)
        .await
        .expect("AWS managed identity resolver should return SigV4 credentials");

    assert_eq!(credential.kind(), CredentialKind::AwsSigV4);
}

#[tokio::test]
#[ignore = "requires CODEL00P_INTEGRATION_TESTS=1, CODEL00P_PROVIDER_GCP_MANAGED_IDENTITY_TESTS=1, and a GCP runtime with an attached service account"]
async fn live_gcp_managed_identity_resolver_smoke_test() {
    let config = IntegrationConfig::from_env();
    if let Some(message) = config.skip_gcp_managed_identity_message() {
        eprintln!("{message}");
        return;
    }

    let live = config.require_gcp_managed_identity();
    let resolver = GcpManagedIdentityCredentialResolver::service_account(live.service_account);

    let credential = resolver
        .resolve("custom", live.identity_ref)
        .await
        .expect("GCP managed identity resolver should return a token");

    assert_eq!(credential.kind(), CredentialKind::ApiKey);
}
