//! microtun-signer CLI adapter. Every host-side signature goes through it.
//!
//! The signer URL defaults to [`LOCAL_SIGNER_URL`]: development keys in
//! microtun-signer's local store on this machine (`microtun-signer local`),
//! with no service and no sign-in. A service URL keeps the private keys on
//! the signing service instead. Either way the CLI signs a base64-encoded
//! 32-byte SHA-256 digest and returns a base64 signature; all results are
//! authenticated locally before use.
use anyhow::{Context, Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
};

pub use crate::{FIRMWARE_DEV_KEY_ID, FIRMWARE_KEY_ID_ENV, LOCAL_SIGNER_URL, is_local_signer_url};

/// One signing purpose: its development key in microtun-signer's local
/// store, and the variable that selects another key for this purpose only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DevKey {
    /// Default key ID with a `local:` signer URL.
    pub key_id: &'static str,
    /// Key ID override for this purpose; takes precedence over
    /// `MICROTUN_SIGNER_KEY_ID`, so one shell can name keys for several
    /// purposes at once.
    pub env: &'static str,
}

/// MCUboot application images (all boards): ECDSA P-256.
pub const FIRMWARE_DEV_KEY: DevKey = DevKey {
    key_id: FIRMWARE_DEV_KEY_ID,
    env: FIRMWARE_KEY_ID_ENV,
};

/// The microtun-signer executable (`MICROTUN_SIGNER` overrides it).
pub fn program() -> String {
    super::env("MICROTUN_SIGNER", "microtun-signer")
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signer {
    pub url: String,
    pub key_id: String,
    pub github_login: bool,
}

impl Signer {
    /// Resolve a signer for `purpose`. The URL defaults to `local:`. The key
    /// ID is the first of `key_id` (for example `--signer-key-id`), the
    /// purpose's variable (`purpose.env`), `MICROTUN_SIGNER_KEY_ID` and, for
    /// a local URL only, `purpose.key_id`. A service URL without a key ID is
    /// an error, never an invitation to use a development key ID remotely.
    pub fn new(
        url: Option<String>,
        key_id: Option<String>,
        github_login: bool,
        purpose: DevKey,
    ) -> Result<Self> {
        Self::resolve(
            url,
            [
                key_id,
                std::env::var(purpose.env).ok(),
                std::env::var("MICROTUN_SIGNER_KEY_ID").ok(),
            ],
            github_login,
            purpose,
        )
    }

    fn resolve(
        url: Option<String>,
        key_ids: [Option<String>; 3],
        github_login: bool,
        purpose: DevKey,
    ) -> Result<Self> {
        let url = non_empty(url).unwrap_or_else(|| LOCAL_SIGNER_URL.into());
        let key_id = match key_ids.into_iter().find_map(non_empty) {
            Some(key_id) => key_id,
            None if is_local_signer_url(&url) => purpose.key_id.into(),
            None => anyhow::bail!(
                "signer {url} is a service; set {} or MICROTUN_SIGNER_KEY_ID (or --signer-key-id) as well",
                purpose.env
            ),
        };
        Ok(Self {
            url,
            key_id,
            github_login,
        })
    }

    /// The signer for `purpose` from `MICROTUN_SIGNER_URL`, the key ID
    /// variables (see [`Signer::new`]) and `MICROTUN_SIGNER_GITHUB_LOGIN=1`.
    pub fn from_env(purpose: DevKey) -> Result<Self> {
        Self::new(
            std::env::var("MICROTUN_SIGNER_URL").ok(),
            None,
            super::flag("MICROTUN_SIGNER_GITHUB_LOGIN"),
            purpose,
        )
    }

    /// The local key `key_id`, ignoring any remote signer configuration. The
    /// store is the one `MICROTUN_SIGNER_URL` names when that is a `local:`
    /// URL, otherwise microtun-signer's default (`MICROTUN_SIGNER_LOCAL_DIR`
    /// or the user data directory). For test fixtures that must always sign
    /// with development keys.
    pub fn local(key_id: &str) -> Self {
        let url = non_empty(std::env::var("MICROTUN_SIGNER_URL").ok())
            .filter(|url| is_local_signer_url(url))
            .unwrap_or_else(|| LOCAL_SIGNER_URL.into());
        Self {
            url,
            key_id: key_id.into(),
            github_login: false,
        }
    }

    pub fn is_local(&self) -> bool {
        is_local_signer_url(&self.url)
    }

    fn command(&self, subcommand: &str) -> Command {
        let mut cmd = Command::new(program());
        cmd.arg(subcommand)
            .args(["--url", &self.url, "--key-id", &self.key_id]);
        cmd
    }

    /// Captures stdout (the PEM or signature). A remote signer's stderr goes
    /// straight to the terminal, so interactive prompts such as the GitHub
    /// sign-in URL and code are visible while the signer waits for them. A
    /// local signer never prompts; its stderr (the development-key warning)
    /// is replayed on the calling thread so libtest can capture it.
    fn output(&self, mut cmd: Command) -> Result<Vec<u8>> {
        cmd.stdin(Stdio::inherit()).stderr(if self.is_local() {
            Stdio::piped()
        } else {
            Stdio::inherit()
        });
        let output = cmd.output().with_context(|| {
            format!("cannot execute {cmd:?}; install microtun-signer or set MICROTUN_SIGNER")
        })?;
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        ensure!(
            output.status.success(),
            "microtun-signer failed ({}); see its messages above",
            output.status
        );
        Ok(output.stdout)
    }

    pub fn public_key(&self) -> Result<String> {
        let out = self.output(self.command("public-key"))?;
        let pem = String::from_utf8(out)?;
        ensure!(
            pem.trim_start().starts_with("-----BEGIN PUBLIC KEY-----"),
            "signer did not return an SPKI PEM public key"
        );
        if let Some(path) = std::env::var_os("MICROTUN_SIGNER_EXPECTED_PUBLIC_KEY") {
            let path = PathBuf::from(path);
            let expected = std::fs::read(&path)
                .with_context(|| format!("read trusted signer key {}", path.display()))?;
            let actual = pem::parse(pem.as_bytes())?;
            let expected = pem::parse(&expected)?;
            ensure!(
                actual.tag() == "PUBLIC KEY"
                    && expected.tag() == "PUBLIC KEY"
                    && actual.contents() == expected.contents(),
                "microtun-signer public key differs from MICROTUN_SIGNER_EXPECTED_PUBLIC_KEY"
            );
        }
        Ok(pem)
    }

    pub fn sign_digest(&self, digest: &[u8]) -> Result<Vec<u8>> {
        ensure!(
            digest.len() == 32,
            "microtun-signer requires a SHA-256 digest"
        );
        let mut cmd = self.command("sign");
        cmd.arg("--digest").arg(STANDARD.encode(digest));
        if self.github_login && !self.is_local() {
            cmd.arg("--github-login");
        }
        let out = self.output(cmd)?;
        STANDARD
            .decode(String::from_utf8(out)?.trim())
            .context("signer returned invalid base64 signature")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROM: DevKey = DevKey {
        key_id: "rom-dev",
        env: "TEST_ROM_KEY_ID",
    };

    fn ids(
        flag: Option<&str>,
        purpose: Option<&str>,
        generic: Option<&str>,
    ) -> [Option<String>; 3] {
        [flag, purpose, generic].map(|id| id.map(String::from))
    }

    #[test]
    fn local_url_and_purpose_key_are_the_defaults() {
        let signer = Signer::resolve(None, ids(None, None, None), false, ROM).unwrap();
        assert_eq!(signer.url, "local:");
        assert_eq!(signer.key_id, "rom-dev");
        // Empty values count as unset.
        let signer = Signer::resolve(
            Some(String::new()),
            ids(Some(""), Some(""), Some("")),
            false,
            ROM,
        )
        .unwrap();
        assert_eq!(
            (signer.url.as_str(), signer.key_id.as_str()),
            ("local:", "rom-dev")
        );
        let signer = Signer::resolve(
            Some("local:/tmp/keys".into()),
            ids(None, None, None),
            false,
            ROM,
        )
        .unwrap();
        assert_eq!(signer.key_id, "rom-dev");
    }

    #[test]
    fn flag_beats_purpose_variable_beats_generic_variable() {
        let key = |flag, purpose, generic| {
            Signer::resolve(None, ids(flag, purpose, generic), false, ROM)
                .unwrap()
                .key_id
        };
        assert_eq!(key(Some("flag"), Some("purpose"), Some("generic")), "flag");
        assert_eq!(key(None, Some("purpose"), Some("generic")), "purpose");
        assert_eq!(key(None, None, Some("generic")), "generic");
    }

    #[test]
    fn service_url_requires_an_explicit_key_id() {
        let service = || Some("https://signer.example".into());
        let error = Signer::resolve(service(), ids(None, None, None), false, ROM).unwrap_err();
        assert!(error.to_string().contains("TEST_ROM_KEY_ID"));
        for key_ids in [
            ids(Some("release"), None, None),
            ids(None, Some("release"), None),
            ids(None, None, Some("release")),
        ] {
            let signer = Signer::resolve(service(), key_ids, true, ROM).unwrap();
            assert!(!signer.is_local());
            assert_eq!(signer.key_id, "release");
        }
    }

    #[test]
    fn missing_digest_is_rejected_before_invoking_signer() {
        let signer = Signer::resolve(None, ids(None, None, None), false, ROM).unwrap();
        assert!(signer.sign_digest(&[0; 31]).is_err());
    }

    /// stdout is still returned to the caller while a remote signer's stderr
    /// (where it prints its GitHub sign-in prompt) is left on the terminal.
    #[cfg(unix)]
    #[test]
    fn signer_stdout_is_returned_and_stderr_passed_on() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("fake-signer");
        std::fs::write(
            &fake,
            "#!/bin/sh\necho 'GitHub authentication required (test prompt)' >&2\necho c2lnbmF0dXJl\n",
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        for url in ["https://example.invalid", "local:"] {
            let signer =
                Signer::resolve(Some(url.into()), ids(Some("test"), None, None), false, ROM)
                    .unwrap();
            let mut cmd = Command::new(&fake);
            cmd.arg("sign");
            let stdout = signer.output(cmd).unwrap();
            assert_eq!(String::from_utf8(stdout).unwrap().trim(), "c2lnbmF0dXJl");
        }
    }
}
