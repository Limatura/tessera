//! Prepares the directory that `rust-embed` compiles the SPA out of.
//!
//! `rust-embed` needs its folder to exist at compile time, but `apps/web/dist` is a
//! build output that may not have been produced yet (a fresh clone, a backend-only
//! CI job, `cargo run` during API work). Rather than making every `cargo build`
//! depend on a pnpm build, this script mirrors the web bundle into
//! `apps/server/assets/dist` when it exists and drops in a placeholder page when it
//! does not. The server therefore always compiles, and always serves *something*.
//!
//! Errors are returned rather than panicked: cargo prints the `Err` and fails the
//! build, which is the same outcome without tripping the crate's `panic` lint.

use std::{
    env,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const PLACEHOLDER: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>tessera</title>
<style>
  body { font: 16px/1.6 system-ui, sans-serif; margin: 4rem auto; max-width: 40rem; padding: 0 1rem; }
  code { background: #eee; padding: .15em .4em; border-radius: .25em; }
</style>
</head>
<body>
<h1>tessera</h1>
<p>The API is running, but no frontend bundle was embedded in this binary.</p>
<p>Build the SPA and recompile:</p>
<pre><code>pnpm --filter web build
cargo build --release</code></pre>
<p>API docs: <a href="/docs">/docs</a> &middot; Health: <a href="/health">/health</a></p>
</body>
</html>
"#;

fn main() -> Result<(), Box<dyn Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let embed_dir = manifest_dir.join("assets").join("dist");

    // Overridable so a release pipeline can point at an artifact directory instead
    // of the in-repo web workspace.
    println!("cargo::rerun-if-env-changed=TESSERA_WEB_DIST");
    let source = env::var_os("TESSERA_WEB_DIST").map_or_else(
        || manifest_dir.join("..").join("web").join("dist"),
        PathBuf::from,
    );
    println!("cargo::rerun-if-changed={}", source.display());
    println!("cargo::rerun-if-changed=build.rs");

    fs::create_dir_all(&embed_dir)?;

    if source.is_dir() {
        clear_dir(&embed_dir)?;
        copy_dir(&source, &embed_dir)?;
    }

    if is_empty(&embed_dir)? {
        fs::write(embed_dir.join("index.html"), PLACEHOLDER)?;
    }

    emit_build_revision(manifest_dir.as_path());

    Ok(())
}

/// Records which commit produced this binary, as `TESSERA_BUILD_REV`.
///
/// A version number alone cannot identify a build between releases, which is
/// exactly when bug reports arrive. Resolution order:
///
/// 1. `TESSERA_BUILD_REV` from the environment — how a container build, which has no `.git`, gets a
///    real answer.
/// 2. `git describe`, for an ordinary checkout.
/// 3. `unknown`, so a source tarball still builds.
fn emit_build_revision(manifest_dir: &Path) {
    println!("cargo::rerun-if-env-changed=TESSERA_BUILD_REV");

    let revision = env::var("TESSERA_BUILD_REV")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| git_revision(manifest_dir))
        .unwrap_or_else(|| "unknown".to_owned());

    println!("cargo::rustc-env=TESSERA_BUILD_REV={revision}");
}

fn git_revision(manifest_dir: &Path) -> Option<String> {
    // Rebuild when HEAD moves. Emitted only when the path exists — pointing
    // `rerun-if-changed` at a missing file makes cargo rebuild every time.
    let git_head = manifest_dir.join("../../.git/HEAD");
    if git_head.is_file() {
        println!("cargo::rerun-if-changed={}", git_head.display());
    }

    let output = Command::new("git")
        .args(["-C", &manifest_dir.display().to_string()])
        .args(["describe", "--always", "--dirty=-dirty", "--abbrev=12"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let revision = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!revision.is_empty()).then_some(revision)
}

fn is_empty(dir: &Path) -> Result<bool, Box<dyn Error>> {
    Ok(fs::read_dir(dir)?.next().is_none())
}

fn clear_dir(dir: &Path) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            fs::create_dir_all(&dst)?;
            copy_dir(&src, &dst)?;
        } else {
            fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}
