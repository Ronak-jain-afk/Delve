use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DelveConfig {
    #[serde(default)]
    pub thresholds: Thresholds,
    #[serde(default)]
    pub weights: Weights,
    #[serde(default)]
    pub ignore: Vec<String>,
    #[serde(default)]
    pub entry_points: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Thresholds {
    #[serde(default = "default_warning_lines")]
    pub warning_lines: usize,
    #[serde(default = "default_critical_lines")]
    pub critical_lines: usize,
    #[serde(default = "default_warning_complexity")]
    pub warning_complexity: usize,
    #[serde(default = "default_critical_complexity")]
    pub critical_complexity: usize,
    #[serde(default = "default_jaccard_threshold")]
    pub jaccard_threshold: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Weights {
    #[serde(default = "default_unused_file")]
    pub unused_file: usize,
    #[serde(default = "default_giant_critical")]
    pub giant_critical: usize,
    #[serde(default = "default_giant_warning")]
    pub giant_warning: usize,
    #[serde(default = "default_duplicate")]
    pub duplicate: usize,
    #[serde(default = "default_any_type")]
    pub any_type: usize,
    #[serde(default = "default_console_log")]
    pub console_log: usize,
    #[serde(default = "default_circular_dep")]
    pub circular_dep: usize,
}

const fn default_warning_lines() -> usize { 40 }
const fn default_critical_lines() -> usize { 80 }
const fn default_warning_complexity() -> usize { 10 }
const fn default_critical_complexity() -> usize { 20 }
const fn default_jaccard_threshold() -> f64 { 0.7 }
const fn default_unused_file() -> usize { 10 }
const fn default_giant_critical() -> usize { 3 }
const fn default_giant_warning() -> usize { 1 }
const fn default_duplicate() -> usize { 1 }
const fn default_any_type() -> usize { 1 }
const fn default_console_log() -> usize { 1 }
const fn default_circular_dep() -> usize { 5 }

impl Default for Thresholds {
    fn default() -> Self {
        Thresholds {
            warning_lines: default_warning_lines(),
            critical_lines: default_critical_lines(),
            warning_complexity: default_warning_complexity(),
            critical_complexity: default_critical_complexity(),
            jaccard_threshold: default_jaccard_threshold(),
        }
    }
}

impl Default for Weights {
    fn default() -> Self {
        Weights {
            unused_file: default_unused_file(),
            giant_critical: default_giant_critical(),
            giant_warning: default_giant_warning(),
            duplicate: default_duplicate(),
            any_type: default_any_type(),
            console_log: default_console_log(),
            circular_dep: default_circular_dep(),
        }
    }
}

impl Default for DelveConfig {
    fn default() -> Self {
        DelveConfig {
            thresholds: Thresholds::default(),
            weights: Weights::default(),
            ignore: Vec::new(),
            entry_points: Vec::new(),
        }
    }
}

impl DelveConfig {
    pub fn load(config_path: Option<&Path>, project_root: &Path) -> Self {
        let path = config_path.map(|p| p.to_path_buf()).unwrap_or_else(|| {
            project_root.join(".delve.json")
        });

        if !path.exists() {
            return DelveConfig::default();
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Warning: failed to read config file {}: {}", path.display(), e);
                return DelveConfig::default();
            }
        };

        match serde_json::from_str::<DelveConfig>(&content) {
            Ok(config) => config,
            Err(e) => {
                eprintln!("Warning: invalid config file {}: {}", path.display(), e);
                DelveConfig::default()
            }
        }
    }
}
