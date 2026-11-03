//! The launch contract for link routers (Manicule): `cooee://join?profile=<name>&url=<meet url>`,
//! and the profiles manifest they read to offer cooee's profiles as targets.

use serde::Serialize;
use url::Url;

use crate::{parse_meet_link, Config, MeetLink};

pub const SCHEME: &str = "cooee";
const MANIFEST_VERSION: u32 = 1;

#[derive(Debug, PartialEq)]
pub struct JoinRequest {
    /// The profile the router chose; `None` lets the link's `authuser` decide.
    pub profile: Option<String>,
    pub link: MeetLink,
}

/// A `cooee://join` URL whose `url` is a Meet link.
pub fn parse_join(s: &str) -> Option<JoinRequest> {
    let u = Url::parse(s).ok()?;
    if u.scheme() != SCHEME || u.host_str() != Some("join") {
        return None;
    }
    let mut profile = None;
    let mut link = None;
    for (k, v) in u.query_pairs() {
        match k.as_ref() {
            "profile" if !v.is_empty() => profile = Some(v.into_owned()),
            "url" => link = parse_meet_link(&v),
            _ => {}
        }
    }
    Some(JoinRequest {
        profile,
        link: link?,
    })
}

#[derive(Serialize)]
struct Manifest<'a> {
    version: u32,
    profiles: Vec<ManifestProfile<'a>>,
}

#[derive(Serialize)]
struct ManifestProfile<'a> {
    name: &'a str,
    account: Option<&'a str>,
}

/// The JSON published at `<app data>/profiles.json` for link routers.
pub fn profiles_manifest(cfg: &Config) -> String {
    serde_json::to_string(&Manifest {
        version: MANIFEST_VERSION,
        profiles: cfg
            .profiles
            .iter()
            .map(|p| ManifestProfile {
                name: &p.name,
                account: p.account.as_deref(),
            })
            .collect(),
    })
    .expect("manifest serialize")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_and_validate;

    #[test]
    fn parse_join_with_profile() {
        let j = parse_join(
            "cooee://join?profile=work&url=https%3A%2F%2Fmeet.google.com%2Fabc-defg-hij",
        )
        .unwrap();
        assert_eq!(j.profile.as_deref(), Some("work"));
        assert_eq!(j.link.url.as_str(), "https://meet.google.com/abc-defg-hij");
    }

    #[test]
    fn parse_join_without_profile_falls_back_to_link_account() {
        let j = parse_join(
            "cooee://join?url=https%3A%2F%2Fmeet.google.com%2Fabc-defg-hij%3Fauthuser%3Dme%40example.com",
        )
        .unwrap();
        assert_eq!(j.profile, None);
        assert_eq!(j.link.authuser.as_deref(), Some("me@example.com"));
    }

    #[test]
    fn parse_join_rejects_non_meet_url() {
        assert_eq!(
            parse_join("cooee://join?url=https%3A%2F%2Fexample.com%2F"),
            None
        );
        assert_eq!(parse_join("cooee://join?profile=work"), None);
        assert_eq!(
            parse_join("cooee://other?url=https%3A%2F%2Fmeet.google.com%2Fabc"),
            None
        );
        assert_eq!(parse_join("https://meet.google.com/abc-defg-hij"), None);
    }

    #[test]
    fn manifest_json_shape() {
        let cfg = parse_and_validate(
            "[[profile]]\nname = \"work\"\naccount = \"me@example.com\"\n[[profile]]\nname = \"home\"\n",
        )
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&profiles_manifest(&cfg)).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"version": 1, "profiles": [
                {"name": "work", "account": "me@example.com"},
                {"name": "home", "account": null}
            ]})
        );
    }
}
