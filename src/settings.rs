use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DropArea {
    pub corner: String,
    pub position: Option<[f32; 2]>,
    pub size: [f32; 2],
}
impl Default for DropArea {
    fn default() -> Self {
        Self {
            corner: "bottom-left".into(),
            position: None,
            size: [180.0, 110.0],
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub destination: PathBuf,
    pub edge: String,
    pub reduced_motion: bool,
    pub position: Option<[f32; 2]>,
    pub corner: String,
    pub area_size: [f32; 2],
    pub areas: Vec<DropArea>,
    pub above_focused: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let user = directories::UserDirs::new();
        let destination = user
            .as_ref()
            .and_then(|u| u.download_dir())
            .or_else(|| user.as_ref().map(|u| u.home_dir()))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        Self {
            destination,
            edge: "right".into(),
            reduced_motion: false,
            position: None,
            corner: "bottom-left".into(),
            area_size: [180.0, 110.0],
            areas: Vec::new(),
            above_focused: false,
        }
    }
}

impl Settings {
    pub fn path() -> PathBuf {
        if let Some(path) =
            std::env::var_os("DROPFLIP_CONFIG").or_else(|| std::env::var_os("DROPFORM_CONFIG"))
        {
            return path.into();
        }
        directories::ProjectDirs::from("", "Dropflip", "Dropflip")
            .map(|d| d.config_dir().join("settings.json"))
            .unwrap_or_else(|| PathBuf::from("dropflip-settings.json"))
    }
    pub fn load() -> Self {
        let current = Self::path();
        let custom = std::env::var_os("DROPFLIP_CONFIG")
            .or_else(|| std::env::var_os("DROPFORM_CONFIG"))
            .is_some();
        let legacy = directories::ProjectDirs::from("", "Dropform", "Dropform")
            .map(|d| d.config_dir().join("settings.json"));
        let source = if !custom && !current.exists() {
            legacy
                .filter(|p| p.is_file())
                .unwrap_or_else(|| current.clone())
        } else {
            current.clone()
        };
        let stored: Option<Self> = std::fs::read(&source)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok());
        let migrate = source != current && stored.is_some();
        let mut s = stored.unwrap_or_default();
        if !matches!(s.edge.as_str(), "left" | "right") {
            s.edge = "right".into();
        }
        if s.position.is_some_and(|p| p.iter().any(|v| !v.is_finite())) {
            s.position = None;
        }
        if !matches!(
            s.corner.as_str(),
            "bottom-left" | "bottom-right" | "top-left" | "top-right"
        ) {
            s.corner = "bottom-left".into();
        }
        for value in &mut s.area_size {
            if !value.is_finite() {
                *value = 150.0;
            }
            *value = value.clamp(100.0, 320.0);
        }
        if s.areas.is_empty() {
            s.areas.push(DropArea {
                corner: s.corner.clone(),
                position: s.position,
                size: s.area_size,
            });
        }
        for area in &mut s.areas {
            if !matches!(
                area.corner.as_str(),
                "bottom-left" | "bottom-right" | "top-left" | "top-right"
            ) {
                area.corner = "bottom-left".into();
            }
            if area
                .position
                .is_some_and(|p| p.iter().any(|n| !n.is_finite()))
            {
                area.position = None;
            }
            for (i, n) in area.size.iter_mut().enumerate() {
                if !n.is_finite() {
                    *n = if i == 0 { 180.0 } else { 110.0 };
                }
                *n = n.clamp(
                    if i == 0 { 100.0 } else { 80.0 },
                    if i == 0 { 1600.0 } else { 1000.0 },
                );
            }
        }
        if migrate {
            let _ = s.save();
        }
        s
    }
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut temp =
            tempfile::NamedTempFile::new_in(path.parent().unwrap_or(std::path::Path::new(".")))?;
        use std::io::Write;
        temp.write_all(&serde_json::to_vec_pretty(self)?)?;
        temp.persist(path)?;
        Ok(())
    }
    pub fn sync_legacy(&mut self) {
        if let Some(area) = self.areas.first() {
            self.corner = area.corner.clone();
            self.position = area.position;
            self.area_size = area.size;
        }
    }
}
