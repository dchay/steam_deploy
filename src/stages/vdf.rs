use anyhow::{bail, ensure, Context, Result};
use fs_extra::dir::{copy as copy_dir, CopyOptions};
use glob::{glob, Pattern};
use minijinja::{context, Environment, UndefinedBehavior, Value as JinjaValue};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
};
use steam_vdf_parser::{parse_text, Obj, Value as VdfValue, Vdf};

use crate::{
    models::config::Config,
    stages::build::BuildArtifacts,
};

const CONTENT_DIR_NAME: &str = "content";
const SCRIPTS_DIR_NAME: &str = "scripts";
const OUTPUT_DIR_NAME: &str = "output";
const APP_BUILD_FILENAME: &str = "app_build.vdf";
const BOOTSTRAP_LAUNCHER: &str = "Pink_Chocolate_V8.exe";
const GAME_EXECUTABLE: &str = "PinkChocolate.exe";
const STEAM_APP_ID_FILE: &str = "SteamAppId.txt";
const REQUIRED_EXCLUSIONS: [&str; 3] = [
    "*.pdb",
    r"Saved\*",
    "Pink_Chocolate_V8.exe",
];

/// Fully prepared SteamPipe content and structurally verified VDF scripts.
#[derive(Clone, Debug)]
pub struct VdfArtifacts {
    pub content_dir: PathBuf,
    pub app_build_vdf_path: PathBuf,
    pub depot_vdf_paths: Vec<PathBuf>,
}

/// Prepare content, generate SteamPipe VDFs from the configured templates,
/// parse the generated files, and verify all IDs, references, paths, and exclusions.
pub fn run(config: &Config, build: &BuildArtifacts) -> Result<VdfArtifacts> {
    ensure!(cfg!(windows), "PinkChoco Deploy VDF stage is Windows-only");
    validate_config(config)?;
    validate_build_artifacts(build, config.app_id)?;

    let content_builder_dir = config
        .content_builder_dir
        .canonicalize()
        .with_context(|| {
            format!(
                "ContentBuilder directory is missing: {}",
                config.content_builder_dir.display()
            )
        })?;
    ensure!(
        content_builder_dir.is_dir(),
        "ContentBuilder path is not a directory: {}",
        content_builder_dir.display()
    );

    let staged_canonical = build
        .staged_output_dir
        .canonicalize()
        .with_context(|| {
            format!(
                "Staged build directory is missing: {}",
                build.staged_output_dir.display()
            )
        })?;

    ensure!(
        !path_contains(&content_builder_dir, &staged_canonical)
            && !path_contains(&staged_canonical, &content_builder_dir),
        "ContentBuilder and staged output directories must not contain one another"
    );

    let content_dir = content_builder_dir.join(CONTENT_DIR_NAME);
    let scripts_dir = content_builder_dir.join(SCRIPTS_DIR_NAME);
    let output_dir = content_builder_dir.join(OUTPUT_DIR_NAME);

    let depot_ids = normalized_depot_ids(config)?;
    let depot_templates = resolve_depot_templates(&config.depot_vdf_template_glob)?;
    let template_map = assign_depot_templates(&depot_ids, &depot_templates)?;
    ensure_template_outside_generated_dirs(
        &config.app_build_vdf_template,
        &content_dir,
        &scripts_dir,
    )?;
    for template in &depot_templates {
        ensure_template_outside_generated_dirs(template, &content_dir, &scripts_dir)?;
    }

    // Preflight both destinations before deleting either one's contents.
    ensure_clearable_directory(&content_dir)?;
    ensure_clearable_directory(&scripts_dir)?;
    clear_directory_contents(&content_dir)?;
    clear_directory_contents(&scripts_dir)?;
    ensure_directory(&output_dir)?;

    prepare_content(build, &content_dir)?;
    remove_bootstrap_launcher(&content_dir)?;
    verify_copied_build_content(build, config.app_id, &content_dir)?;

    let content_root = vdf_path_value(&content_dir);
    let build_output = vdf_path_value(&output_dir);
    let build_description = escape_vdf_scalar(config.build_description.trim());
    let beta_branch = escape_vdf_scalar(config.beta_branch.trim());
    ensure!(
        !build_description.is_empty(),
        "build_description must not be empty"
    );
    ensure!(!beta_branch.is_empty(), "beta_branch must not be empty");

    // Provided for compatibility with templates that already expose this slot.
    // apply_set_live_directive below enforces the final directive independently.
    let set_live_entry = if config.set_live_after_upload {
        format!("\"SetLive\" \"{beta_branch}\"")
    } else {
        String::new()
    };

    let depots: Vec<BTreeMap<String, String>> = depot_ids
        .iter()
        .map(|id| {
            let filename = format!("depot_build_{id}.vdf");
            BTreeMap::from([
                ("id".to_owned(), id.to_string()),
                ("depot_id".to_owned(), id.to_string()),
                ("filename".to_owned(), filename.clone()),
                ("vdf_filename".to_owned(), filename.clone()),
                ("depot_vdf_filename".to_owned(), filename),
            ])
        })
        .collect();
    let depot_entries = depot_ids
        .iter()
        .map(|id| format!("\"{id}\" \"depot_build_{id}.vdf\""))
        .collect::<Vec<_>>()
        .join("\n        ");

    let mut depot_vdf_paths = Vec::with_capacity(depot_ids.len());
    for id in &depot_ids {
        let template_path = template_map
            .get(id)
            .with_context(|| format!("No depot template assigned for depot ID {id}"))?;
        let template_source = fs::read_to_string(template_path).with_context(|| {
            format!("Cannot read depot VDF template: {}", template_path.display())
        })?;
        let filename = format!("depot_build_{id}.vdf");

        let rendered = render_template(
            "depot",
            &template_source,
            context! {
                depot_id => *id,
                DEPOT_ID => *id,
                app_id => config.app_id,
                APP_ID => config.app_id,
                build_description => build_description.as_str(),
                BUILD_DESCRIPTION => build_description.as_str(),
                content_root => content_root.as_str(),
                CONTENT_ROOT => content_root.as_str(),
                build_output => build_output.as_str(),
                BUILD_OUTPUT => build_output.as_str(),
                beta_branch => beta_branch.as_str(),
                BETA_BRANCH => beta_branch.as_str(),
                set_live_after_upload => config.set_live_after_upload,
                SET_LIVE_AFTER_UPLOAD => config.set_live_after_upload,
                depot_vdf_filename => filename.as_str(),
                DEPOT_VDF_FILENAME => filename.as_str(),
            },
            template_path,
        )?;

        let path = scripts_dir.join(filename);
        write_generated_file(&path, &rendered)?;
        depot_vdf_paths.push(path);
    }

    let app_template_source = fs::read_to_string(&config.app_build_vdf_template)
        .with_context(|| {
            format!(
                "Cannot read app-build VDF template: {}",
                config.app_build_vdf_template.display()
            )
        })?;
    let app_rendered = render_template(
        "app_build",
        &app_template_source,
        context! {
            app_id => config.app_id,
            APP_ID => config.app_id,
            build_description => build_description.as_str(),
            BUILD_DESCRIPTION => build_description.as_str(),
            content_root => content_root.as_str(),
            CONTENT_ROOT => content_root.as_str(),
            build_output => build_output.as_str(),
            BUILD_OUTPUT => build_output.as_str(),
            beta_branch => beta_branch.as_str(),
            BETA_BRANCH => beta_branch.as_str(),
            set_live_after_upload => config.set_live_after_upload,
            SET_LIVE_AFTER_UPLOAD => config.set_live_after_upload,
            set_live_entry => set_live_entry.as_str(),
            SET_LIVE_ENTRY => set_live_entry.as_str(),
            depot_entries => depot_entries.as_str(),
            DEPOT_ENTRIES => depot_entries.as_str(),
            depots => &depots,
            DEPOTS => &depots,
        },
        &config.app_build_vdf_template,
    )?;

    // Enforce SetLive centrally rather than relying on every template author to
    // implement the condition correctly. Any template-provided root SetLive entry
    // is removed, then exactly one is inserted only when explicitly enabled.
    let app_rendered = apply_set_live_directive(
        &app_rendered,
        config.beta_branch.trim(),
        config.set_live_after_upload,
    )?;

    let app_build_vdf_path = scripts_dir.join(APP_BUILD_FILENAME);
    write_generated_file(&app_build_vdf_path, &app_rendered)?;

    verify_generated_outputs(
        config,
        build,
        &content_dir,
        &output_dir,
        &app_build_vdf_path,
        &depot_vdf_paths,
        &depot_ids,
    )?;

    Ok(VdfArtifacts {
        content_dir,
        app_build_vdf_path,
        depot_vdf_paths,
    })
}

fn validate_config(config: &Config) -> Result<()> {
    ensure!(config.app_id > 0, "app_id must be greater than zero");
    ensure!(!config.beta_branch.trim().is_empty(), "beta_branch is required");
    ensure!(
        !config.build_description.trim().is_empty(),
        "build_description is required"
    );
    ensure!(
        config.content_builder_dir.is_dir(),
        "ContentBuilder directory is missing: {}",
        config.content_builder_dir.display()
    );
    ensure!(
        config.app_build_vdf_template.is_file(),
        "App-build VDF template is missing: {}",
        config.app_build_vdf_template.display()
    );
    Ok(())
}

fn validate_build_artifacts(build: &BuildArtifacts, app_id: u32) -> Result<()> {
    ensure!(
        build.staged_output_dir.is_dir(),
        "Staged output directory is missing: {}",
        build.staged_output_dir.display()
    );
    ensure!(
        build.executable_path.is_file(),
        "Build executable is missing: {}",
        build.executable_path.display()
    );
    let executable_relative = build
        .executable_path
        .strip_prefix(&build.staged_output_dir)
        .context("Game executable is outside staged_output_dir")?;
    ensure!(
        executable_relative == Path::new(GAME_EXECUTABLE),
        "Expected game executable at the staging root as {GAME_EXECUTABLE}, found {}",
        executable_relative.display()
    );
    ensure!(
        build.steam_appid_path.is_file(),
        "Build SteamAppId.txt is missing: {}",
        build.steam_appid_path.display()
    );
    let appid_relative = build
        .steam_appid_path
        .strip_prefix(&build.staged_output_dir)
        .context("SteamAppId.txt is outside staged_output_dir")?;
    ensure!(
        appid_relative == Path::new(STEAM_APP_ID_FILE),
        "Expected {STEAM_APP_ID_FILE} at the staging root, found {}",
        appid_relative.display()
    );
    ensure!(
        !build.pak_files.is_empty(),
        "BuildArtifacts contains no .pak files"
    );

    for path in std::iter::once(&build.executable_path)
        .chain(std::iter::once(&build.steam_appid_path))
        .chain(build.pak_files.iter())
    {
        ensure!(
            path.strip_prefix(&build.staged_output_dir).is_ok(),
            "Build artifact is outside staged_output_dir: {}",
            path.display()
        );
        ensure!(path.is_file(), "Build artifact is missing: {}", path.display());
    }

    for pak in &build.pak_files {
        ensure!(
            pak.extension().is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("pak")),
            "BuildArtifacts pak_files contains a non-.pak file: {}",
            pak.display()
        );
    }

    let source_app_id = read_app_id(&build.steam_appid_path)?;
    ensure!(
        source_app_id == app_id,
        "Build SteamAppId.txt contains AppID {source_app_id}, expected {app_id}"
    );

    Ok(())
}

fn normalized_depot_ids(config: &Config) -> Result<Vec<u32>> {
    let store: BTreeSet<u32> = config.store_depot_ids.iter().copied().collect();
    let devcomp: BTreeSet<u32> = config.devcomp_depot_ids.iter().copied().collect();

    ensure!(!store.is_empty(), "store_depot_ids must not be empty");
    ensure!(!devcomp.is_empty(), "devcomp_depot_ids must not be empty");
    ensure!(store == devcomp, "Store and DevComp depot ID sets do not match");
    ensure!(
        store.iter().all(|id| *id > 0),
        "Depot IDs must be greater than zero"
    );
    ensure!(
        store.len() == config.store_depot_ids.len(),
        "store_depot_ids contains duplicate IDs"
    );
    ensure!(
        devcomp.len() == config.devcomp_depot_ids.len(),
        "devcomp_depot_ids contains duplicate IDs"
    );

    Ok(store.into_iter().collect())
}

fn resolve_depot_templates(pattern: &str) -> Result<Vec<PathBuf>> {
    let entries = glob(pattern)
        .with_context(|| format!("Invalid depot template glob: {pattern}"))?;
    let mut templates = Vec::new();
    for entry in entries {
        templates.push(entry.with_context(|| {
            format!("Cannot enumerate depot template glob: {pattern}")
        })?);
    }
    templates.sort();
    templates.dedup();

    ensure!(
        !templates.is_empty(),
        "No depot VDF templates matched: {pattern}"
    );
    for path in &templates {
        ensure!(
            path.is_file(),
            "Depot VDF template is missing or not a file: {}",
            path.display()
        );
    }
    Ok(templates)
}

fn assign_depot_templates(
    ids: &[u32],
    templates: &[PathBuf],
) -> Result<HashMap<u32, PathBuf>> {
    ensure!(!templates.is_empty(), "No depot VDF templates supplied");
    ensure!(
        templates.len() == 1 || templates.len() == ids.len(),
        "Expected one reusable depot template or one template per depot ({} depots, {} templates)",
        ids.len(),
        templates.len()
    );

    let mut result = HashMap::new();
    if templates.len() == 1 {
        for id in ids {
            result.insert(*id, templates[0].clone());
        }
        return Ok(result);
    }

    // Prefer explicit filenames like depot_build_123.vdf.j2. If those are not
    // consistently present, pair sorted template paths with sorted IDs.
    let mut explicit = HashMap::new();
    let mut explicit_valid = true;
    for template in templates {
        match depot_id_from_filename(template) {
            Some(id) if ids.contains(&id) && !explicit.contains_key(&id) => {
                explicit.insert(id, template.clone());
            }
            _ => explicit_valid = false,
        }
    }
    if explicit_valid && explicit.len() == ids.len() {
        return Ok(explicit);
    }

    for (id, template) in ids.iter().zip(templates.iter()) {
        result.insert(*id, template.clone());
    }
    Ok(result)
}

fn depot_id_from_filename(path: &Path) -> Option<u32> {
    let name = path.file_name()?.to_str()?;
    let number = name.strip_prefix("depot_build_")?.split('.').next()?;
    number.parse().ok()
}

fn ensure_template_outside_generated_dirs(
    template: &Path,
    content_dir: &Path,
    scripts_dir: &Path,
) -> Result<()> {
    let canonical_template = template
        .canonicalize()
        .with_context(|| format!("Cannot resolve VDF template: {}", template.display()))?;
    ensure!(
        !path_contains(content_dir, &canonical_template)
            && !path_contains(scripts_dir, &canonical_template),
        "VDF template must be outside the generated content/ and scripts/ directories: {}",
        template.display()
    );
    Ok(())
}

fn path_contains(parent: &Path, path: &Path) -> bool {
    path == parent || path.starts_with(parent)
}

fn ensure_clearable_directory(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            ensure!(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "Refusing to clear a path that is not a real directory: {}",
                path.display()
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| {
                format!("Cannot inspect directory before clearing: {}", path.display())
            });
        }
    }
    Ok(())
}

/// Clear directory contents but preserve the directory itself. Symlinked
/// destination roots are rejected to prevent deleting outside ContentBuilder.
fn clear_directory_contents(path: &Path) -> Result<()> {
    ensure_clearable_directory(path)?;
    if !path.exists() {
        fs::create_dir_all(path).with_context(|| {
            format!("Cannot create directory: {}", path.display())
        })?;
    }

    for entry in fs::read_dir(path)
        .with_context(|| format!("Cannot list directory: {}", path.display()))?
    {
        let entry = entry.with_context(|| {
            format!("Cannot read directory entry under: {}", path.display())
        })?;
        let entry_path = entry.path();
        let file_type = entry.file_type().with_context(|| {
            format!("Cannot inspect directory entry: {}", entry_path.display())
        })?;

        if file_type.is_dir() && !file_type.is_symlink() {
            fs::remove_dir_all(&entry_path).with_context(|| {
                format!("Cannot remove stale directory: {}", entry_path.display())
            })?;
        } else {
            fs::remove_file(&entry_path).with_context(|| {
                format!("Cannot remove stale file: {}", entry_path.display())
            })?;
        }
    }
    Ok(())
}

fn ensure_directory(path: &Path) -> Result<()> {
    if path.exists() {
        let metadata = fs::symlink_metadata(path)
            .with_context(|| format!("Cannot inspect directory: {}", path.display()))?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "Expected a real directory: {}",
            path.display()
        );
    } else {
        fs::create_dir_all(path)
            .with_context(|| format!("Cannot create directory: {}", path.display()))?;
    }
    Ok(())
}

fn prepare_content(build: &BuildArtifacts, content_dir: &Path) -> Result<()> {
    let options = CopyOptions::new().content_only(true);
    copy_dir(&build.staged_output_dir, content_dir, &options).with_context(|| {
        format!(
            "Cannot copy staged content from {} to {}",
            build.staged_output_dir.display(),
            content_dir.display()
        )
    })?;
    Ok(())
}

fn remove_bootstrap_launcher(content_dir: &Path) -> Result<()> {
    let launcher = content_dir.join(BOOTSTRAP_LAUNCHER);
    match fs::symlink_metadata(&launcher) {
        Ok(metadata) => {
            ensure!(
                metadata.is_file() || metadata.file_type().is_symlink(),
                "Bootstrap launcher path is not a file: {}",
                launcher.display()
            );
            fs::remove_file(&launcher).with_context(|| {
                format!("Cannot remove bootstrap launcher: {}", launcher.display())
            })?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| {
                format!("Cannot inspect bootstrap launcher: {}", launcher.display())
            });
        }
    }
    match fs::symlink_metadata(&launcher) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => bail!("Bootstrap launcher remains in content root: {}", launcher.display()),
        Err(error) => Err(error).with_context(|| {
            format!("Cannot verify bootstrap launcher removal: {}", launcher.display())
        }),
    }
}

fn verify_copied_build_content(
    build: &BuildArtifacts,
    expected_app_id: u32,
    content_dir: &Path,
) -> Result<()> {
    for source in std::iter::once(&build.executable_path)
        .chain(std::iter::once(&build.steam_appid_path))
        .chain(build.pak_files.iter())
    {
        ensure!(source.is_file(), "Build artifact is missing: {}", source.display());
        let relative = source.strip_prefix(&build.staged_output_dir).with_context(|| {
            format!(
                "Build artifact is outside staged output directory: {}",
                source.display()
            )
        })?;
        let copied = content_dir.join(relative);
        ensure!(
            copied.is_file(),
            "Build artifact was not copied into content: {}",
            copied.display()
        );
    }

    let copied_app_id_path = content_dir.join(
        build.steam_appid_path.strip_prefix(&build.staged_output_dir).with_context(|| {
            format!("SteamAppId.txt is outside staged output: {}", build.steam_appid_path.display())
        })?,
    );
    let copied_app_id = read_app_id(&copied_app_id_path)?;
    ensure!(
        copied_app_id == expected_app_id,
        "Copied SteamAppId.txt contains AppID {copied_app_id}, expected {expected_app_id}"
    );

    let expected_paks = build
        .pak_files
        .iter()
        .map(|path| relative_path_key(&build.staged_output_dir, path))
        .collect::<Result<BTreeSet<_>>>()?;
    ensure!(
        expected_paks.len() == build.pak_files.len(),
        "BuildArtifacts contains duplicate .pak paths"
    );
    let copied_paks = discover_pak_files(content_dir)?
        .iter()
        .map(|path| relative_path_key(content_dir, path))
        .collect::<Result<BTreeSet<_>>>()?;
    ensure!(
        copied_paks == expected_paks,
        "Copied .pak set does not match BuildArtifacts (expected {:?}, found {:?})",
        expected_paks,
        copied_paks
    );

    ensure!(
        !content_dir.join(BOOTSTRAP_LAUNCHER).exists(),
        "Bootstrap launcher must not be present in content root"
    );
    Ok(())
}

fn read_app_id(path: &Path) -> Result<u32> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("Cannot read Steam AppID file: {}", path.display()))?;
    text.trim().parse::<u32>().with_context(|| {
        format!("Steam AppID file does not contain a valid integer: {}", path.display())
    })
}

fn relative_path_key(root: &Path, path: &Path) -> Result<String> {
    let relative = path.strip_prefix(root).with_context(|| {
        format!("Path is outside expected root {}: {}", root.display(), path.display())
    })?;
    Ok(relative.to_string_lossy().replace('\\', "/").to_ascii_lowercase())
}

fn discover_pak_files(root: &Path) -> Result<Vec<PathBuf>> {
    let escaped_root = Pattern::escape(&root.to_string_lossy().replace('\\', "/"));
    let pattern = format!("{escaped_root}/**/*");
    let mut paths = Vec::new();
    for entry in glob(&pattern)
        .with_context(|| format!("Invalid .pak discovery glob: {pattern}"))?
    {
        let path = entry.with_context(|| {
            format!("Cannot enumerate staged .pak files: {pattern}")
        })?;
        if path.is_file()
            && path.extension().is_some_and(|extension| {
                extension.to_string_lossy().eq_ignore_ascii_case("pak")
            })
        {
            paths.push(path);
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn render_template(
    name: &str,
    source: &str,
    values: JinjaValue,
    source_path: &Path,
) -> Result<String> {
    let mut environment = Environment::new();
    environment.set_undefined_behavior(UndefinedBehavior::Strict);
    environment.add_template(name, source).with_context(|| {
        format!("Invalid Jinja VDF template: {}", source_path.display())
    })?;
    environment
        .get_template(name)
        .with_context(|| format!("Cannot load VDF template: {}", source_path.display()))?
        .render(values)
        .with_context(|| format!("Cannot render VDF template: {}", source_path.display()))
}

fn vdf_path_value(path: &Path) -> String {
    escape_vdf_scalar(&path.to_string_lossy().replace('\\', "/"))
}

fn escape_vdf_scalar(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\r', " ")
        .replace('\n', " ")
}

/// Normalize the root AppBuild SetLive directive. Existing top-level entries are
/// removed from rendered output; if enabled, one correctly escaped directive is
/// inserted immediately before the AppBuild object's closing brace. Nested keys
/// named SetLive (if any) are left unchanged.
fn apply_set_live_directive(source: &str, branch: &str, enabled: bool) -> Result<String> {
    ensure!(!branch.trim().is_empty(), "beta_branch must not be empty");

    let mut output = String::with_capacity(source.len() + 64);
    let mut depth: i32 = 0;
    let mut root_closed = false;

    for line in source.lines() {
        let trimmed = line.trim_start();
        let previous_depth = depth;
        let delta = vdf_brace_delta(line);

        // Remove any template-provided AppBuild-level SetLive. This avoids
        // duplicate directives and guarantees false means no active SetLive.
        if depth == 1 {
            if let Some((key, _)) = parse_simple_vdf_pair(line) {
                if key.eq_ignore_ascii_case("SetLive") {
                    continue;
                }
            }
        }

        // The root closing line is encountered with depth 1. Inject the exact
        // directive before it, but only when explicitly configured.
        if previous_depth == 1 && delta < 0 && enabled {
            let indent = &line[..line.len() - trimmed.len()];
            output.push_str(indent);
            output.push_str("    \"SetLive\" \"");
            output.push_str(&escape_vdf_scalar(branch));
            output.push_str("\"\n");
        }

        output.push_str(line);
        output.push('\n');

        depth += delta;
        ensure!(depth >= 0, "Rendered app-build VDF has unbalanced braces");
        if previous_depth > 0 && depth == 0 {
            root_closed = true;
        }
    }

    ensure!(depth == 0, "Rendered app-build VDF has unbalanced braces");
    ensure!(root_closed, "Rendered app-build VDF has no root closing brace");
    Ok(output)
}

/// Count VDF braces outside quoted strings and comments on one line.
fn vdf_brace_delta(line: &str) -> i32 {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") || trimmed.starts_with('#') {
        return 0;
    }

    let mut in_string = false;
    let mut escaped = false;
    let mut delta = 0;

    for ch in line.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '{' => delta += 1,
            '}' => delta -= 1,
            _ => {}
        }
    }

    delta
}

fn write_generated_file(path: &Path, contents: &str) -> Result<()> {
    ensure!(!contents.trim().is_empty(), "Refusing to write empty VDF: {}", path.display());
    ensure!(
        !contents.contains("{{") && !contents.contains("{%") && !contents.contains("{#"),
        "Unresolved Jinja syntax remains in generated VDF: {}",
        path.display()
    );
    fs::write(path, contents)
        .with_context(|| format!("Cannot write generated VDF: {}", path.display()))
}

fn read_parse_vdf(path: &Path) -> Result<(String, Vdf<'static>)> {
    let source = fs::read_to_string(path)
        .with_context(|| format!("Cannot read generated VDF: {}", path.display()))?;
    ensure!(!source.trim().is_empty(), "Generated VDF is empty: {}", path.display());
    let parsed = parse_text(&source)
        .with_context(|| format!("Generated VDF syntax is invalid: {}", path.display()))?
        .into_owned();
    ensure!(parsed.is_obj(), "Generated VDF root is not an object: {}", path.display());
    Ok((source, parsed))
}

fn object_ci<'object, 'text>(
    object: &'object Obj<'text>,
    key: &str,
) -> Option<&'object Obj<'text>> {
    value_ci(object, key).and_then(VdfValue::as_obj)
}

fn value_ci<'object, 'text>(
    object: &'object Obj<'text>,
    key: &str,
) -> Option<&'object VdfValue<'text>> {
    object
        .iter()
        .find_map(|(candidate, value)| candidate.as_ref().eq_ignore_ascii_case(key).then_some(value))
}

fn string_ci<'object, 'text>(
    object: &'object Obj<'text>,
    key: &str,
) -> Option<&'object str> {
    value_ci(object, key).and_then(VdfValue::as_str)
}

fn normalized_vdf_path(value: &str) -> String {
    value
        .trim()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

fn verify_generated_outputs(
    config: &Config,
    build: &BuildArtifacts,
    content_dir: &Path,
    output_dir: &Path,
    app_build_vdf_path: &Path,
    depot_vdf_paths: &[PathBuf],
    depot_ids: &[u32],
) -> Result<()> {
    ensure!(content_dir.is_dir(), "Generated content directory is missing: {}", content_dir.display());
    ensure_directory(output_dir)?;
    ensure!(
        depot_vdf_paths.len() == depot_ids.len(),
        "Generated depot VDF count does not match configured depot count"
    );
    ensure!(
        !depot_vdf_paths.is_empty(),
        "No depot-build VDF files were generated"
    );

    let expected_content_root = normalized_vdf_path(&vdf_path_value(content_dir));
    let expected_build_output = normalized_vdf_path(&vdf_path_value(output_dir));

    let (_app_source, app_vdf) = read_parse_vdf(app_build_vdf_path)?;
    ensure!(
        app_vdf.key().eq_ignore_ascii_case("AppBuild"),
        "App-build VDF root must be AppBuild: {}",
        app_build_vdf_path.display()
    );
    let app_obj = app_vdf.as_obj().context("App-build VDF root must be an object")?;
    let actual_app_id = string_ci(app_obj, "AppID")
        .context("App-build VDF is missing AppID")?
        .parse::<u32>()
        .context("App-build VDF AppID is not an integer")?;
    ensure!(
        actual_app_id == config.app_id,
        "Generated app-build VDF AppID is {actual_app_id}, expected {}",
        config.app_id
    );

    let actual_content_root = string_ci(app_obj, "ContentRoot")
        .context("App-build VDF is missing ContentRoot")?;
    ensure!(
        normalized_vdf_path(actual_content_root) == expected_content_root,
        "App-build VDF ContentRoot does not match generated content directory"
    );
    let actual_build_output = string_ci(app_obj, "BuildOutput")
        .context("App-build VDF is missing BuildOutput")?;
    ensure!(
        normalized_vdf_path(actual_build_output) == expected_build_output,
        "App-build VDF BuildOutput does not match ContentBuilder output directory"
    );

    let actual_set_live_value = value_ci(app_obj, "SetLive");
    if config.set_live_after_upload {
        let actual_set_live = actual_set_live_value
            .and_then(VdfValue::as_str)
            .context("SetLive must be a string when set_live_after_upload=true")?;
        ensure!(
            actual_set_live == config.beta_branch.trim(),
            "SetLive must equal configured beta_branch {:?} when set_live_after_upload=true",
            config.beta_branch
        );
    } else {
        ensure!(
            actual_set_live_value.is_none(),
            "App-build VDF contains SetLive although set_live_after_upload=false; remove it from the template or make it conditional"
        );
    }

    let depots_obj = object_ci(app_obj, "Depots")
        .context("App-build VDF is missing a Depots object")?;
    let mut actual_references = BTreeMap::<u32, String>::new();
    for (raw_id, reference_value) in depots_obj.iter() {
        let depot_id = raw_id.as_ref().parse::<u32>().with_context(|| {
            format!("App-build VDF contains a non-numeric depot key: {raw_id}")
        })?;
        let reference = reference_value.as_str().with_context(|| {
            format!("App-build VDF depot reference {depot_id} must be a string")
        })?;
        ensure!(
            actual_references.insert(depot_id, reference.to_owned()).is_none(),
            "App-build VDF contains duplicate depot ID {depot_id}"
        );
    }

    let expected_ids: BTreeSet<u32> = depot_ids.iter().copied().collect();
    let actual_ids: BTreeSet<u32> = actual_references.keys().copied().collect();
    ensure!(
        actual_ids == expected_ids,
        "App-build VDF depot IDs {:?} do not match configured IDs {:?}",
        actual_ids,
        expected_ids
    );

    for depot_id in depot_ids {
        let expected_filename = format!("depot_build_{depot_id}.vdf");
        let reference = actual_references
            .get(depot_id)
            .with_context(|| format!("App-build VDF is missing depot reference {depot_id}"))?;
        let normalized_reference = reference.replace('\\', "/");
        ensure!(
            normalized_reference.eq_ignore_ascii_case(&expected_filename),
            "App-build VDF depot {depot_id} must reference the sibling script {expected_filename:?}, found {reference:?}"
        );
    }

    let generated_names: BTreeSet<String> = depot_vdf_paths
        .iter()
        .filter_map(|path| path.file_name().map(|name| name.to_string_lossy().to_ascii_lowercase()))
        .collect();
    let expected_names: BTreeSet<String> = depot_ids
        .iter()
        .map(|id| format!("depot_build_{id}.vdf").to_ascii_lowercase())
        .collect();
    ensure!(
        generated_names.len() == depot_vdf_paths.len() && generated_names == expected_names,
        "Generated depot VDF filenames do not exactly match configured depot IDs"
    );

    for (depot_id, path) in depot_ids.iter().zip(depot_vdf_paths.iter()) {
        verify_depot_vdf(path, *depot_id, &expected_content_root)?;
    }

    // Double-check the final tree against BuildArtifacts after all generation.
    verify_copied_build_content(build, config.app_id, content_dir)?;
    Ok(())
}

fn verify_depot_vdf(
    path: &Path,
    expected_depot_id: u32,
    expected_content_root: &str,
) -> Result<()> {
    let (source, parsed) = read_parse_vdf(path)?;
    ensure!(
        parsed.key().eq_ignore_ascii_case("DepotBuildConfig"),
        "Depot VDF root must be DepotBuildConfig: {}",
        path.display()
    );
    let object = parsed.as_obj().context("Depot VDF root must be an object")?;
    let actual_depot_id = string_ci(object, "DepotID")
        .with_context(|| format!("Depot VDF is missing DepotID: {}", path.display()))?
        .parse::<u32>()
        .with_context(|| format!("Depot VDF DepotID is invalid: {}", path.display()))?;
    ensure!(
        actual_depot_id == expected_depot_id,
        "Depot VDF {} contains DepotID {actual_depot_id}, expected {expected_depot_id}",
        path.display()
    );

    let actual_content_root = string_ci(object, "ContentRoot")
        .with_context(|| format!("Depot VDF is missing ContentRoot: {}", path.display()))?;
    ensure!(
        normalized_vdf_path(actual_content_root) == expected_content_root,
        "Depot VDF ContentRoot does not match generated content directory: {}",
        path.display()
    );

    verify_file_exclusions(path, &source)
}

fn verify_file_exclusions(path: &Path, source: &str) -> Result<()> {
    // steam-vdf-parser validates syntax and structured values. Since a VDF
    // object uses last-value-wins for duplicate keys, scan parsed single-line
    // key/value directives as well so all repeated FileExclusion entries count.
    let exclusions: BTreeSet<String> = source
        .lines()
        .filter_map(parse_simple_vdf_pair)
        .filter_map(|(key, value)| {
            key.eq_ignore_ascii_case("FileExclusion")
                .then_some(normalize_exclusion(&value))
        })
        .collect();

    for expected in REQUIRED_EXCLUSIONS {
        ensure!(
            exclusions.contains(&normalize_exclusion(expected)),
            "Depot VDF is missing active FileExclusion {expected:?}: {}",
            path.display()
        );
    }
    Ok(())
}

fn parse_simple_vdf_pair(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
        return None;
    }
    let (key, remainder) = read_quoted_token(trimmed)?;
    let (value, _) = read_quoted_token(remainder.trim_start())?;
    Some((key, value))
}

fn read_quoted_token(input: &str) -> Option<(String, &str)> {
    let input = input.strip_prefix('"')?;
    let mut result = String::new();
    let mut escaped = false;

    for (index, ch) in input.char_indices() {
        if escaped {
            match ch {
                '"' => result.push('"'),
                '\\' => result.push('\\'),
                other => {
                    result.push('\\');
                    result.push(other);
                }
            }
            escaped = false;
        } else {
            match ch {
                '\\' => escaped = true,
                '"' => return Some((result, &input[index + ch.len_utf8()..])),
                other => result.push(other),
            }
        }
    }
    None
}

fn normalize_exclusion(value: &str) -> String {
    value
        .trim()
        .replace('/', "\\")
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vdf_quotes_and_windows_exclusion() {
        let (key, rest) = read_quoted_token(r#""FileExclusion" "Saved\\*""#).unwrap();
        let (value, _) = read_quoted_token(rest.trim_start()).unwrap();
        assert_eq!(key, "FileExclusion");
        assert_eq!(value, r"Saved\*");
    }

    #[test]
    fn ignores_commented_exclusions() {
        let source = r#"
// "FileExclusion" "*.pdb"
"FileExclusion" "*.pdb"
"FileExclusion" "Saved\\*"
"FileExclusion" "Pink_Chocolate_V8.exe"
"#;
        let exclusions: BTreeSet<String> = source
            .lines()
            .filter_map(parse_simple_vdf_pair)
            .filter_map(|(key, value)| {
                key.eq_ignore_ascii_case("FileExclusion")
                    .then_some(normalize_exclusion(&value))
            })
            .collect();
        for expected in REQUIRED_EXCLUSIONS {
            assert!(exclusions.contains(&normalize_exclusion(expected)));
        }
    }

    #[test]
    fn set_live_is_inserted_only_when_enabled() {
        let source = r#""AppBuild"
{
    "AppID" "480"
    "SetLive" "wrong-branch"
    "Depots"
    {
        "1001" "depot_build_1001.vdf"
    }
}"#;

        let enabled = apply_set_live_directive(source, "private-beta", true).unwrap();
        assert_eq!(enabled.matches("\"SetLive\"").count(), 1);
        assert!(enabled.contains(r#""SetLive" "private-beta""#));

        let disabled = apply_set_live_directive(source, "private-beta", false).unwrap();
        assert!(!disabled.contains("SetLive"));
        parse_text(&disabled).unwrap();
    }

    #[test]
    fn app_vdf_is_parsed_and_ids_are_readable() {
        let source = r#""AppBuild"
{
  "AppID" "480"
  "ContentRoot" "C:/ContentBuilder/content"
  "BuildOutput" "C:/ContentBuilder/output"
  "Depots"
  {
    "1001" "depot_build_1001.vdf"
  }
}"#;
        let vdf = parse_text(source).unwrap();
        assert_eq!(vdf.key(), "AppBuild");
        let object = vdf.as_obj().unwrap();
        assert_eq!(string_ci(object, "AppID"), Some("480"));
        assert!(object_ci(object, "Depots").is_some());
    }
}
