use std::{
    env, fs, io,
    path::{self, Path, PathBuf},
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

fn gen_hash_for_file(path: &Path, name: &str) -> anyhow::Result<()> {
    let mut file = fs::OpenOptions::new()
        .read(true)
        .open(path)
        .context("open")?;
    let mut hasher = blake3::Hasher::new();
    io::copy(&mut file, &mut hasher).context("copy to hash")?;
    let hash = hasher.finalize();
    let hash = hash.to_hex();
    println!("cargo::rustc-env=AH_{name}={hash}");

    Ok(())
}

fn collect_hash_entry_rec(
    root: &Path,
    suffix: &Path,
) -> anyhow::Result<()> {
    let path = root.join(suffix);

    for entry in fs::read_dir(path).context("read_dir")? {
        let entry = entry.context("entry")?;
        let ft = entry.file_type().context("file_type")?;
        let suffix = suffix.join(entry.file_name());
        if ft.is_dir() {
            collect_hash_entry_rec(root, &suffix)
                .context("gen_hash_rec")?;
        } else if ft.is_file() {
            let name = suffix
                .components()
                .map(|it| match it {
                    path::Component::Prefix(_)
                    | path::Component::RootDir
                    | path::Component::CurDir
                    | path::Component::ParentDir => unreachable!(),
                    path::Component::Normal(os_str) => {
                        os_str.to_str().expect("utf8")
                    }
                })
                .fold(String::new(), |mut acc, it| {
                    if !acc.is_empty() {
                        acc.push('_');
                    }

                    for (idx, ch) in it.char_indices() {
                        if matches!(ch, '.') {
                            acc.push('_');
                            continue;
                        }

                        if ch.is_ascii_uppercase()
                            && idx != 0
                            && it.as_bytes()[idx - 1] != b'_'
                        {
                            acc.push('_');
                        }

                        acc.push(ch.to_ascii_uppercase());
                    }

                    acc
                });

            gen_hash_for_file(&entry.path(), &name)
                .context("gen_hash_for_file")?;
        } else {
            unreachable!()
        }
    }

    Ok(())
}

fn main() -> ExitCode {
    println!("cargo::rerun-if-changed=sql/");

    println!("cargo::rerun-if-changed=static/");
    collect_hash_entry_rec(Path::new("./static"), Path::new("")).unwrap();

    if env::var("CARGO_FEATURE_DEV_AUTO_RELOAD").is_ok() {
        println!("cargo::rustc-env=AUTO_RELOAD=true");
    } else {
        println!("cargo::rustc-env=AUTO_RELOAD=false");
    }

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
