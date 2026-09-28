//! The `mg` release layout the extension downloads (plan 4, Z3/R): which
//! asset a platform needs, how it is packed, and where its binary lands.
//! The asset names themselves are listed once, in `release-assets.txt`,
//! which the release job publishes from and the tests below check.

use zed_extension_api::{Architecture, DownloadedFileType, Os};

/// The GitHub repository whose releases carry `mg`.
pub const REPO: &str = "MikeRomaa/metaglyph";

/// The name of the release asset for a platform, or `None` for one no
/// release is built for (32-bit x86).
pub fn asset_name(os: Os, arch: Architecture) -> Option<String> {
    let os = match os {
        Os::Linux => "linux",
        Os::Mac => "macos",
        Os::Windows => "windows",
    };
    let arch = match arch {
        Architecture::X8664 => "x86_64",
        Architecture::Aarch64 => "aarch64",
        Architecture::X86 => return None,
    };
    let extension = if os == "windows" { "zip" } else { "tar.gz" };
    Some(format!("mg-{os}-{arch}.{extension}"))
}

/// How the asset for `os` is packed.
pub fn file_type(os: Os) -> DownloadedFileType {
    match os {
        Os::Windows => DownloadedFileType::Zip,
        Os::Linux | Os::Mac => DownloadedFileType::GzipTar,
    }
}

/// The binary's name inside the archive.
pub fn binary_name(os: Os) -> &'static str {
    match os {
        Os::Windows => "mg.exe",
        Os::Linux | Os::Mac => "mg",
    }
}

/// The directory, relative to the extension's working directory, that
/// release `version` is extracted into. Every version gets its own, so
/// an upgrade never overwrites a binary Zed may still be running.
pub fn version_dir(version: &str) -> String {
    format!("mg-{version}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const OSES: [Os; 3] = [Os::Linux, Os::Mac, Os::Windows];
    const ARCHES: [Architecture; 3] = [
        Architecture::X8664,
        Architecture::Aarch64,
        Architecture::X86,
    ];

    /// Every platform with a release maps to a listed asset, and every
    /// listed asset belongs to a platform.
    #[test]
    fn asset_names_match_the_published_list() {
        let listed: Vec<&str> = include_str!("../release-assets.txt")
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect();
        let mut named: Vec<String> = OSES
            .iter()
            .flat_map(|&os| ARCHES.iter().filter_map(move |&arch| asset_name(os, arch)))
            .collect();
        named.sort();
        let mut listed_sorted: Vec<String> = listed.iter().map(|s| s.to_string()).collect();
        listed_sorted.sort();
        assert_eq!(named, listed_sorted);
    }

    #[test]
    fn packing_and_binary_follow_the_os() {
        assert_eq!(
            asset_name(Os::Mac, Architecture::Aarch64).unwrap(),
            "mg-macos-aarch64.tar.gz"
        );
        assert!(matches!(file_type(Os::Windows), DownloadedFileType::Zip));
        assert!(matches!(file_type(Os::Linux), DownloadedFileType::GzipTar));
        assert_eq!(binary_name(Os::Windows), "mg.exe");
        assert_eq!(binary_name(Os::Mac), "mg");
        assert_eq!(asset_name(Os::Linux, Architecture::X86), None);
        assert_eq!(version_dir("v0.2.0"), "mg-v0.2.0");
    }
}
