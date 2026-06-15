//! The organization directory: a read-only projection of Clerk's membership data.
//!
//! codel00p does not own membership; Clerk is the source of truth. This module
//! calls the Clerk **Backend API** with a secret key and maps the response into
//! the protocol `OrgMember` shape the rest of the system renders. The API base is
//! overridable so the path is testable against a mock without a live Clerk.

use std::env;

use codel00p_protocol::{OrgMember, OrgRef, OrgRole};
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
            let total_count = payload
                .total_count
                .unwrap_or(payload.data.len() + offset as usize);
            members.extend(payload.data.into_iter().filter_map(Membership::into_member));
            if members.len() >= total_count || page_len == 0 {
                break;
            }
            offset += PAGE_LIMIT;
        }
        Ok(members)
    }

    /// Lists the organizations a user belongs to, using Clerk as the source of
    /// truth. The returned refs are suitable for client-side org switching.
    pub async fn list_user_orgs(&self, user_id: &str) -> Result<Vec<OrgRef>, ApiError> {
        let mut offset = 0;
        let mut orgs = Vec::new();
        loop {
            let payload = self.list_user_orgs_page(user_id, offset).await?;
            let page_len = payload.data.len();
            let total_count = payload
                .total_count
                .unwrap_or(payload.data.len() + offset as usize);
            orgs.extend(
                payload
                    .data
                    .into_iter()
                    .filter_map(UserMembership::into_org),
            );
            if orgs.len() >= total_count || page_len == 0 {
                break;
            }
            offset += PAGE_LIMIT;
        }
        Ok(orgs)
    }

    async fn list_members_page(
        &self,
        org_id: &str,
        offset: u32,
    ) -> Result<MembershipList, ApiError> {
        let url = format!("{}/v1/organizations/{org_id}/memberships", self.api_base);
        let response = self
            .http
            .get(url)
            .query(&[("limit", PAGE_LIMIT), ("offset", offset)])
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

    async fn list_user_orgs_page(
        &self,
        user_id: &str,
        offset: u32,
    ) -> Result<UserMembershipList, ApiError> {
        let user_id = path_encode(user_id);
        let url = format!(
            "{}/v1/users/{user_id}/organization_memberships",
            self.api_base
        );
        let response = self
            .http
            .get(url)
            .query(&[("limit", PAGE_LIMIT), ("offset", offset)])
            .bearer_auth(&self.secret_key)
            .send()
            .await
            .map_err(|err| ApiError::Internal(format!("clerk request failed: {err}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ApiError::Internal(format!(
                "clerk organization lookup failed ({status}): {body}"
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
struct UserMembershipList {
    #[serde(default)]
    data: Vec<UserMembership>,
    total_count: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct UserMembership {
    organization: Option<ClerkOrganization>,
}

#[derive(Debug, Deserialize)]
struct ClerkOrganization {
    id: String,
    name: String,
    #[serde(default)]
    slug: Option<String>,
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
    fn into_member(self) -> Option<OrgMember> {
        let user = self.public_user_data?;
        let user_id = user.valid_user_id()?;
        let role = self
            .role
            .as_deref()
            .and_then(OrgRole::from_clerk_claim)
            .unwrap_or(OrgRole::Member);

        let mut member = OrgMember::new(user_id, role);
        if let Some(email) = user.email() {
            member = member.with_email(email);
        }
        if let Some(name) = user.display_name() {
            member = member.with_name(name);
        }
        Some(member)
    }
}

impl UserMembership {
    fn into_org(self) -> Option<OrgRef> {
        let org = self.organization?;
        let id = org.id.trim();
        let name = org.name.trim();
        if id.is_empty() || name.is_empty() {
            return None;
        }
        let mut org_ref = OrgRef::new(id, name);
        if let Some(slug) = org.slug {
            org_ref = org_ref.with_slug(slug);
        }
        Some(org_ref)
    }
}

impl PublicUserData {
    fn valid_user_id(&self) -> Option<String> {
        self.user_id
            .as_deref()
            .map(str::trim)
            .filter(|user_id| !user_id.is_empty())
            .map(str::to_string)
    }

    fn email(&self) -> Option<String> {
        self.identifier
            .as_deref()
            .map(str::trim)
            .filter(|identifier| !identifier.is_empty())
            .map(str::to_string)
    }

    /// Joins a first and last name into a display name, tolerating either being
    /// absent. Returns `None` when both are empty.
    fn display_name(&self) -> Option<String> {
        let name = [self.first_name.as_deref(), self.last_name.as_deref()]
            .into_iter()
            .flatten()
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if name.is_empty() { None } else { Some(name) }
    }
}

fn path_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
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
        let member = membership.into_member().expect("member");
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
        let member = membership.into_member().expect("member");
        assert_eq!(member.role(), OrgRole::Member);
        assert_eq!(member.name(), None);
        assert_eq!(member.email(), Some("mem@example.com"));
    }

    #[test]
    fn join_name_handles_partial_names() {
        let mut user = PublicUserData {
            first_name: Some("Ada".into()),
            ..PublicUserData::default()
        };
        assert_eq!(user.display_name(), Some("Ada".to_string()));
        user = PublicUserData {
            last_name: Some("Lovelace".into()),
            ..PublicUserData::default()
        };
        assert_eq!(user.display_name(), Some("Lovelace".to_string()));
        user = PublicUserData {
            first_name: Some("  ".into()),
            ..PublicUserData::default()
        };
        assert_eq!(user.display_name(), None);
        assert_eq!(PublicUserData::default().display_name(), None);
    }

    #[test]
    fn drops_membership_without_user_id() {
        let membership = Membership {
            role: Some("org:member".into()),
            public_user_data: Some(PublicUserData {
                user_id: Some(" ".into()),
                identifier: Some("mem@example.com".into()),
                ..PublicUserData::default()
            }),
        };
        assert!(membership.into_member().is_none());
    }

    #[test]
    fn maps_user_membership_to_org_ref() {
        let membership = UserMembership {
            organization: Some(ClerkOrganization {
                id: "org_acme".into(),
                name: "Acme".into(),
                slug: Some("acme".into()),
            }),
        };
        let org = membership.into_org().expect("org");
        assert_eq!(org.id(), "org_acme");
        assert_eq!(org.name(), "Acme");
        assert_eq!(org.slug(), Some("acme"));
    }

    #[test]
    fn drops_user_membership_without_valid_org() {
        let membership = UserMembership {
            organization: Some(ClerkOrganization {
                id: " ".into(),
                name: "Acme".into(),
                slug: None,
            }),
        };
        assert!(membership.into_org().is_none());
        assert!(UserMembership { organization: None }.into_org().is_none());
    }
}
