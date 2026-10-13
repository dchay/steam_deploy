use anyhow::{bail, ensure, Context, Result};
use glob::{glob, Pattern};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use crate::models::config::Config;

const GAME_EXECUTABLE: &str = "PinkChocolate.exe";
const STEAM_APP_ID_FILE: &str = "SteamAppId.txt";
const TARGET_MAP: &str = "L_PChoc_Main";

/// Verified output of a successful Unreal BuildCookRun operation.
#[derive(Clone, Debug)]
pub struct BuildArtifacts {
    pub staged_output_dir: PathBuf,
    pub executable_path: PathBuf,
    pub steam_appid_path: PathBuf,
    pub pak_files: Vec<PathBuf>,
}

/// Build, stage, and verify the Win64 Shipping output.
///
/// The staged directory is always:
/// `<ProjectRoot>/Saved/StagedBuilds/ReleaseA/Windows/`.
/// Existing output at that exact location is removed before starting, preventing
/// stale artifacts from being mistaken for output from the current build.
pub fn run(config: &Config) -> Result<BuildArtifacts> {
    ensure!(cfg!(windows), "PinkChoco Deploy Build is Windows-only");
    ensure!(
        config.uproject.is_file(),
        "Unreal project descriptor is missing: {}",
        config.uproject.display()
    );
    ensure!(
        config.uat_bat.is_file(),
        "RunUAT.bat is missing: {}",
        config.uat_bat.display()
    );

    let project_root = config
        .uproject
        .parent()
        .context("The .uproject path has no parent directory")?;
    ensure!(
        project_root.is_dir(),
        "Project root is not a directory: {}",
        project_root.display()
    );

    let staged_output_dir = project_root
        .join("Saved")
        .join("StagedBuilds")
        .join("ReleaseA")
        .join("Windows");

    prepare_output_directory(&staged_output_dir)?;

    let output = execute_uat(config, project_root, &staged_output_dir)?;
    ensure_uat_succeeded(&output)?;
    forward_success_output(&output);

    verify_staged_output(staged_output_dir)
}

fn prepare_output_directory(staged_output_dir: &Path) -> Result<()> {
    if staged_output_dir.exists() {
        fs::remove_dir_all(staged_output_dir).with_context(|| {
            format!(
                "Cannot remove previous staged output: {}",
                staged_output_dir.display()
            )
        })?;
    }

    fs::create_dir_all(staged_output_dir).with_context(|| {
        format!(
            "Cannot create staged output directory: {}",
            staged_output_dir.display()
        )
    })
}

fn execute_uat(
    config: &Config,
    project_root: &Path,
    staged_output_dir: &Path,
) -> Result<Output> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        let uat = quote_cmd_path(&config.uat_bat, "RunUAT.bat")?;
        let project = quote_cmd_path(&config.uproject, "project descriptor")?;
        let staging = quote_cmd_path(staged_output_dir, "staging directory")?;

        // RunUAT.bat is a batch file, so launch it through cmd.exe. raw_arg is
        // used only for this Windows shell command so that the quotes around
        // paths remain in the form expected by CMD and Unreal's argument parser.
        let command_line = format!(
            "call {uat} BuildCookRun -project={project} -noP4 \
             -platform=Win64 -clientconfig=Shipping -build -cook -stage -pak \
             -map={TARGET_MAP} -stagingdirectory={staging} \
             -unattended -utf8output -nopause"
        );

        let mut command = Command::new("cmd.exe");
        command
            .args(["/D", "/V:OFF", "/S", "/C"])
            .current_dir(project_root)
            .raw_arg(format!(" {command_line}"));

        command.output().with_context(|| {
            format!(
                "Failed to launch RunUAT.bat through cmd.exe: {}",
                config.uat_bat.display()
            )
        })
    }

    #[cfg(not(windows))]
    {
        let _ = (config, project_root, staged_output_dir);
        bail!("RunUAT.bat can only be executed on Windows")
    }
}

#[cfg(windows)]
fn quote_cmd_path(path: &Path, description: &str) -> Result<String> {
    let value = path.to_str().with_context(|| {
        format!("{description} path is not valid Unicode: {}", path.display())
    })?;

    // CMD expands percent variables even inside double quotes. Reject shell
    // quoting/control characters rather than silently changing the path.
    ensure!(
        !value.contains('"')
            && !value.contains('%')
            && !value.contains('\r')
            && !value.contains('\n'),
        "{description} path contains unsupported CMD characters: {}",
        path.display()
    );

    Ok(format!("\"{value}\""))
}

fn ensure_uat_succeeded(output: &Output) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }

    bail!(
        "Unreal BuildCookRun failed with {}.\n\
         --- stdout ---\n{}\n\
         --- stderr ---\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout).trim_end(),
        String::from_utf8_lossy(&output.stderr).trim_end()
    )
}

fn forward_success_output(output: &Output) {
    if !output.stdout.is_empty() {
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }
    if !output.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
    }
}

fn verify_staged_output(staged_output_dir: PathBuf) -> Result<BuildArtifacts> {
    ensure!(
        staged_output_dir.is_dir(),
        "Staged output directory is missing: {}",
        staged_output_dir.display()
    );

    let executable_path = staged_output_dir.join(GAME_EXECUTABLE);
    ensure!(
        executable_path.is_file(),
        "Game executable is missing: {}",
        executable_path.display()
    );

    let steam_appid_path = staged_output_dir.join(STEAM_APP_ID_FILE);
    ensure!(
        steam_appid_path.is_file(),
        "SteamAppId.txt is missing: {}",
        steam_appid_path.display()
    );

    let pak_files = discover_pak_files(&staged_output_dir)?;
    ensure!(
        !pak_files.is_empty(),
        "No cooked .pak files found under: {}",
        staged_output_dir.display()
    );

    Ok(BuildArtifacts {
        staged_output_dir,
        executable_path,
        steam_appid_path,
        pak_files,
    })
}

fn discover_pak_files(staged_output_dir: &Path) -> Result<Vec<PathBuf>> {
    let root = staged_output_dir.to_string_lossy().replace('\\', "/");
    let escaped_root = Pattern::escape(&root);
    let pattern = format!("{escaped_root}/**/*");

    let mut pak_files = Vec::new();
    for entry in glob(&pattern)
        .with_context(|| format!("Invalid staged-file glob: {pattern}"))?
    {
        let path = entry.with_context(|| {
            format!("Failed while scanning staged output: {pattern}")
        })?;

        if path.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("pak"))
        {
            pak_files.push(path);
        }
    }

    pak_files.sort();
    pak_files.dedup();
    Ok(pak_files)
}
