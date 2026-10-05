//! Prepares the directories that `rust-embed` compiles the two SPAs out of.
//!
//! `rust-embed` needs its folder to exist at compile time, but `apps/web/dist`
//! and `apps/admin/dist` are build outputs that may not exist yet (a fresh
//! clone, a backend-only CI job, `cargo run` during API work). Instead of
//! making every `cargo build` depend on a pnpm build, this script mirrors each
//! bundle into `apps/server/assets/` when it exists and writes a placeholder
//! page when it does not. So the server always compiles and always serves
//! *something*.
//!
//! Errors are returned, not panicked. Cargo prints the `Err` and fails the
//! build: the same outcome without tripping the crate's `panic` lint.

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

/// The same, for the admin panel at `/admin/`.
const ADMIN_PLACEHOLDER: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>tessera admin</title>
<style>
  body { font: 16px/1.6 system-ui, sans-serif; margin: 4rem auto; max-width: 40rem; padding: 0 1rem; }
  code { background: #eee; padding: .15em .4em; border-radius: .25em; }
</style>
</head>
<body>
<h1>tessera admin</h1>
<p>No admin panel was embedded in this binary.</p>
<p>Build it and recompile:</p>
<pre><code>pnpm --filter admin build
cargo build --release</code></pre>
</body>
</html>
"#;

fn main() -> Result<(), Box<dyn Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    println!("cargo::rerun-if-changed=build.rs");

    mirror(
        &manifest_dir,
        "web",
        "TESSERA_WEB_DIST",
        "dist",
        PLACEHOLDER,
    )?;
    mirror(
        &manifest_dir,
        "admin",
        "TESSERA_ADMIN_DIST",
        "admin",
        ADMIN_PLACEHOLDER,
    )?;

    emit_build_revision(manifest_dir.as_path());

    Ok(())
}

/// Copies `apps/<app>/dist` into `assets/<into>`, or leaves a placeholder.
///
/// The source is overridable through `override_var`, so a release pipeline can
/// point at an artifact directory instead of the in-repo workspace.
fn mirror(
    manifest_dir: &Path,
    app: &str,
    override_var: &str,
    into: &str,
    placeholder: &str,
) -> Result<(), Box<dyn Error>> {
    let embed_dir = manifest_dir.join("assets").join(into);
    println!("cargo::rerun-if-env-changed={override_var}");
    let source = env::var_os(override_var).map_or_else(
        || manifest_dir.join("..").join(app).join("dist"),
        PathBuf::from,
    );
    println!("cargo::rerun-if-changed={}", source.display());

    fs::create_dir_all(&embed_dir)?;
    if source.is_dir() {
        clear_dir(&embed_dir)?;
        copy_dir(&source, &embed_dir)?;
    }
    if is_empty(&embed_dir)? {
        fs::write(embed_dir.join("index.html"), placeholder)?;
    }
    Ok(())
}

/// Records which commit produced this binary, as `TESSERA_BUILD_REV`.
///
/// A version number alone cannot identify a build between releases, exactly
/// when bug reports arrive. Resolution order:
///
/// 1. `TESSERA_BUILD_REV` from the environment, how a container build with no `.git` gets a real
///    answer.
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
    // Rebuild when HEAD moves. Emitted only when the path exists, since
    // pointing `rerun-if-changed` at a missing file rebuilds every time.
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
