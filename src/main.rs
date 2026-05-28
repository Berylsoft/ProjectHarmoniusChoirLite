#![warn(missing_debug_implementations)]
#![warn(clippy::pedantic, clippy::nursery)]
// #![clippy::too_many_line_threshold = 60]
#![allow(clippy::default_trait_access)]
#![allow(clippy::unnecessary_debug_formatting)]

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use base64::{Engine as _, prelude::BASE64_URL_SAFE_NO_PAD};
use ed25519_dalek::{
    SigningKey, VerifyingKey,
    pkcs8::{
        DecodePrivateKey as _, DecodePublicKey, EncodePrivateKey as _,
    },
};
use review_sys::{
    ServerConfig, ServerState,
    routes::routes,
    sql::db_open,
    utils::{init_env, shutdown_signal, var_opt},
};
use rusqlite::OpenFlags;

#[derive(Debug, argh::FromArgs)]
/// .
struct Args {
    /// change working directory
    #[argh(option, short = 'C')]
    cwd: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    init_env();

    let args: Args = argh::from_env();
    if let Some(path) = args.cwd {
        tracing::info!("set current working directory to: {path:?}");
        std::env::set_current_dir(path).context("set_current_dir")?;
    }

    tracing::info!("initialize database");
    let conn = db_open("database.db", OpenFlags::default())
        .context("db_open")?;
    drop(conn);

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(run())
}

async fn run() -> anyhow::Result<()> {
    let addr = var_opt("BIND_ADDR")
        .context("get BIND_ADDR")?
        .unwrap_or_else(|| "127.0.0.1:29701".to_owned());

    let redir_prefix = var_opt("REDIR_PREFIX")
        .context("get REDIR_PREFIX")?
        .unwrap_or_else(|| format!("http://{addr}"));

    let key =
        get_or_init_signing_key().context("get_or_init_signing_key")?;

    let bot_key = get_bot_pub_key().context("get_bot_pub_key")?;

    let db_path = var_opt("DATABASE_PATH")
        .context("DATABASE_PATH")?
        .context("DATABASE_PATH")?;

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .context("TcpListener::bind")?;

    tracing::info!("listening on {addr}");

    let state = ServerState {
        cfg: ServerConfig {
            db_path: PathBuf::from(db_path).into_boxed_path(),
            key,
            bot_key,
            redir_prefix: redir_prefix.into_boxed_str(),
        }
        .into(),
    };
    let app = routes(state);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("axum::serve")?;

    Ok(())
}

fn get_or_init_signing_key() -> anyhow::Result<SigningKey> {
    let env_key = var_opt("SIGNING_KEY")
        .context("SIGNING_KEY")?
        .map(|it| BASE64_URL_SAFE_NO_PAD.decode(it))
        .transpose()
        .context("decode SIGNING_KEY as base64")?
        .map(|it| SigningKey::from_pkcs8_der(&it))
        .transpose()
        .context("parse SIGNING_KEY as pkcs8_der")?;

    if let Some(key) = env_key {
        return Ok(key);
    }

    let file_key = fs::exists("./signing_key.der")
        .context("check signing_key.der exists")?
        .then(|| fs::read("./signing_key.der"))
        .transpose()
        .context("read ./signing_key.der")?
        .map(|it| SigningKey::from_pkcs8_der(&it))
        .transpose()
        .context("parse ./signing_key.der")?;

    if let Some(key) = file_key {
        return Ok(key);
    }

    let key = SigningKey::generate(&mut rand::rngs::OsRng);

    let der = key.to_pkcs8_der().expect("expect encoding success");
    der.write_der_file("./signing_key.der")
        .context("write ./signing_key.der")?;
    let der_base64 = BASE64_URL_SAFE_NO_PAD.encode(der.as_bytes());
    fs::write("./signing_key.der.base64", der_base64)
        .context("write ./signing_key.der.base64")?;

    Ok(key)
}

fn get_bot_pub_key() -> anyhow::Result<VerifyingKey> {
    let env_key = var_opt("BOT_PUB_KEY")
        .context("BOT_PUB_KEY")?
        .map(|it| BASE64_URL_SAFE_NO_PAD.decode(it))
        .transpose()
        .context("decode BOT_PUB_KEY as base64")?
        .map(|it| VerifyingKey::from_public_key_der(&it))
        .transpose()
        .context("parse BOT_PUB_KEY as public_key_der")?;

    if let Some(key) = env_key {
        return Ok(key);
    }

    let file_key = fs::exists("./bot_pub_key.der")
        .context("check bot_pub_key.der exists")?
        .then(|| fs::read("./bot_pub_key.der"))
        .transpose()
        .context("read ./bot_pub_key.der")?
        .map(|it| VerifyingKey::from_public_key_der(&it))
        .transpose()
        .context("parse ./bot_pub_key.der")?;

    if let Some(key) = file_key {
        return Ok(key);
    }

    Err(anyhow::anyhow!("bot public key not found"))
}
