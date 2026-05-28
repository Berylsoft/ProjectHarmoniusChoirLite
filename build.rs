use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    str::FromStr,
};

use anyhow::Context as _;

macro_rules! handle_output {
    ($out:expr) => {
        if !$out.status.success() {
            ::std::io::Write::write_all(
                &mut ::std::io::stdout(),
                &$out.stdout,
            )
            .unwrap();
            ::std::io::Write::write_all(
                &mut ::std::io::stderr(),
                &$out.stderr,
            )
            .unwrap();
            return ExitCode::FAILURE;
        }
    };
}

fn copy_rec(
    from: impl AsRef<Path>,
    to: impl AsRef<Path>,
) -> anyhow::Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();

    if from.is_dir() {
        if !to.exists() {
            fs::create_dir_all(to).context("fs::create_dir_all")?;
        }
        anyhow::ensure!(to.is_dir());
        for entry in fs::read_dir(from).context("fs::read_dir")? {
            let entry = entry.context("entry")?;
            let name = entry.file_name();
            copy_rec(from.join(&name), to.join(name))
                .context("copy_rec")?;
        }
    } else {
        fs::copy(from, to).context("fs::copy")?;
    }

    Ok(())
}

fn main() -> ExitCode {
    println!("cargo::rerun-if-changed=sql/");

    let out_dir = env::var("OUT_DIR").unwrap();
    let out_dir = PathBuf::from_str(&out_dir).unwrap();
    let out_dir_sql = out_dir.join("sql");
    if fs::exists(&out_dir_sql).unwrap() {
        fs::remove_dir_all(&out_dir_sql).unwrap();
    }
    fs::create_dir_all(&out_dir_sql).unwrap();

    copy_rec("./sql/", &out_dir_sql).unwrap();

    let out = Command::new("sqlc")
        .current_dir(&out_dir_sql)
        .arg("generate")
        .output()
        .unwrap();
    handle_output!(out);

    let out = Command::new("rustfmt")
        .arg(out_dir_sql.join("out").join("generated.rs"))
        .output()
        .unwrap();
    handle_output!(out);

    ExitCode::SUCCESS
}
