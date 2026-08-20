//! Local content-plugin manifests.
//!
//! The host owns tracking, rendering, and composition. Character and
//! background plugins own the replaceable assets and theme policy, with all
//! paths resolved relative to their manifest so a plugin is self-contained.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;

use crate::compositor::BackgroundMode;

const PLUGIN_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterRenderConfig {
    pub max_texture_dimension: u32,
    pub fov_y_degrees: f32,
    pub anchor_height_ratio: f32,
    pub camera_distance: f32,
    pub split_camera_distance: f32,
}

impl Default for CharacterRenderConfig {
    fn default() -> Self {
        Self {
            max_texture_dimension: 2048,
            fov_y_degrees: 40.0,
            anchor_height_ratio: 0.72,
            camera_distance: 1.0,
            split_camera_distance: 1.35,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CharacterPlugin {
    pub id: String,
    pub name: String,
    pub model_path: PathBuf,
    pub idle_animation_path: PathBuf,
    pub policy_bundle_path: PathBuf,
    pub render: CharacterRenderConfig,
}

#[derive(Clone, Debug)]
pub struct BackgroundPlugin {
    pub id: String,
    pub name: String,
    pub shader_source: String,
    pub default_mode: BackgroundMode,
    pub clean_plate_delay: Duration,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CharacterManifest {
    schema_version: u32,
    kind: String,
    id: String,
    name: String,
    model: String,
    idle_animation: String,
    policy: CharacterPolicyManifest,
    #[serde(default)]
    render: CharacterRenderManifest,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CharacterPolicyManifest {
    /// TypeScript entry used by the build tool. The native runtime consumes
    /// only the built bundle but accepts this field as part of one manifest.
    entry: String,
    bundle: String,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct CharacterRenderManifest {
    max_texture_dimension: u32,
    fov_y_degrees: f32,
    anchor_height_ratio: f32,
    camera_distance: f32,
    split_camera_distance: f32,
}

impl Default for CharacterRenderManifest {
    fn default() -> Self {
        let defaults = CharacterRenderConfig::default();
        Self {
            max_texture_dimension: defaults.max_texture_dimension,
            fov_y_degrees: defaults.fov_y_degrees,
            anchor_height_ratio: defaults.anchor_height_ratio,
            camera_distance: defaults.camera_distance,
            split_camera_distance: defaults.split_camera_distance,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BackgroundManifest {
    schema_version: u32,
    kind: String,
    id: String,
    name: String,
    shader: String,
    default_mode: String,
    #[serde(default = "default_clean_plate_delay")]
    clean_plate_delay_seconds: f32,
}

#[derive(Debug, Deserialize)]
struct PluginHeader {
    schema_version: u32,
    kind: String,
    id: String,
}

fn default_clean_plate_delay() -> f32 {
    3.0
}

pub fn load_character_plugin(manifest_path: &Path) -> Result<CharacterPlugin> {
    validate_manifest_kind(manifest_path, "character")?;
    let manifest: CharacterManifest = read_manifest(manifest_path)?;
    validate_header(
        manifest_path,
        manifest.schema_version,
        &manifest.kind,
        "character",
        &manifest.id,
    )?;
    ensure!(
        !manifest.name.trim().is_empty(),
        "character plugin name cannot be empty"
    );
    ensure!(
        !manifest.policy.entry.trim().is_empty(),
        "character plugin policy.entry cannot be empty"
    );

    let render = CharacterRenderConfig {
        max_texture_dimension: manifest.render.max_texture_dimension,
        fov_y_degrees: manifest.render.fov_y_degrees,
        anchor_height_ratio: manifest.render.anchor_height_ratio,
        camera_distance: manifest.render.camera_distance,
        split_camera_distance: manifest.render.split_camera_distance,
    };
    validate_character_render(render)?;

    Ok(CharacterPlugin {
        id: manifest.id,
        name: manifest.name,
        model_path: required_file(manifest_path, &manifest.model, "model")?,
        idle_animation_path: required_file(
            manifest_path,
            &manifest.idle_animation,
            "idle_animation",
        )?,
        policy_bundle_path: required_file(manifest_path, &manifest.policy.bundle, "policy.bundle")?,
        render,
    })
}

pub fn load_background_plugin(manifest_path: &Path) -> Result<BackgroundPlugin> {
    validate_manifest_kind(manifest_path, "background")?;
    let manifest: BackgroundManifest = read_manifest(manifest_path)?;
    validate_header(
        manifest_path,
        manifest.schema_version,
        &manifest.kind,
        "background",
        &manifest.id,
    )?;
    ensure!(
        !manifest.name.trim().is_empty(),
        "background plugin name cannot be empty"
    );
    ensure!(
        manifest.clean_plate_delay_seconds.is_finite() && manifest.clean_plate_delay_seconds >= 0.0,
        "background plugin clean_plate_delay_seconds must be finite and non-negative"
    );
    let default_mode = BackgroundMode::parse(&manifest.default_mode).ok_or_else(|| {
        anyhow::anyhow!(
            "background plugin default_mode '{}' is invalid; expected transparent, virtual, camera, matte, clean, or split",
            manifest.default_mode
        )
    })?;
    let shader_path = required_file(manifest_path, &manifest.shader, "shader")?;
    let shader_source = fs::read_to_string(&shader_path)
        .with_context(|| format!("reading background shader {}", shader_path.display()))?;
    ensure!(
        shader_source.contains("fn plugin_background("),
        "background shader {} must define fn plugin_background(uv: vec2f, time: f32) -> vec3f",
        shader_path.display()
    );

    Ok(BackgroundPlugin {
        id: manifest.id,
        name: manifest.name,
        shader_source,
        default_mode,
        clean_plate_delay: Duration::from_secs_f32(manifest.clean_plate_delay_seconds),
    })
}

fn read_manifest<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let bytes =
        fs::read(path).with_context(|| format!("reading plugin manifest {}", path.display()))?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("parsing plugin manifest {}", path.display()))
}

fn validate_manifest_kind(path: &Path, expected_kind: &str) -> Result<()> {
    let header: PluginHeader = read_manifest(path)?;
    validate_header(
        path,
        header.schema_version,
        &header.kind,
        expected_kind,
        &header.id,
    )
}

fn validate_header(
    path: &Path,
    schema_version: u32,
    actual_kind: &str,
    expected_kind: &str,
    id: &str,
) -> Result<()> {
    ensure!(
        schema_version == PLUGIN_SCHEMA_VERSION,
        "plugin {} uses schema_version {}; expected {}",
        path.display(),
        schema_version,
        PLUGIN_SCHEMA_VERSION
    );
    ensure!(
        actual_kind == expected_kind,
        "plugin {} has kind '{}'; expected '{}'",
        path.display(),
        actual_kind,
        expected_kind
    );
    ensure!(!id.trim().is_empty(), "plugin id cannot be empty");
    ensure!(
        id.bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')),
        "plugin id '{id}' may contain only ASCII letters, digits, '-', '_', and '.'"
    );
    Ok(())
}

fn required_file(manifest_path: &Path, value: &str, field: &str) -> Result<PathBuf> {
    if value.trim().is_empty() {
        bail!("plugin field {field} cannot be empty");
    }
    let value = Path::new(value);
    let path = if value.is_absolute() {
        value.to_owned()
    } else {
        manifest_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(value)
    };
    ensure!(
        path.is_file(),
        "plugin field {field} does not resolve to a file: {}",
        path.display()
    );
    Ok(path)
}

fn validate_character_render(config: CharacterRenderConfig) -> Result<()> {
    ensure!(
        (256..=8192).contains(&config.max_texture_dimension),
        "character render.max_texture_dimension must be within 256..=8192"
    );
    ensure!(
        config.fov_y_degrees.is_finite() && (10.0..=100.0).contains(&config.fov_y_degrees),
        "character render.fov_y_degrees must be within 10..=100"
    );
    ensure!(
        config.anchor_height_ratio.is_finite() && (0.0..=1.0).contains(&config.anchor_height_ratio),
        "character render.anchor_height_ratio must be within 0..=1"
    );
    for (name, distance) in [
        ("camera_distance", config.camera_distance),
        ("split_camera_distance", config.split_camera_distance),
    ] {
        ensure!(
            distance.is_finite() && (0.1..=20.0).contains(&distance),
            "character render.{name} must be within 0.1..=20"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn character_render_defaults_preserve_the_existing_framing() {
        assert_eq!(
            CharacterRenderConfig::default(),
            CharacterRenderConfig {
                max_texture_dimension: 2048,
                fov_y_degrees: 40.0,
                anchor_height_ratio: 0.72,
                camera_distance: 1.0,
                split_camera_distance: 1.35,
            }
        );
    }

    #[test]
    fn plugin_ids_are_safe_and_kinds_are_separate() {
        assert!(validate_header(Path::new("x"), 1, "character", "character", "hero.v1").is_ok());
        assert!(validate_header(Path::new("x"), 1, "background", "character", "hero").is_err());
        assert!(validate_header(Path::new("x"), 1, "character", "character", "../hero").is_err());
    }

    #[test]
    fn background_shader_contract_is_explicit() {
        let valid =
            "fn plugin_background(uv: vec2f, time: f32) -> vec3f { return vec3f(uv, time); }";
        assert!(valid.contains("fn plugin_background("));
        assert!(!"fn main() {}".contains("fn plugin_background("));
    }
}
