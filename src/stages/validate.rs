use anyhow::{bail, ensure, Context, Result};
use glob::glob;
use std::{
    collections::BTreeSet,
    env,
    path::{Path, PathBuf},
    process::Command,
};

use crate::models::config::Config;

/// Load and validate deployment configuration. Returns `Config` only when
/// all checks pass. Unreal compilation/cooking belongs to the Build stage.
pub fn run(config_path: impl AsRef<Path>) -> Result<Config> {
    ensure!(cfg!(windows), "PinkChoco Deploy validation is Windows-only");

    let config = Config::load(config_path)?;
    load_dotenv_if_present()?;

    check_config_values(&config)?;
    require_env("STEAM_USERNAME")?;
    // STEAM_PASSWORD is intentionally optional: SteamCMD may use its credential cache.
    check_paths(&config)?;
    resolve_depot_vdf_templates(&config.depot_vdf_template_glob)?;
    check_msvc_toolchain(&config)?;
    check_cxx23_support(&config)?;

    Ok(config)
}

fn load_dotenv_if_present() -> Result<()> {
    if Path::new(".env").is_file() {
        dotenv::from_path(".env").context("Cannot load .env")?;
    } else {
        // A .env file is optional; credentials may already exist in the process environment.
        let _ = dotenv::dotenv();
    }
    Ok(())
}

fn check_config_values(config: &Config) -> Result<()> {
    ensure!(config.app_id > 0, "app_id must be greater than zero");
    ensure!(!config.beta_branch.trim().is_empty(), "beta_branch is required");
    ensure!(
        !config.msvc_compiler_version_prefix.trim().is_empty(),
        "msvc_compiler_version_prefix is required"
    );
    ensure!(!config.store_depot_ids.is_empty(), "store_depot_ids must not be empty");
    ensure!(!config.devcomp_depot_ids.is_empty(), "devcomp_depot_ids must not be empty");

    let store: BTreeSet<u32> = config.store_depot_ids.iter().copied().collect();
    let devcomp: BTreeSet<u32> = config.devcomp_depot_ids.iter().copied().collect();

    ensure!(
        store.len() == config.store_depot_ids.len(),
        "store_depot_ids contains duplicates"
    );
    ensure!(
        devcomp.len() == config.devcomp_depot_ids.len(),
        "devcomp_depot_ids contains duplicates"
    );
    ensure!(
        store.iter().all(|id| *id > 0) && devcomp.iter().all(|id| *id > 0),
        "Depot IDs must be greater than zero"
    );
    ensure!(store == devcomp, "Store and DevComp depot ID sets do not match");
    Ok(())
}

fn require_env(name: &str) -> Result<()> {
    let value = env::var(name).with_context(|| {
        format!("Required environment variable {name} is missing or not Unicode")
    })?;
    ensure!(!value.trim().is_empty(), "Required environment variable {name} is empty");
    Ok(())
}

fn check_paths(config: &Config) -> Result<()> {
    require_dir(&config.engine_root, "Unreal Engine root")?;
    require_file(&config.uproject, "Unreal project descriptor")?;
    require_file(&config.uat_bat, "RunUAT.bat")?;
    require_dir(&config.steamworks_sdk_dir, "Steamworks SDK directory")?;
    require_file(&config.steamcmd_exe, "SteamCMD executable")?;
    require_file(&config.msvc_cl_exe, "MSVC compiler executable")?;
    require_file(&config.cxx23_probe, "C++23 probe source")?;
    require_file(&config.app_build_vdf_template, "App-build VDF template")?;
    Ok(())
}

fn require_file(path: &Path, description: &str) -> Result<()> {
    ensure!(
        path.is_file(),
        "{description} is missing or not a file: {}",
        path.display()
    );
    Ok(())
}

fn require_dir(path: &Path, description: &str) -> Result<()> {
    ensure!(
        path.is_dir(),
        "{description} is missing or not a directory: {}",
        path.display()
    );
    Ok(())
}

/// Resolve depot templates and validate that at least one matching file exists.
/// Contents are deliberately not read or parsed in Validate.
fn resolve_depot_vdf_templates(pattern: &str) -> Result<Vec<PathBuf>> {
    let entries = glob(pattern)
        .with_context(|| format!("Invalid depot VDF template glob: {pattern}"))?;

    let mut paths = Vec::new();
    for entry in entries {
        paths.push(entry.with_context(|| {
            format!("Cannot enumerate depot VDF templates for glob: {pattern}")
        })?);
    }

    paths.sort();
    paths.dedup();
    ensure!(!paths.is_empty(), "No depot VDF templates matched: {pattern}");

    for path in &paths {
        require_file(path, "Depot VDF template")?;
    }

    Ok(paths)
}

fn check_msvc_toolchain(config: &Config) -> Result<()> {
    let output = Command::new(&config.msvc_cl_exe)
        .arg("/Bv")
        .output()
        .with_context(|| {
            format!("Failed to execute MSVC compiler: {}", config.msvc_cl_exe.display())
        })?;

    let report = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    ensure!(
        report.contains(config.msvc_compiler_version_prefix.trim()),
        "MSVC version check failed for {}: expected version marker {:?}. Compiler report:\n{}",
        config.msvc_cl_exe.display(),
        config.msvc_compiler_version_prefix,
        report.trim()
    );
    Ok(())
}

fn check_cxx23_support(config: &Config) -> Result<()> {
    let output = Command::new(&config.msvc_cl_exe)
        .args(["/nologo", "/std:c++latest", "/Zc:__cplusplus", "/Zs"])
        .arg(&config.cxx23_probe)
        .output()
        .with_context(|| {
            format!(
                "Failed to run C++23 probe with {}",
                config.msvc_cl_exe.display()
            )
        })?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "C++23 compiler probe failed ({}).\nstdout:\n{}\nstderr:\n{}",
            config.cxx23_probe.display(),
            stdout.trim(),
            stderr.trim()
        );
    }
    Ok(())
}
