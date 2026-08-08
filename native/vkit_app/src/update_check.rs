//! Asks GitHub once per launch whether a newer release exists.
//!
//! Every failure is silence. No internet, a proxy that refuses, a rate limit, a
//! response that is not what we expected — none of it is the user's problem, and
//! none of it produces a dialog, a log line at warning level, or a stuck
//! spinner. The only outcome that reaches the interface is "a newer version
//! exists", and even that is a capsule the user is free to ignore.
//!
//! This is deliberately not a self-updater. The program is one executable and
//! its caches are already built locally; an in-place updater would be heavier
//! than the download it replaces, and far more ways to go wrong while the
//! internals are still moving.

use std::sync::OnceLock;

/// Where the capsule sends someone who clicks it.
///
/// `concat!` will not take a constant, so this cannot be built from
/// `REPOSITORY_URL` at compile time. A test ties the two together instead — the
/// page the capsule opens, the repository the rest of the program names, and
/// the path the check asks about all have to be one repository.
pub const RELEASES_PAGE: &str = "https://github.com/yass3d/vkit/releases";

const HOST: &str = "api.github.com";

/// The release *list*, not `/releases/latest`.
///
/// `/releases/latest` excludes pre-releases, and this project publishes
/// pre-releases — so that endpoint answers 404 and the check would have sat
/// there reporting nothing, indistinguishable from working correctly, until the
/// first stable tag. The list returns everything and lets us decide.
const PATH: &str = "/repos/yass3d/vkit/releases?per_page=20";

/// GitHub rejects a request with no user agent, so this is not decoration.
const USER_AGENT: &str = concat!("Vkit/", env!("CARGO_PKG_VERSION"));

/// A hostile or broken endpoint should not be able to grow the process. The
/// real body is a few kilobytes.
const BODY_CEILING: usize = 64 * 1024;

/// A launch check that no one is waiting for has no business holding a thread
/// open on a network that never answers.
const TIMEOUT_MILLIS: i32 = 5_000;

/// The answer, once there is one. Write-once for the life of the process,
/// which is exactly what a launch check produces — so there is nothing to poll,
/// nothing to thread through state, and nothing that can be answered twice.
static NEWER_RELEASE: OnceLock<String> = OnceLock::new();

/// Starts the check on its own thread. Never blocks the caller and never
/// delays the window appearing.
///
/// The context is here only so the frame that learns the answer is actually
/// drawn: on an idle window nothing else would ask for a repaint, and the
/// capsule would wait for the next stray mouse move to appear.
pub fn start(context: &egui::Context) {
    let context = context.clone();
    // A thread that will not start is one more thing to say nothing about.
    let _ = std::thread::Builder::new()
        .name("vkit-update-check".to_owned())
        .spawn(move || {
            let Some(tag) = latest_release_tag() else {
                return;
            };
            if is_newer_than_running(&tag) && NEWER_RELEASE.set(tag).is_ok() {
                context.request_repaint();
            }
        });
}

/// The newer version's tag, if the check found one. A plain load — safe to ask
/// every frame.
pub fn newer_release() -> Option<&'static str> {
    NEWER_RELEASE.get().map(String::as_str)
}

/// Compares a release tag against the version this binary was built as.
///
/// Only a strictly greater version counts. Equal is not an update, and neither
/// is older — a repository whose latest release is behind a locally built
/// binary must not nag its own author.
pub fn is_newer_than_running(tag: &str) -> bool {
    let Some(theirs) = version_of(tag) else {
        return false;
    };
    let Some(ours) = version_of(env!("CARGO_PKG_VERSION")) else {
        return false;
    };
    theirs > ours
}

/// Reads `v1.2.3`, `1.2.3`, `V1.2`, `1` and stops at the first thing that is
/// not a number, so `0.0.2-beta` reads as `0.0.2`.
///
/// The components are numbers, not text. `0.0.10` is newer than `0.0.9`, which
/// a string comparison would get backwards — and would keep getting backwards
/// for exactly one release out of every ten.
fn version_of(tag: &str) -> Option<[u64; 3]> {
    let digits = tag.trim().trim_start_matches(['v', 'V']);
    let mut version = [0_u64; 3];
    let mut seen = 0;
    for (slot, part) in version.iter_mut().zip(digits.split('.')) {
        let numeric: String = part.chars().take_while(char::is_ascii_digit).collect();
        if numeric.is_empty() {
            break;
        }
        *slot = numeric.parse().ok()?;
        seen += 1;
    }
    (seen > 0).then_some(version)
}

/// The highest version among the published releases, and nothing else.
///
/// Drafts are skipped: they are invisible to anyone who would click through, so
/// announcing one would send a user to a page that does not exist for them.
/// Pre-releases count, because at this stage they are what actually ships.
///
/// Picks the highest version rather than the first entry. GitHub happens to
/// return these newest-first, but publishing an old tag later would put it at
/// the front, and a downgrade is not an update.
fn newest_tag_in(body: &str) -> Option<String> {
    let releases: serde_json::Value = serde_json::from_str(body).ok()?;
    releases
        .as_array()?
        .iter()
        .filter(|release| release.get("draft").and_then(serde_json::Value::as_bool) != Some(true))
        .filter_map(|release| release.get("tag_name")?.as_str())
        .filter(|tag| !tag.is_empty())
        .max_by_key(|tag| version_of(tag))
        .map(str::to_owned)
}

#[cfg(windows)]
fn latest_release_tag() -> Option<String> {
    newest_tag_in(&windows_http::get(HOST, PATH)?)
}

#[cfg(not(windows))]
fn latest_release_tag() -> Option<String> {
    // The shipping build is Windows only. Elsewhere the check simply never
    // finds anything, which is the same silence every other failure produces.
    None
}

/// One HTTPS GET, through the operating system's own stack.
///
/// WinHTTP rather than a bundled client: it is already on every machine that
/// can run this, so it costs the executable nothing, and it brings the system
/// proxy configuration and the system certificate store with it. A vendored
/// TLS stack would add megabytes to a binary that is watching its size, and
/// would then have to be kept current against certificate roots by hand.
#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "isolated WinHTTP request, no callbacks or state"
)]
mod windows_http {
    use std::ffi::c_void;

    use super::{BODY_CEILING, TIMEOUT_MILLIS, USER_AGENT};

    use windows_sys::Win32::Foundation::FALSE;
    use windows_sys::Win32::Networking::WinHttp::{
        INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
        WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect,
        WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders, WinHttpReadData,
        WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
    };

    /// Closes on the way out of every path, including the early returns that
    /// make up most of this module.
    struct Handle(*mut c_void);

    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: the handle came from WinHttp and is closed exactly once,
            // because only Drop closes it and Handle is not Copy or Clone.
            unsafe { WinHttpCloseHandle(self.0) };
        }
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub(super) fn get(host: &str, path: &str) -> Option<String> {
        let agent = wide(USER_AGENT);
        let host_wide = wide(host);
        let path_wide = wide(path);
        let method = wide("GET");

        // SAFETY: every pointer below is either null where the API documents
        // null as meaningful, or a NUL-terminated wide buffer that outlives the
        // call that reads it. Each handle is checked against null before use and
        // owned by a Handle, so no path leaks one and none is closed twice.
        unsafe {
            let session = Handle(WinHttpOpen(
                agent.as_ptr(),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                std::ptr::null(),
                std::ptr::null(),
                0,
            ));
            if session.0.is_null() {
                return None;
            }
            // Without these a network that accepts the connection and then goes
            // quiet would hold this thread for the life of the process.
            WinHttpSetTimeouts(
                session.0,
                TIMEOUT_MILLIS,
                TIMEOUT_MILLIS,
                TIMEOUT_MILLIS,
                TIMEOUT_MILLIS,
            );

            let connection = Handle(WinHttpConnect(
                session.0,
                host_wide.as_ptr(),
                INTERNET_DEFAULT_HTTPS_PORT,
                0,
            ));
            if connection.0.is_null() {
                return None;
            }

            let request = Handle(WinHttpOpenRequest(
                connection.0,
                method.as_ptr(),
                path_wide.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                WINHTTP_FLAG_SECURE,
            ));
            if request.0.is_null() {
                return None;
            }

            if WinHttpSendRequest(request.0, std::ptr::null(), 0, std::ptr::null(), 0, 0, 0)
                == FALSE
                || WinHttpReceiveResponse(request.0, std::ptr::null_mut()) == FALSE
            {
                return None;
            }

            // A rate limit answers with a perfectly well formed body that is not
            // a release, so the status is checked before the body is trusted.
            let mut status: u32 = 0;
            let mut status_size = u32::try_from(size_of::<u32>()).ok()?;
            if WinHttpQueryHeaders(
                request.0,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                std::ptr::null(),
                std::ptr::from_mut(&mut status).cast(),
                &raw mut status_size,
                std::ptr::null_mut(),
            ) == FALSE
                || status != 200
            {
                return None;
            }

            let mut body = Vec::new();
            let mut chunk = [0_u8; 4096];
            loop {
                let mut read: u32 = 0;
                if WinHttpReadData(
                    request.0,
                    chunk.as_mut_ptr().cast(),
                    u32::try_from(chunk.len()).ok()?,
                    &raw mut read,
                ) == FALSE
                {
                    return None;
                }
                if read == 0 {
                    break;
                }
                let read = usize::try_from(read).ok()?;
                if body.len() + read > BODY_CEILING {
                    return None;
                }
                body.extend_from_slice(&chunk[..read]);
            }
            String::from_utf8(body).ok()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_reads_as_numbers_and_not_as_text() {
        assert_eq!(version_of("v1.2.3"), Some([1, 2, 3]));
        assert_eq!(version_of("1.2.3"), Some([1, 2, 3]));
        assert_eq!(version_of("V1.2"), Some([1, 2, 0]));
        assert_eq!(version_of("1"), Some([1, 0, 0]));
        assert_eq!(version_of("0.0.2-beta.1"), Some([0, 0, 2]));

        // The trap this exists to avoid: as text, "0.0.10" sorts below "0.0.9",
        // so one release in every ten would go unannounced.
        assert!(version_of("0.0.10") > version_of("0.0.9"));
        assert!(version_of("0.1.0") > version_of("0.0.9"));
        assert!(version_of("1.0.0") > version_of("0.9.9"));

        for refused in ["", "latest", "v", "nightly", "  "] {
            assert_eq!(version_of(refused), None, "{refused:?} is not a version");
        }
    }

    #[test]
    fn only_a_strictly_newer_release_is_worth_saying_anything_about() {
        let running = env!("CARGO_PKG_VERSION");
        assert!(
            !is_newer_than_running(running),
            "we are not behind ourselves"
        );
        assert!(!is_newer_than_running(&format!("v{running}")));

        let [major, minor, patch] = version_of(running).expect("our own version must parse");
        assert!(is_newer_than_running(&format!(
            "v{major}.{minor}.{}",
            patch + 1
        )));
        assert!(is_newer_than_running(&format!("{major}.{}.0", minor + 1)));
        assert!(is_newer_than_running(&format!("{}.0.0", major + 1)));

        // A repository whose latest release trails a locally built binary must
        // not nag its own author.
        if patch > 0 {
            assert!(!is_newer_than_running(&format!(
                "v{major}.{minor}.{}",
                patch - 1
            )));
        }

        for noise in ["", "{}", "not a tag", "v", "😀"] {
            assert!(!is_newer_than_running(noise), "{noise:?} means silence");
        }
    }

    #[test]
    fn a_response_that_is_not_a_release_produces_nothing() {
        assert_eq!(
            newest_tag_in(r#"[{"tag_name":"v0.9.1","draft":false,"prerelease":true}]"#),
            Some("v0.9.1".to_owned()),
            "a pre-release is what this project actually ships"
        );

        // Newest-first is GitHub's habit, not a promise, and publishing an old
        // tag later would put it at the front. A downgrade is not an update.
        assert_eq!(
            newest_tag_in(
                r#"[{"tag_name":"v0.1.0","draft":false},{"tag_name":"v0.2.0","draft":false}]"#
            ),
            Some("v0.2.0".to_owned())
        );

        // A draft is invisible to whoever would click through, so announcing it
        // would send them to a page that does not exist for them.
        assert_eq!(
            newest_tag_in(
                r#"[{"tag_name":"v9.9.9","draft":true},{"tag_name":"v0.1.0","draft":false}]"#
            ),
            Some("v0.1.0".to_owned()),
            "a draft must never be announced"
        );

        // What a rate limit, a private repository, and a broken proxy actually
        // send back. None of them is a release, and none may be read as one.
        for refused in [
            r#"{"message":"API rate limit exceeded","documentation_url":"..."}"#,
            r#"[{"tag_name":null}]"#,
            r#"[{"tag_name":""}]"#,
            r#"[{"tag_name":123}]"#,
            "[]",
            "<html><body>proxy says no</body></html>",
            "",
        ] {
            assert_eq!(newest_tag_in(refused), None, "{refused:?} is not a release");
        }
    }

    /// Reaches the real GitHub, so it is not part of an ordinary run. Everything
    /// above tests the parsing; only this tests that the request itself works,
    /// which no amount of compiling will tell you.
    ///
    /// `cargo test -p vkit-app --bin Vkit -- --ignored the_request_actually`
    #[test]
    #[ignore = "hits the network"]
    fn the_request_actually_reaches_github_and_comes_back_with_a_release() {
        let tag = latest_release_tag().expect("the release endpoint answered with a tag");
        assert!(
            version_of(&tag).is_some(),
            "{tag:?} came back but does not read as a version"
        );
        println!(
            "latest release: {tag}, running {}",
            env!("CARGO_PKG_VERSION")
        );
    }

    #[test]
    fn the_page_the_capsule_opens_is_the_repository_the_check_asked_about() {
        // Three places name the repository and none of them can derive from the
        // others at compile time, so a rename would quietly leave the capsule
        // opening one project while the check reports on another. That is the
        // kind of break nobody notices until someone clicks.
        let owner_and_repo = PATH
            .strip_prefix("/repos/")
            .and_then(|rest| rest.split_once("/releases"))
            .map(|(owner_and_repo, _)| owner_and_repo)
            .expect("the API path names an owner and a repository");

        assert_eq!(
            RELEASES_PAGE,
            format!("{}/releases", crate::REPOSITORY_URL),
            "the capsule has drifted from the repository the program names"
        );
        assert_eq!(
            crate::REPOSITORY_URL,
            format!("https://github.com/{owner_and_repo}"),
            "the check asks about {owner_and_repo}, which is not where the capsule goes"
        );
        assert_eq!(HOST, "api.github.com");
    }
}
