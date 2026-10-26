//! Dotted version comparison. Mirrors `AppVersion.isNewer` in the Swift core.

/// True when `candidate` is a newer version than `installed`.
pub fn is_newer(candidate: &str, installed: &str) -> bool {
    let x = numbers(candidate);
    let y = numbers(installed);
    for i in 0..x.len().max(y.len()) {
        let l = x.get(i).copied().unwrap_or(0);
        let r = y.get(i).copied().unwrap_or(0);
        if l != r {
            return l > r;
        }
    }
    false
}

// i64 matches Swift's Int, so a sign or an overflow parses the same way on both platforms.
fn numbers(version: &str) -> Vec<i64> {
    version
        .trim_matches(|c| c == 'v' || c == 'V')
        .split('.')
        .filter(|piece| !piece.is_empty())
        .map(|piece| piece.parse().unwrap_or(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn empty_segments_are_skipped() {
        // Swift `split` drops empty pieces, so "1..2" reads as [1, 2].
        assert!(!is_newer("1..2", "1.2"));
        assert!(!is_newer("1.2", "1..2"));
    }

    #[test]
    fn leading_and_trailing_v_are_trimmed() {
        assert!(is_newer("V1.0v", "0.9"));
    }

    #[test]
    fn non_numeric_segment_reads_as_zero() {
        assert!(!is_newer("abc", "0"));
        assert!(!is_newer("1.x", "1.0"));
        assert!(!is_newer("1.0", "1.x"));
    }

    #[test]
    fn overflowing_segment_reads_as_zero() {
        assert!(!is_newer("99999999999999999999", "1"));
    }

    #[test]
    fn missing_segments_read_as_zero() {
        assert!(is_newer("1.0.1", "1.0"));
        assert!(!is_newer("1.0", "1.0.0"));
    }
}
