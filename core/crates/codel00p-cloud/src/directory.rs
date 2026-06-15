//! The organization directory: a read-only projection of Clerk's membership data.
//!
//! codel00p does not own membership; Clerk is the source of truth. This module
//! calls the Clerk **Backend API** with a secret key and maps the response into
//! the protocol `OrgMember` shape the rest of the system renders. The API base is
//! overridable so the path is testable against a mock without a live Clerk.

use std::env;

use codel00p_protocol::{OrgMember, OrgRole};
use serde::Deserialize;

use crate::error::ApiError;

const DEFAULT_API_BASE: &str = "https://api.clerk.com";
/// Clerk caps `limit` at 100 per page; a single page covers typical teams.
const PAGE_LIMIT: u32 = 100;

/// Reads an organization's members from the Clerk Backend API.
#[derive(Clone)]
pub struct ClerkDirectory {
    http: reqwest::Client,
    secret_key: String,
    api_base: String,
}

impl ClerkDirectory {
    /// Builds a directory from an explicit secret key and API base.
    pub fn new(secret_key: impl Into<String>, api_base: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            secret_key: secret_key.into(),
            api_base: api_base.into().trim_end_matches('/').to_string(),
        }
    }

    /// Builds a directory from the environment, or `None` when `CLERK_SECRET_KEY`
    /// is unset - the service then runs without a live member directory and the
    /// members route reports it as unconfigured. `CLERK_API_BASE` overrides the
    /// default host (used in tests).
    pub fn from_env() -> Option<Self> {
        let secret_key = env::var("CLERK_SECRET_KEY")
            .ok()
            .filter(|key| !key.trim().is_empty())?;
        let api_base = env::var("CLERK_API_BASE")
            .ok()
            .filter(|base| !base.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_API_BASE.to_string());
        Some(Self::new(secret_key, api_base))
    }

    /// Lists the members of `org_id`, ordered by Clerk's default (role, then join
    /// date). Maps Clerk's membership records into protocol `OrgMember`s.
    pub async fn list_members(&self, org_id: &str) -> Result<Vec<OrgMember>, ApiError> {
        let mut offset = 0;
        let mut members = Vec::new();
        loop {
            let payload = self.list_members_page(org_id, offset).await?;
            let page_len = payload.data.len();
            let total_count = payload.total_count.unwrap_or(payload.data.len() + offset as usize);
            members.extend(payload.data.into_iter().map(Membership::into_member));
            if members.len() >= total_count || page_len == 0 {
                break;
            }
            offset += PAGE_LIMIT;
        }
        Ok(members)
    }

    async fn list_members_page(
        &self,
        org_id: &str,
        offset: u32,
    ) -> Result<MembershipList, ApiError> {
        let url = format!(
            "{}/v1/organizations/{org_id}/memberships?limit={PAGE_LIMIT}&offset={offset}",
            self.api_base
        );
        let response = self
            .http
            .get(url)
            .bearer_auth(&self.secret_key)
            .send()
            .await
            .map_err(|err| ApiError::Internal(format!("clerk request failed: {err}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ApiError::Internal(format!(
                "clerk membership lookup failed ({status}): {body}"
            )));
        }

        response
            .json()
            .await
            .map_err(|err| ApiError::Internal(format!("invalid clerk response: {err}")))
    }
}

#[derive(Debug, Deserialize)]
struct MembershipList {
    #[serde(default)]
    data: Vec<Membership>,
    total_count: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct Membership {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    public_user_data: Option<PublicUserData>,
}

#[derive(Debug, Default, Deserialize)]
struct PublicUserData {
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    first_name: Option<String>,
    #[serde(default)]
    last_name: Option<String>,
    #[serde(default)]
    identifier: Option<String>,
}

impl Membership {
    fn into_member(self) -> OrgMember {
        let user = self.public_user_data.unwrap_or_default();
        let role = self
            .role
            .as_deref()
            .and_then(OrgRole::from_clerk_claim)
            .unwrap_or(OrgRole::Member);
        let user_id = user.user_id.unwrap_or_default();

        let mut member = OrgMember::new(user_id, role);
        if let Some(email) = user.identifier {
            member = member.with_email(email);
        }
        if let Some(name) = join_name(user.first_name, user.last_name) {
            member = member.with_name(name);
        }
        member
    }
}

/// Joins a first and last name into a display name, tolerating either being
/// absent. Returns `None` when both are empty.
fn join_name(first: Option<String>, last: Option<String>) -> Option<String> {
    let name = [first, last]
        .into_iter()
        .flatten()
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_role_name_and_email() {
        let membership = Membership {
            role: Some("org:admin".into()),
            public_user_data: Some(PublicUserData {
                user_id: Some("user_1".into()),
                first_name: Some("Ada".into()),
                last_name: Some("Lovelace".into()),
                identifier: Some("ada@example.com".into()),
            }),
        };
        let member = membership.into_member();
        assert_eq!(member.user_id(), "user_1");
        assert!(member.role().is_admin());
        assert_eq!(member.name(), Some("Ada Lovelace"));
        assert_eq!(member.email(), Some("ada@example.com"));
    }

    #[test]
    fn tolerates_missing_names_and_unknown_role() {
        let membership = Membership {
            role: None,
            public_user_data: Some(PublicUserData {
                user_id: Some("user_2".into()),
                first_name: None,
                last_name: None,
                identifier: Some("mem@example.com".into()),
            }),
        };
        let member = membership.into_member();
        assert_eq!(member.role(), OrgRole::Member);
        assert_eq!(member.name(), None);
        assert_eq!(member.email(), Some("mem@example.com"));
    }

    #[test]
    fn join_name_handles_partial_names() {
        assert_eq!(join_name(Some("Ada".into()), None), Some("Ada".to_string()));
        assert_eq!(join_name(None, Some("Lovelace".into())), Some("Lovelace".to_string()));
        assert_eq!(join_name(Some("  ".into()), None), None);
        assert_eq!(join_name(None, None), None);
    }
}
