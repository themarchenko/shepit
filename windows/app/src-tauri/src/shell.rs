//! Opening Windows pages from the app. There is no generic opener: the one page Shepit links to
//! is the microphone privacy page, so no argument can reach `ShellExecuteW`.

/// The Settings page the microphone guidance points at.
#[cfg_attr(not(windows), allow(dead_code))]
pub const MIC_PRIVACY_URI: &str = "ms-settings:privacy-microphone";

/// Where Shepit releases live.
#[cfg_attr(not(windows), allow(dead_code))]
pub const RELEASES_URL: &str = "https://github.com/themarchenko/shepit/releases";

/// The page of one release, or the latest releases page. A version that is not digits and dots
/// (`0.4.0/../x`, `0.4.0-rc1`, empty) gets the latest page, so no feed text reaches the URL.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn release_page_url(version: Option<&str>) -> String {
    todo!()
}

/// Opens the Windows microphone privacy page.
#[cfg(windows)]
pub fn open_microphone_privacy() -> Result<(), String> {
    use windows::core::{w, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let uri: Vec<u16> = MIC_PRIVACY_URI
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: `uri` is a NUL-terminated UTF-16 string that outlives the call, and the other string
    // arguments are null, which ShellExecuteW allows. No window handle is passed.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(uri.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW returns a value above 32 on success and an error code up to 32 otherwise.
    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err("Could not open the Windows privacy settings".to_string())
    }
}

/// There is no Windows privacy page elsewhere; the button does nothing.
#[cfg(not(windows))]
pub fn open_microphone_privacy() -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LATEST: &str = "https://github.com/themarchenko/shepit/releases/latest";

    #[test]
    fn a_plain_version_gets_its_release_page() {
        assert_eq!(
            release_page_url(Some("0.4.0")),
            "https://github.com/themarchenko/shepit/releases/tag/v0.4.0"
        );
    }

    #[test]
    fn anything_but_digits_and_dots_gets_the_latest_page() {
        for bad in [
            "0.4.0-rc1",
            "0.4.0/../x",
            "",
            "v0.4.0",
            "0.4.0?x=1",
            "0.4.0 ",
            "0,4",
        ] {
            assert_eq!(release_page_url(Some(bad)), LATEST, "{bad:?}");
        }
    }

    #[test]
    fn no_version_gets_the_latest_page() {
        assert_eq!(release_page_url(None), LATEST);
    }

    #[test]
    fn the_privacy_uri_is_the_microphone_page() {
        assert_eq!(MIC_PRIVACY_URI, "ms-settings:privacy-microphone");
    }

    #[test]
    fn the_privacy_uri_is_a_settings_uri_with_no_spaces_or_query() {
        assert!(MIC_PRIVACY_URI.starts_with("ms-settings:"));
        assert!(!MIC_PRIVACY_URI.contains([' ', '?', '&', '/']));
    }
}
