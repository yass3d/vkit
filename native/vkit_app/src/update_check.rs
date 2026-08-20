use std::sync::OnceLock;

pub const RELEASES_PAGE: &str = "https://github.com/yass3d/vkit/releases";

const HOST: &str = "api.github.com";

pub const FAKE_UPDATE_VARIABLE: &str = "VKIT_FAKE_UPDATE";

const PATH: &str = "/repos/yass3d/vkit/releases?per_page=20";

const USER_AGENT: &str = concat!("Vkit/", env!("CARGO_PKG_VERSION"));

const BODY_CEILING: usize = 64 * 1024;

const TIMEOUT_MILLIS: i32 = 5_000;

static NEWER_RELEASE: OnceLock<String> = OnceLock::new();

fn summoned_badge() -> Option<String> {
    #[cfg(debug_assertions)]
    {
        let asked = std::env::var(FAKE_UPDATE_VARIABLE).unwrap_or_default();
        let tag = asked.trim();
        if !tag.is_empty() && !tag.eq_ignore_ascii_case("off") {
            return Some(tag.to_owned());
        }
    }
    None
}

pub fn start(context: &egui::Context) {
    if let Some(tag) = summoned_badge() {
        let _ = NEWER_RELEASE.set(tag);
        return;
    }
    let context = context.clone();
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

pub fn newer_release() -> Option<&'static str> {
    NEWER_RELEASE.get().map(String::as_str)
}

pub fn is_newer_than_running(tag: &str) -> bool {
    let Some(theirs) = version_of(tag) else {
        return false;
    };
    let Some(ours) = version_of(env!("CARGO_PKG_VERSION")) else {
        return false;
    };
    theirs > ours
}

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
    None
}

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

    struct Handle(*mut c_void);

    impl Drop for Handle {
        fn drop(&mut self) {
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

        assert_eq!(
            newest_tag_in(
                r#"[{"tag_name":"v0.1.0","draft":false},{"tag_name":"v0.2.0","draft":false}]"#
            ),
            Some("v0.2.0".to_owned())
        );

        assert_eq!(
            newest_tag_in(
                r#"[{"tag_name":"v9.9.9","draft":true},{"tag_name":"v0.1.0","draft":false}]"#
            ),
            Some("v0.1.0".to_owned()),
            "a draft must never be announced"
        );

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

    #[test]
    fn the_summoned_badge_stays_away_until_it_is_asked_for() {
        assert!(
            std::env::var(FAKE_UPDATE_VARIABLE).is_err(),
            "this test describes the unset case; the variable is set in this environment"
        );
        assert!(
            summoned_badge().is_none(),
            "a build nobody asked must not claim an update: the badge was a visual              check, and a shipped screenshot of it would be a lie"
        );
    }

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
