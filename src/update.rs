//! Tells the user when a newer ytmfast release is on GitHub.
//!
//! The running version is the `version` in Cargo.toml, compiled in through
//! `CARGO_PKG_VERSION`, so every release bumps it before its tag is pushed.
//! The check asks GitHub for the latest published release at launch and then
//! every [`CHECK_INTERVAL`]. A failed check is logged and otherwise ignored:
//! an update notice is never worth an error.

use std::sync::mpsc::Sender;
use std::time::Duration;

use reqwest::StatusCode;
use semver::Version;
use serde::Deserialize;

use crate::backend::Event;

const REPO_URL: &str = "https://github.com/carlonzo/ytmfast";
const LATEST_API: &str = "https://api.github.com/repos/carlonzo/ytmfast/releases/latest";
const USER_AGENT: &str = concat!(
    "ytmfast/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/carlonzo/ytmfast)"
);
/// How often a running app asks GitHub for a newer release.
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// A published release newer than the running build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    /// The release page on GitHub, where the download is.
    pub url: String,
}

#[derive(Deserialize)]
struct LatestRelease {
    tag_name: String,
}

/// The version this build was made from.
pub fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("Cargo.toml version is semver")
}

/// The release `tag` names, when it is newer than `current`. Tags are a `v`
/// and a semver version, like `v0.2.0`; anything else cannot be compared.
fn newer_release(current: &Version, tag: &str) -> Option<Release> {
    let version = Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()?;
    (version > *current).then(|| Release {
        url: format!("{REPO_URL}/releases/tag/{tag}"),
        version,
    })
}

/// Reads GitHub's answer to the latest-release request. A 404 means no
/// release has been published yet: nothing is newer, and that is not a fault.
fn from_response(
    status: StatusCode,
    body: &str,
    current: &Version,
) -> Result<Option<Release>, String> {
    if status == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !status.is_success() {
        return Err(format!("GitHub answered {status}"));
    }
    let latest: LatestRelease =
        serde_json::from_str(body).map_err(|e| format!("unexpected answer from GitHub: {e}"))?;
    Ok(newer_release(current, &latest.tag_name))
}

async fn check(http: &reqwest::Client, current: &Version) -> Result<Option<Release>, String> {
    let response = http
        .get(LATEST_API)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .map_err(|e| format!("cannot reach GitHub: {e}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("cannot read GitHub's answer: {e}"))?;
    from_response(status, &body, current)
}

/// Runs for the life of the app. The first tick of an interval is immediate,
/// so the first check happens at launch.
pub async fn watch(events: Sender<Event>, ctx: crate::Repaint) {
    let Ok(http) = reqwest::Client::builder().timeout(REQUEST_TIMEOUT).build() else {
        return;
    };
    let current = current_version();
    let mut ticks = tokio::time::interval(CHECK_INTERVAL);
    loop {
        ticks.tick().await;
        match check(&http, &current).await {
            Ok(newer) => {
                let _ = events.send(Event::UpdateChecked(newer));
                ctx.request_repaint();
            }
            Err(error) => eprintln!("Update check failed: {error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    const LATEST_BODY: &str = r#"{
        "tag_name": "v0.2.0",
        "name": "0.2.0",
        "draft": false,
        "prerelease": false,
        "html_url": "https://github.com/carlonzo/ytmfast/releases/tag/v0.2.0"
    }"#;

    #[test]
    fn a_newer_tag_is_an_update_with_its_release_page() {
        let release = newer_release(&version("0.1.0"), "v0.2.0").unwrap();
        assert_eq!(release.version, version("0.2.0"));
        assert_eq!(
            release.url,
            "https://github.com/carlonzo/ytmfast/releases/tag/v0.2.0"
        );
    }

    #[test]
    fn the_running_version_and_older_ones_are_not_updates() {
        assert_eq!(newer_release(&version("0.2.0"), "v0.2.0"), None);
        assert_eq!(newer_release(&version("0.3.0"), "v0.2.0"), None);
    }

    #[test]
    fn versions_compare_by_number_not_by_text() {
        assert!(newer_release(&version("0.9.0"), "v0.10.0").is_some());
    }

    #[test]
    fn a_release_candidate_is_older_than_the_release_it_leads_to() {
        assert!(newer_release(&version("0.2.0-rc.1"), "v0.2.0").is_some());
        assert!(newer_release(&version("0.2.0"), "v0.2.0-rc.1").is_none());
    }

    #[test]
    fn tags_that_are_not_versions_are_not_updates() {
        assert_eq!(newer_release(&version("0.1.0"), "nightly"), None);
    }

    #[test]
    fn no_release_yet_is_not_an_error() {
        let answer = from_response(
            StatusCode::NOT_FOUND,
            r#"{"message":"Not Found"}"#,
            &version("0.1.0"),
        );
        assert_eq!(answer, Ok(None));
    }

    #[test]
    fn a_newer_latest_release_is_reported() {
        let answer = from_response(StatusCode::OK, LATEST_BODY, &version("0.1.0")).unwrap();
        assert_eq!(
            answer.map(|release| release.version),
            Some(version("0.2.0"))
        );
    }

    #[test]
    fn a_refused_request_or_a_garbled_answer_is_an_error() {
        assert!(from_response(StatusCode::FORBIDDEN, "", &version("0.1.0")).is_err());
        assert!(from_response(StatusCode::OK, "<html>", &version("0.1.0")).is_err());
    }

    /// Talks to GitHub, so it only runs on demand: `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn latest_release_answers_from_github() {
        let http = reqwest::Client::new();
        // Any published release is newer than 0.0.1, and no release is a 404:
        // both are valid answers.
        assert!(check(&http, &version("0.0.1")).await.is_ok());
    }
}
