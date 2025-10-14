/// Compares two semantic version strings and returns true if the second is newer than the first.
/// Supports version suffixes like rc, prerelease, alpha, beta, etc.
///
/// # Examples
/// ```
/// assert!(is_version_newer("1.2.3", "1.2.2"));
/// assert!(is_version_newer("1.3.0", "1.2.9"));
/// assert!(is_version_newer("2.0.0", "1.9.9"));
/// assert!(is_version_newer("1.2.3", "1.2.3-rc1"));
/// assert!(is_version_newer("1.2.3", "1.2.3-prerelease"));
/// ```
pub fn is_version_newer(older_version: &str, newer_version: &str) -> bool {
    let newer = Version::from_str(newer_version);
    let older = Version::from_str(older_version);

    // Compare major, minor, patch
    match newer.major.cmp(&older.major) {
        std::cmp::Ordering::Greater => return true,
        std::cmp::Ordering::Less => return false,
        std::cmp::Ordering::Equal => {}
    }

    match newer.minor.cmp(&older.minor) {
        std::cmp::Ordering::Greater => return true,
        std::cmp::Ordering::Less => return false,
        std::cmp::Ordering::Equal => {}
    }

    match newer.patch.cmp(&older.patch) {
        std::cmp::Ordering::Greater => return true,
        std::cmp::Ordering::Less => return false,
        std::cmp::Ordering::Equal => {}
    }

    // If major.minor.patch are equal, compare suffixes
    compare_suffixes(&newer.suffix, &older.suffix)
}

#[derive(Debug, PartialEq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub suffix: Option<String>,
}
impl Version {
    pub fn from_str(version: &str) -> Self {
        // Remove 'v' prefix if present
        let version = version.strip_prefix('v').unwrap_or(version);

        // Split on '-' to separate version from suffix
        let parts: Vec<&str> = version.splitn(2, '-').collect();
        let version_part = parts[0];
        let suffix = if parts.len() > 1 {
            Some(parts[1].to_string())
        } else {
            None
        };

        // Parse major.minor.patch
        let numbers: Vec<&str> = version_part.split('.').collect();
        #[allow(clippy::get_first)]
        let major = numbers.get(0).and_then(|s| s.parse().ok()).unwrap_or(0);
        let minor = numbers.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        let patch = numbers.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);

        Version {
            major,
            minor,
            patch,
            suffix,
        }
    }
}

fn compare_suffixes(suffix1: &Option<String>, suffix2: &Option<String>) -> bool {
    match (suffix1, suffix2) {
        // If both have no suffix, they're equal (not newer)
        (None, None) => false,
        // If only the first has a suffix, it's older (not newer)
        (Some(_), None) => false,
        // If only the second has a suffix, the first is newer
        (None, Some(_)) => true,
        // If both have suffixes, compare them
        (Some(s1), Some(s2)) => compare_suffix_strings(s1, s2),
    }
}

fn compare_suffix_strings(suffix1: &str, suffix2: &str) -> bool {
    // For simple suffixes like "rc1", "alpha2", etc., we can compare them directly
    // by trying to extract the numeric part and compare that
    let (prefix1, num1) = extract_suffix_parts(suffix1);
    let (prefix2, num2) = extract_suffix_parts(suffix2);

    // First compare the prefix (alpha, beta, rc, etc.)
    let prefix_comparison = compare_suffix_prefix(&prefix1, &prefix2);
    if prefix_comparison != std::cmp::Ordering::Equal {
        return prefix_comparison == std::cmp::Ordering::Greater;
    }

    // If prefixes are equal, compare the numeric parts
    num1.cmp(&num2) == std::cmp::Ordering::Greater
}

fn extract_suffix_parts(suffix: &str) -> (String, u32) {
    // Try to find where the numeric part starts
    let mut numeric_start = suffix.len();
    for (i, c) in suffix.char_indices() {
        if c.is_ascii_digit() {
            numeric_start = i;
            break;
        }
    }

    let prefix = suffix[..numeric_start].to_string();
    let numeric_part = if numeric_start < suffix.len() {
        suffix[numeric_start..].parse().unwrap_or(0)
    } else {
        0
    };

    (prefix, numeric_part)
}

fn compare_suffix_prefix(prefix1: &str, prefix2: &str) -> std::cmp::Ordering {
    let priority1 = get_suffix_priority(prefix1);
    let priority2 = get_suffix_priority(prefix2);

    match priority1.cmp(&priority2) {
        std::cmp::Ordering::Equal => prefix1.cmp(prefix2),
        other => other,
    }
}

fn get_suffix_priority(part: &str) -> u8 {
    match part.to_lowercase().as_str() {
        "alpha" => 1,
        "beta" => 2,
        "rc" => 3,
        "pre" | "prerelease" => 4,
        _ => 5, // Other suffixes get higher priority
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_parsing() {
        assert_eq!(
            Version::from_str("1.2.3"),
            Version {
                major: 1,
                minor: 2,
                patch: 3,
                suffix: None,
            }
        );

        assert_eq!(
            Version::from_str("v1.2.3"),
            Version {
                major: 1,
                minor: 2,
                patch: 3,
                suffix: None,
            }
        );

        assert_eq!(
            Version::from_str("1.2.3-rc1"),
            Version {
                major: 1,
                minor: 2,
                patch: 3,
                suffix: Some("rc1".to_string()),
            }
        );

        assert_eq!(
            Version::from_str("2.0.0-prerelease"),
            Version {
                major: 2,
                minor: 0,
                patch: 0,
                suffix: Some("prerelease".to_string()),
            }
        );
    }

    #[test]
    fn test_version_comparison_basic() {
        // Basic version comparison
        assert!(is_version_newer("1.2.3", "1.2.2"));
        assert!(is_version_newer("1.3.0", "1.2.9"));
        assert!(is_version_newer("2.0.0", "1.9.9"));
        assert!(!is_version_newer("1.2.2", "1.2.3"));
        assert!(!is_version_newer("1.2.9", "1.3.0"));
        assert!(!is_version_newer("1.9.9", "2.0.0"));

        // Equal versions
        assert!(!is_version_newer("1.2.3", "1.2.3"));
    }

    #[test]
    fn test_version_comparison_with_suffixes() {
        // Version with suffix vs version without suffix
        assert!(!is_version_newer("1.2.3-rc1", "1.2.3"));
        assert!(is_version_newer("1.2.3", "1.2.3-rc1"));

        // Different suffix types
        assert!(is_version_newer("1.2.3", "1.2.3-alpha"));
        assert!(is_version_newer("1.2.3", "1.2.3-beta"));
        assert!(is_version_newer("1.2.3", "1.2.3-rc1"));
        assert!(is_version_newer("1.2.3", "1.2.3-prerelease"));

        // Same suffix type, different numbers
        assert!(is_version_newer("1.2.3-rc2", "1.2.3-rc1"));
        assert!(!is_version_newer("1.2.3-rc1", "1.2.3-rc2"));

        // Different suffix types with same version
        assert!(is_version_newer("1.2.3-beta", "1.2.3-alpha"));
        assert!(is_version_newer("1.2.3-rc1", "1.2.3-beta"));
        assert!(is_version_newer("1.2.3", "1.2.3-rc1"));

        // Numeric suffixes
        assert!(is_version_newer("1.2.3-alpha2", "1.2.3-alpha1"));
        assert!(is_version_newer("1.2.3-beta5", "1.2.3-beta3"));
        assert!(is_version_newer("1.2.3-rc10", "1.2.3-rc2"));

        // Suffixes without numbers
        assert!(is_version_newer("1.2.3-beta", "1.2.3-alpha"));
        assert!(is_version_newer("1.2.3-rc", "1.2.3-beta"));
        assert!(is_version_newer("1.2.3", "1.2.3-rc"));

        // Mixed numeric and non-numeric
        assert!(is_version_newer("1.2.3-rc1", "1.2.3-beta"));
        assert!(is_version_newer("1.2.3-beta2", "1.2.3-alpha"));
    }

    #[test]
    fn test_version_comparison_edge_cases() {
        // Missing parts (should default to 0)
        assert!(is_version_newer("1.2", "1.1.9"));
        assert!(is_version_newer("1", "0.9.9"));
        assert!(is_version_newer("1.2.3", "1.2"));
        assert!(is_version_newer("1.2.3", "1"));

        // v prefix
        assert!(is_version_newer("v1.2.3", "1.2.2"));
        assert!(is_version_newer("1.2.3", "v1.2.2"));
        assert!(!is_version_newer("v1.2.3", "v1.2.3"));

        // Empty or invalid versions
        assert!(is_version_newer("1.0.0", ""));
        assert!(is_version_newer("1.0.0", "invalid"));

        // Same suffix
        assert!(!is_version_newer("1.2.3-rc1", "1.2.3-rc1"));
    }

    #[test]
    fn test_version_comparison_real_world_examples() {
        // Real-world version comparison examples
        assert!(is_version_newer("2.1.0", "2.0.9"));
        assert!(is_version_newer("2.1.0", "2.0.9-rc1"));
        assert!(is_version_newer("2.1.0-rc1", "2.0.9"));
        assert!(is_version_newer("2.1.0-rc2", "2.1.0-rc1"));

        // GitHub release examples
        assert!(is_version_newer("v1.0.0", "v0.9.9"));
        assert!(is_version_newer("v1.0.0", "v1.0.0-rc1"));
        assert!(is_version_newer("v1.0.0-beta2", "v1.0.0-beta1"));
        assert!(is_version_newer("v1.0.0-rc1", "v1.0.0-beta2"));
    }
}
