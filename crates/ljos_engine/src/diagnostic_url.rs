/// A URL for diagnostics only. Requests must keep using the original URL.
/// Queries and fragments can carry credentials whose names are not known here.
pub fn diagnostic_url(value: &str) -> String {
    match url::Url::parse(value) {
        Ok(mut parsed) if matches!(parsed.scheme(), "http" | "https") => {
            let _ = parsed.set_username("");
            let _ = parsed.set_password(None);
            parsed.set_query(None);
            parsed.set_fragment(None);
            parsed.into()
        }
        Ok(_) => value.to_owned(),
        Err(_)
            if value.trim_start().to_ascii_lowercase().starts_with("http:")
                || value
                    .trim_start()
                    .to_ascii_lowercase()
                    .starts_with("https:") =>
        {
            "[invalid HTTP URL]".to_owned()
        }
        Err(_) => value.to_owned(),
    }
}

#[cfg(test)]
mod diagnostic_tests {
    use super::diagnostic_url;

    #[test]
    fn credentials_are_removed_without_losing_the_resource_path() {
        for input in [
            "https://user:password@example.com:8443/images/file.png?token=secret#fragment",
            "HTTPS://us%65r:pass%77ord@example.com:8443/images/file.png?token=secret#fragment",
        ] {
            assert_eq!(
                diagnostic_url(input),
                "https://example.com:8443/images/file.png"
            );
        }
        assert_eq!(
            diagnostic_url("https://[::1]:8443/file?q=secret"),
            "https://[::1]:8443/file"
        );
        assert_eq!(diagnostic_url("images/file.png"), "images/file.png");
        assert_eq!(
            diagnostic_url("https://user:password@[bad/?secret"),
            "[invalid HTTP URL]"
        );
    }
}
