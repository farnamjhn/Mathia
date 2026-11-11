//! The decision model: which hosted model reviews the detected blocks, and the
//! API key it needs.
//!
//! The settings live in the app data directory. The key is stored in its own
//! file, readable by the user only, and is never sent to the frontend: the
//! frontend can set it, clear it, and learn whether one exists (plus its last
//! four characters so the user can tell which one it is).

use std::path::{Path, PathBuf};

use math_judge::{ApiConfig, ApiProvider};
use serde::{Deserialize, Serialize};

const SETTINGS_FILE: &str = "decision-model.json";
const KEY_FILE: &str = "decision-model.key";
pub const CACHE_FILE: &str = "decision-cache.json";

/// What the user configures besides the key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct DecisionSettings {
    pub provider: ApiProvider,
    /// Empty means the provider's own address.
    pub base_url: String,
    pub model: String,
}

impl Default for DecisionSettings {
    fn default() -> Self {
        DecisionSettings {
            provider: ApiProvider::Anthropic,
            base_url: String::new(),
            model: String::new(),
        }
    }
}

impl DecisionSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.model.trim().is_empty() {
            return Err("enter the model name".into());
        }
        let url = self.base_url.trim();
        if !url.is_empty() && !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(
                "the API address must start with https:// (or http:// for a local server)".into(),
            );
        }
        Ok(())
    }
}

/// What the frontend sees: the settings and whether a key is stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionView {
    #[serde(flatten)]
    pub settings: DecisionSettings,
    pub has_key: bool,
    /// Last four characters of the key, e.g. `…a1b2`.
    pub key_hint: Option<String>,
    /// A model is set and the key (if the provider needs one) is present.
    pub ready: bool,
}

/// How the frontend changes the key.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyChange {
    Keep,
    Clear,
    Set(String),
}

#[derive(Debug, Default, Clone)]
pub struct Decision {
    pub settings: DecisionSettings,
    key: String,
}

fn only_user(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    let _ = path;
}

impl Decision {
    pub fn load(dir: &Path) -> Self {
        let settings = std::fs::read_to_string(dir.join(SETTINGS_FILE))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let key = std::fs::read_to_string(dir.join(KEY_FILE))
            .map(|k| k.trim().to_string())
            .unwrap_or_default();
        Decision { settings, key }
    }

    pub fn save(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let json = serde_json::to_string_pretty(&self.settings).map_err(|e| e.to_string())?;
        let file = dir.join(SETTINGS_FILE);
        std::fs::write(&file, json + "\n").map_err(|e| format!("{}: {e}", file.display()))?;
        let key_file = dir.join(KEY_FILE);
        if self.key.is_empty() {
            match std::fs::remove_file(&key_file) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("{}: {e}", key_file.display())),
            }
        } else {
            // Create the file closed to others before the key goes in.
            std::fs::write(&key_file, "").map_err(|e| format!("{}: {e}", key_file.display()))?;
            only_user(&key_file);
            std::fs::write(&key_file, &self.key)
                .map_err(|e| format!("{}: {e}", key_file.display()))?;
        }
        Ok(())
    }

    pub fn update(&mut self, settings: DecisionSettings, key: KeyChange) -> Result<(), String> {
        settings.validate()?;
        match key {
            KeyChange::Keep => {}
            KeyChange::Clear => self.key.clear(),
            KeyChange::Set(k) => {
                let k = k.trim();
                if k.chars().any(|c| c.is_whitespace() || c.is_control()) {
                    return Err("the API key has spaces or line breaks in it".into());
                }
                self.key = k.to_string();
            }
        }
        self.settings = DecisionSettings {
            base_url: settings.base_url.trim().trim_end_matches('/').to_string(),
            model: settings.model.trim().to_string(),
            ..settings
        };
        Ok(())
    }

    /// A copy with `settings` and `key` applied, for trying them out before
    /// they are saved.
    pub fn trial(&self, settings: DecisionSettings, key: KeyChange) -> Result<Decision, String> {
        let mut copy = self.clone();
        copy.update(settings, key)?;
        Ok(copy)
    }

    pub fn view(&self) -> DecisionView {
        let chars: Vec<char> = self.key.chars().collect();
        let has_key = !chars.is_empty();
        let local = self.settings.provider == ApiProvider::OpenAiCompatible
            && !self.settings.base_url.is_empty();
        DecisionView {
            settings: self.settings.clone(),
            has_key,
            key_hint: has_key.then(|| {
                format!(
                    "…{}",
                    chars[chars.len().saturating_sub(4)..]
                        .iter()
                        .collect::<String>()
                )
            }),
            // A custom OpenAI-compatible address (Ollama, LM Studio) may need no key.
            ready: self.settings.validate().is_ok() && (has_key || local),
        }
    }

    pub fn api_config(&self) -> Result<ApiConfig, String> {
        let view = self.view();
        if !view.ready {
            return Err("set up the decision model first: Options → Decision model…".into());
        }
        Ok(ApiConfig {
            provider: self.settings.provider,
            base_url: self.settings.base_url.clone(),
            model: self.settings.model.clone(),
            api_key: self.key.clone(),
        })
    }
}

pub fn cache_path(dir: &Path) -> PathBuf {
    dir.join(CACHE_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mathia-decision-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn key_round_trips_and_stays_out_of_the_view() {
        let dir = dir();
        let mut decision = Decision::default();
        assert!(!decision.view().ready);
        assert!(decision.api_config().is_err());
        let settings = DecisionSettings {
            model: " claude-haiku-5-5 ".into(),
            ..Default::default()
        };
        decision
            .update(settings.clone(), KeyChange::Set("sk-ant-abcd1234".into()))
            .unwrap();
        let view = decision.view();
        assert!(view.ready);
        assert_eq!(view.key_hint.as_deref(), Some("…1234"));
        assert!(!serde_json::to_string(&view).unwrap().contains("sk-ant"));
        decision.save(&dir).unwrap();

        let loaded = Decision::load(&dir);
        assert_eq!(loaded.settings.model, "claude-haiku-5-5");
        assert_eq!(loaded.api_config().unwrap().api_key, "sk-ant-abcd1234");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join(KEY_FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o077, 0);
        }

        let mut next = loaded;
        next.update(settings.clone(), KeyChange::Keep).unwrap();
        assert!(next.view().has_key);
        next.update(settings, KeyChange::Clear).unwrap();
        next.save(&dir).unwrap();
        assert!(!Decision::load(&dir).view().has_key);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn bad_input_is_refused() {
        let mut decision = Decision::default();
        assert!(
            decision
                .update(DecisionSettings::default(), KeyChange::Keep)
                .is_err()
        );
        let bad_url = DecisionSettings {
            model: "m".into(),
            base_url: "ftp://x".into(),
            ..Default::default()
        };
        assert!(decision.update(bad_url, KeyChange::Keep).is_err());
        let ok = DecisionSettings {
            model: "m".into(),
            ..Default::default()
        };
        assert!(
            decision
                .update(ok.clone(), KeyChange::Set("a b".into()))
                .is_err()
        );
        // A local OpenAI-compatible server needs no key.
        let local = DecisionSettings {
            provider: ApiProvider::OpenAiCompatible,
            base_url: "http://localhost:11434/v1/".into(),
            model: "llama3".into(),
        };
        decision.update(local, KeyChange::Keep).unwrap();
        assert!(decision.view().ready);
        assert_eq!(decision.settings.base_url, "http://localhost:11434/v1");
    }
}
