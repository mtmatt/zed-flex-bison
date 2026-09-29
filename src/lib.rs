use zed_extension_api::{self as zed, settings::LspSettings, LanguageServerId, Result};

const SERVER_BINARY: &str = "flex-bison-lsp";

struct FlexBisonExtension;

impl zed::Extension for FlexBisonExtension {
    fn new() -> Self {
        Self
    }

    /// Runs `lsp.flex-bison-lsp.binary.path` from the settings if set, and
    /// otherwise `flex-bison-lsp` from the worktree's `PATH`.
    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let binary = LspSettings::for_worktree(language_server_id.as_ref(), worktree)
            .ok()
            .and_then(|s| s.binary);
        let (path, arguments, env) = match binary {
            Some(b) => (b.path, b.arguments, b.env),
            None => (None, None, None),
        };
        let command = path
            .or_else(|| worktree.which(SERVER_BINARY))
            .ok_or_else(|| {
                format!(
                    "`{SERVER_BINARY}` was not found on PATH. Install it with \
                     `cargo install --path server` from the extension repository, \
                     or set `lsp.{SERVER_BINARY}.binary.path` in your settings."
                )
            })?;
        Ok(zed::Command {
            command,
            args: arguments.unwrap_or_default(),
            env: env.unwrap_or_default().into_iter().collect(),
        })
    }
}

zed::register_extension!(FlexBisonExtension);
