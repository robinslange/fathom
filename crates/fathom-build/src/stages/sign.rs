//! Stage 9 — sign.
//!
//! minisign the manifest. Auto-generates a fresh keypair on first run if no
//! key is present at the expected path; commits the public key to
//! `dist/fathom.pub` so verify (and the runtime) have what they need.
//!
//! Key paths default to `~/.config/fathom/minisign.{key,pub}` (override via
//! FATHOM_MINISIGN_KEY / FATHOM_MINISIGN_PUB env vars).
//!
//! Password is read from MINISIGN_PASSWORD env if set; otherwise the key is
//! generated unencrypted. For a single-operator build pipeline the password
//! buys little practical security and adds operational friction.

use crate::stages::shard::dist_dir;
use anyhow::{anyhow, Context, Result};
use clap::Args as ClapArgs;
use minisign::{KeyPair, SecretKeyBox};
use std::io::Cursor;
use std::path::PathBuf;

#[derive(Debug, ClapArgs, Default)]
pub struct Args {
    /// Path to the minisign secret key. Defaults to
    /// `$HOME/.config/fathom/minisign.key` (overridable via FATHOM_MINISIGN_KEY).
    #[arg(long, env = "FATHOM_MINISIGN_KEY")]
    pub key: Option<PathBuf>,
    /// Path to the minisign public key. Defaults to alongside the secret key.
    #[arg(long, env = "FATHOM_MINISIGN_PUB")]
    pub pub_key: Option<PathBuf>,
    /// Generate a new keypair on first use if absent.
    #[arg(long, default_value_t = true)]
    pub auto_generate: bool,
}

pub async fn run(args: Args) -> Result<()> {
    let (secret_path, public_path) = resolve_key_paths(&args)?;
    ensure_keypair(&secret_path, &public_path, args.auto_generate)?;

    let manifest_path = dist_dir().join("index.msgpack");
    let manifest_bytes = std::fs::read(&manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;

    let sk_string = std::fs::read_to_string(&secret_path)
        .with_context(|| format!("read secret key {}", secret_path.display()))?;
    let sk_box = SecretKeyBox::from_string(&sk_string).map_err(|e| anyhow!("parse key: {e}"))?;
    let password =
        std::env::var("MINISIGN_PASSWORD").unwrap_or_else(|_| "fathom-build".to_string());
    let sk = sk_box
        .into_secret_key(Some(password))
        .map_err(|e| anyhow!("decrypt key: {e}"))?;

    let mut reader = Cursor::new(&manifest_bytes);
    let signature = minisign::sign(
        None,
        &sk,
        &mut reader,
        Some("fathom v0.2 manifest"),
        Some("fathom corpus index"),
    )
    .map_err(|e| anyhow!("sign manifest: {e}"))?;

    let sig_path = dist_dir().join("index.msgpack.minisig");
    std::fs::write(&sig_path, signature.into_string())
        .with_context(|| format!("write {}", sig_path.display()))?;

    // Also copy the public key into dist/ so deploy/verify have everything in
    // one place.
    let dist_pub = dist_dir().join("fathom.pub");
    std::fs::copy(&public_path, &dist_pub)
        .with_context(|| format!("copy public key → {}", dist_pub.display()))?;

    eprintln!("sign: signed manifest, signature → {}", sig_path.display());
    eprintln!("sign: public key → {}", dist_pub.display());
    Ok(())
}

fn resolve_key_paths(args: &Args) -> Result<(PathBuf, PathBuf)> {
    let default_dir = directories::ProjectDirs::from("nz", "omit", "fathom")
        .ok_or_else(|| anyhow!("no project dirs available"))?
        .config_dir()
        .to_path_buf();

    let key = args
        .key
        .clone()
        .unwrap_or_else(|| default_dir.join("minisign.key"));
    let pub_key = args
        .pub_key
        .clone()
        .unwrap_or_else(|| key.with_extension("pub"));

    Ok((key, pub_key))
}

fn ensure_keypair(secret: &PathBuf, public: &PathBuf, auto_generate: bool) -> Result<()> {
    if secret.exists() && public.exists() {
        return Ok(());
    }
    if !auto_generate {
        return Err(anyhow!(
            "no keypair at {} / {} (pass --auto-generate to create one)",
            secret.display(),
            public.display()
        ));
    }
    if let Some(p) = secret.parent() {
        std::fs::create_dir_all(p).with_context(|| format!("create key dir {}", p.display()))?;
    }

    // Always generate via the encrypted path. Use MINISIGN_PASSWORD if set,
    // else the sentinel "fathom-build" — this avoids the interactive prompt
    // that the unencrypted variant cannot decode without TTY.
    let password =
        std::env::var("MINISIGN_PASSWORD").unwrap_or_else(|_| "fathom-build".to_string());
    let kp = KeyPair::generate_encrypted_keypair(Some(password.clone()))
        .map_err(|e| anyhow!("generate keypair: {e}"))?;

    std::fs::write(
        secret,
        kp.sk
            .to_box(None)
            .map_err(|e| anyhow!("encode sk: {e}"))?
            .into_string(),
    )
    .with_context(|| format!("write {}", secret.display()))?;
    std::fs::write(
        public,
        kp.pk
            .to_box()
            .map_err(|e| anyhow!("encode pk: {e}"))?
            .into_string(),
    )
    .with_context(|| format!("write {}", public.display()))?;
    eprintln!(
        "sign: keypair password is in MINISIGN_PASSWORD env (sentinel 'fathom-build' if unset)"
    );

    eprintln!(
        "sign: generated new keypair → {} / {}",
        secret.display(),
        public.display()
    );
    Ok(())
}
