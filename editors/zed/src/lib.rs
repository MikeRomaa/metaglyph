//! The Metaglyph Zed extension (plan 4): syntax from the Tree-sitter
//! grammar, and the `mg lsp` language server, found in this order (Z3):
//!
//! 1. `lsp.metaglyph.binary.path` in Zed's settings, with its `arguments`
//!    (default `["lsp"]`) and `env`
//! 2. `mg` on the worktree's `PATH`
//! 3. the latest GitHub release, downloaded into the extension's working
//!    directory and kept there
//!
//! Platform detection goes through `zed::current_platform`: `cfg!` and
//! `std::env` describe the WebAssembly sandbox, not the user's machine.

mod release;

use std::fs;

use zed_extension_api::{self as zed, settings::LspSettings, LanguageServerId, Result};

/// The settings key naming a local build; every "no server" error names
/// it, so the fix is one search away.
const BINARY_SETTING: &str = "lsp.metaglyph.binary.path";

struct MetaglyphExtension {
    /// The downloaded binary, once found or fetched this session.
    cached_binary: Option<String>,
}

impl MetaglyphExtension {
    /// The downloaded `mg`: the cached path while it still exists, else the
    /// latest release's (fetching it when it isn't here yet). When GitHub
    /// can't be reached, an earlier download is used instead.
    fn downloaded_binary(&mut self, id: &LanguageServerId) -> Result<String> {
        if let Some(path) = &self.cached_binary {
            if fs::metadata(path).is_ok_and(|m| m.is_file()) {
                return Ok(path.clone());
            }
        }

        let (os, arch) = zed::current_platform();
        match self.fetch_latest(id, os, arch) {
            Ok(path) => {
                zed::set_language_server_installation_status(
                    id,
                    &zed::LanguageServerInstallationStatus::None,
                );
                self.cached_binary = Some(path.clone());
                Ok(path)
            }
            Err(err) => match existing_download(os) {
                Some(path) => {
                    self.cached_binary = Some(path.clone());
                    Ok(path)
                }
                None => {
                    let message = format!(
                        "no `mg` language server found. Set `{BINARY_SETTING}` in your Zed \
                         settings to a local build (such as `target/release/mg`), or put `mg` \
                         on your PATH. Downloading a release failed: {err}"
                    );
                    zed::set_language_server_installation_status(
                        id,
                        &zed::LanguageServerInstallationStatus::Failed(message.clone()),
                    );
                    Err(message)
                }
            },
        }
    }

    /// Downloads the latest release's binary unless it is already here,
    /// then removes every other version.
    fn fetch_latest(
        &self,
        id: &LanguageServerId,
        os: zed::Os,
        arch: zed::Architecture,
    ) -> Result<String> {
        zed::set_language_server_installation_status(
            id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );
        let latest = zed::latest_github_release(
            release::REPO,
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let asset_name = release::asset_name(os, arch)
            .ok_or_else(|| format!("no `mg` release is built for {os:?} on {arch:?}"))?;
        let asset = latest
            .assets
            .iter()
            .find(|a| a.name == asset_name)
            .ok_or_else(|| format!("release {} has no asset `{asset_name}`", latest.version))?;

        let dir = release::version_dir(&latest.version);
        let binary = format!("{dir}/{}", release::binary_name(os));
        if !fs::metadata(&binary).is_ok_and(|m| m.is_file()) {
            zed::set_language_server_installation_status(
                id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );
            zed::download_file(&asset.download_url, &dir, release::file_type(os))
                .map_err(|err| format!("downloading `{asset_name}`: {err}"))?;
            zed::make_file_executable(&binary)?;
        }

        remove_other_versions(&dir);
        Ok(binary)
    }
}

/// A previously downloaded binary of any version, for when the latest
/// release can't be checked.
fn existing_download(os: zed::Os) -> Option<String> {
    fs::read_dir(".")
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.starts_with(&release::version_dir("")))
        .map(|dir| format!("{dir}/{}", release::binary_name(os)))
        .find(|binary| fs::metadata(binary).is_ok_and(|m| m.is_file()))
}

/// Deletes every downloaded version but `keep`. Best effort: a version
/// that can't be removed now is tried again after the next download.
fn remove_other_versions(keep: &str) {
    let Ok(entries) = fs::read_dir(".") else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name != keep && name.starts_with(&release::version_dir("")) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

impl zed::Extension for MetaglyphExtension {
    fn new() -> Self {
        MetaglyphExtension {
            cached_binary: None,
        }
    }

    fn language_server_command(
        &mut self,
        id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let binary = LspSettings::for_worktree(id.as_ref(), worktree)
            .ok()
            .and_then(|settings| settings.binary);
        let args = binary
            .as_ref()
            .and_then(|b| b.arguments.clone())
            .unwrap_or_else(|| vec!["lsp".to_string()]);
        let mut env: Vec<(String, String)> = binary
            .as_ref()
            .and_then(|b| b.env.clone())
            .map(|env| env.into_iter().collect())
            .unwrap_or_default();
        env.sort();

        if let Some(path) = binary.and_then(|b| b.path) {
            return Ok(zed::Command {
                command: path,
                args,
                env,
            });
        }
        if let Some(path) = worktree.which("mg") {
            return Ok(zed::Command {
                command: path,
                args,
                env: worktree.shell_env(),
            });
        }
        Ok(zed::Command {
            command: self.downloaded_binary(id)?,
            args,
            env,
        })
    }

    /// Forwards `lsp.metaglyph.settings` (plan 4, Z3). It is meant to carry
    /// `fonts`, each font's ordered file list, which the server will read
    /// once the compiler can analyse a font spread over several files.
    fn language_server_workspace_configuration(
        &mut self,
        id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        Ok(LspSettings::for_worktree(id.as_ref(), worktree)
            .ok()
            .and_then(|settings| settings.settings))
    }
}

zed::register_extension!(MetaglyphExtension);
