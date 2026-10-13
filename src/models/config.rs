use anyhow::{Context, Result};
use serde::Deserialize;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

/// Deployment settings. Relative paths resolve against the directory containing deploy.toml.
#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub engine_root: PathBuf,
    pub uproject: PathBuf,
    pub uat_bat: PathBuf,

    pub steamworks_sdk_dir: PathBuf,
    pub steamcmd_exe: PathBuf,
    /// Steam ContentBuilder root, containing builder/, content/, scripts/, and output/.
    pub content_builder_dir: PathBuf,

    pub msvc_cl_exe: PathBuf,
    /// Expected substring in `cl.exe /Bv` output, matching the installed toolchain.
    pub msvc_compiler_version_prefix: String,
    /// Source file that requires a C++23 feature.
    pub cxx23_probe: PathBuf,

    /// App-build VDF template; Validate checks existence only.
    pub app_build_vdf_template: PathBuf,
    /// Glob for depot VDF templates, e.g. `templates/depot_build*.vdf.j2`.
    pub depot_vdf_template_glob: String,

    pub app_id: u32,
    pub store_depot_ids: Vec<u32>,
    pub devcomp_depot_ids: Vec<u32>,
    pub beta_branch: String,
    /// Human-readable SteamPipe build description.
    #[serde(default = "default_build_description")]
    pub build_description: String,
    /// If true, generated AppBuild VDF explicitly sets the configured beta branch live.
    #[serde(default)]
    pub set_live_after_upload: bool,
}

impl Config {
    /// Load TOML and resolve configured paths relative to the config file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let supplied_path = path.as_ref();
        let config_path = if supplied_path.is_absolute() {
            supplied_path.to_path_buf()
        } else {
            env::current_dir()
                .context("Cannot determine current working directory")?
                .join(supplied_path)
        };

        let text = fs::read_to_string(&config_path)
            .with_context(|| format!("Cannot read config: {}", config_path.display()))?;
        let mut config: Self = toml::from_str(&text)
            .with_context(|| format!("Invalid TOML config: {}", config_path.display()))?;

        let base_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
        config.resolve_paths(base_dir);
        Ok(config)
    }

    fn resolve_paths(&mut self, base_dir: &Path) {
        resolve_path(base_dir, &mut self.engine_root);
        resolve_path(base_dir, &mut self.uproject);
        resolve_path(base_dir, &mut self.uat_bat);
        resolve_path(base_dir, &mut self.steamworks_sdk_dir);
        resolve_path(base_dir, &mut self.steamcmd_exe);
        resolve_path(base_dir, &mut self.content_builder_dir);
        resolve_path(base_dir, &mut self.msvc_cl_exe);
        resolve_path(base_dir, &mut self.cxx23_probe);
        resolve_path(base_dir, &mut self.app_build_vdf_template);

        let pattern = Path::new(&self.depot_vdf_template_glob);
        let resolved = if pattern.is_absolute() {
            pattern.to_path_buf()
        } else {
            base_dir.join(pattern)
        };

        // Forward slashes are accepted by glob on Windows.
        self.depot_vdf_template_glob = resolved.to_string_lossy().replace('\\', "/");
    }
}

fn default_build_description() -> String {
    "PinkChoco Win64 Shipping".to_owned()
}

fn resolve_path(base_dir: &Path, path: &mut PathBuf) {
    if path.is_relative() {
        *path = base_dir.join(&*path);
    }
}
