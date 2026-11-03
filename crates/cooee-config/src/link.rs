//! Meet links: recognising one, and choosing the profile it should open in.

use crate::{Config, Profile};
use url::Url;

/// The one host cooee opens. Everything else is refused or sent to the default browser.
pub const MEET_HOST: &str = "meet.google.com";

#[derive(Debug, Clone, PartialEq)]
pub struct MeetLink {
    pub url: Url,
    /// The link's `authuser` query value (an email, or an account index), if any.
    pub authuser: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum ProfilePick<'a> {
    Profile(&'a Profile),
    /// No account match among several profiles: the user chooses.
    Ask,
}

/// A Meet link from arbitrary text (a clipboard, a deep link). HTTPS on exactly [`MEET_HOST`].
pub fn parse_meet_link(s: &str) -> Option<MeetLink> {
    let url = Url::parse(s.trim()).ok()?;
    if url.scheme() != "https" || url.host_str() != Some(MEET_HOST) {
        return None;
    }
    let authuser = url
        .query_pairs()
        .find(|(k, _)| k == "authuser")
        .map(|(_, v)| v.into_owned());
    Some(MeetLink { url, authuser })
}

/// The profile whose `account` equals the link's `authuser` (case-insensitive). With a single
/// profile there is nothing to ask, so it always wins.
pub fn profile_for_link<'a>(cfg: &'a Config, link: &MeetLink) -> ProfilePick<'a> {
    if let [only] = cfg.profiles.as_slice() {
        return ProfilePick::Profile(only);
    }
    link.authuser
        .as_deref()
        .and_then(|who| {
            cfg.profiles.iter().find(|p| {
                p.account
                    .as_deref()
                    .is_some_and(|a| a.eq_ignore_ascii_case(who))
            })
        })
        .map_or(ProfilePick::Ask, ProfilePick::Profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_and_validate;

    #[test]
    fn parse_meet_link_accepts_code_and_path_forms() {
        let a = parse_meet_link("https://meet.google.com/abc-defg-hij").unwrap();
        assert_eq!(a.url.as_str(), "https://meet.google.com/abc-defg-hij");
        assert_eq!(a.authuser, None);
        let b = parse_meet_link("https://meet.google.com/meet/abc-defg-hij?authuser=a@example.com")
            .unwrap();
        assert_eq!(b.authuser.as_deref(), Some("a@example.com"));
    }

    #[test]
    fn parse_meet_link_trims_clipboard_whitespace() {
        assert!(parse_meet_link("  https://meet.google.com/abc-defg-hij\n").is_some());
    }

    #[test]
    fn parse_meet_link_rejects_other_hosts() {
        assert_eq!(parse_meet_link("https://example.com/abc"), None);
        assert_eq!(
            parse_meet_link("https://meet.google.com.evil.example/abc"),
            None
        );
        assert_eq!(parse_meet_link("not a url"), None);
        assert_eq!(parse_meet_link("http://meet.google.com/abc-defg-hij"), None);
    }

    fn two_profiles() -> crate::Config {
        parse_and_validate(
            "[[profile]]\nname = \"work\"\naccount = \"Me@Example.com\"\n[[profile]]\nname = \"home\"\naccount = \"me@home.example\"\n",
        )
        .unwrap()
    }

    #[test]
    fn profile_for_link_matches_account() {
        let cfg = two_profiles();
        let link = parse_meet_link("https://meet.google.com/abc-defg-hij?authuser=me@example.com")
            .unwrap();
        match profile_for_link(&cfg, &link) {
            ProfilePick::Profile(p) => assert_eq!(p.name, "work"),
            ProfilePick::Ask => panic!("expected work"),
        }
    }

    #[test]
    fn profile_for_link_single_profile_needs_no_ask() {
        let cfg = parse_and_validate("").unwrap();
        let link = parse_meet_link("https://meet.google.com/abc-defg-hij?authuser=x@y.z").unwrap();
        assert!(
            matches!(profile_for_link(&cfg, &link), ProfilePick::Profile(p) if p.name == "default")
        );
    }

    #[test]
    fn profile_for_link_unknown_account_asks() {
        let cfg = two_profiles();
        let link =
            parse_meet_link("https://meet.google.com/abc-defg-hij?authuser=other@example.com")
                .unwrap();
        assert!(matches!(profile_for_link(&cfg, &link), ProfilePick::Ask));
        let bare = parse_meet_link("https://meet.google.com/abc-defg-hij").unwrap();
        assert!(matches!(profile_for_link(&cfg, &bare), ProfilePick::Ask));
    }
}
