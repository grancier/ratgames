//! Resolve authored mode/profile IDs into complete campaigns before gameplay.
//! This boundary accepts parsed values; filesystem access stays in the CLI loader.

use std::collections::{BTreeMap, BTreeSet};

use mathgame_app::{Arithmetic, MathLevel, MathgameCampaign, ProblemSpec};
use ratgames::LevelConfig;

use super::{AppConfig, AppConfigError, DifficultyPreset};

/// New profile objects are strict; existing config and problem fields retain
/// their permissive parsing contract for compatibility.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DifficultyProfile {
    pub label: String,
    pub problems: Vec<ProblemSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct AuthoredArithmetic {
    #[serde(flatten)]
    pub arithmetic: Arithmetic,
    /// Empty keeps inline arithmetic for every mode. A mapped level must name
    /// a profile for every selected preset that has a stable mode ID.
    #[serde(default)]
    pub profiles_by_mode: BTreeMap<String, String>,
}

pub type AuthoredLevel = LevelConfig<AuthoredArithmetic>;

#[derive(Debug)]
pub struct PreparedDifficulty {
    pub label: String,
    pub campaign: MathgameCampaign,
}

#[derive(Debug)]
pub struct PreparedCampaigns {
    pub initial: MathgameCampaign,
    pub difficulties: Vec<PreparedDifficulty>,
}

pub(super) fn inline_levels(levels: &[AuthoredLevel]) -> Vec<MathLevel> {
    levels
        .iter()
        .map(|level| MathLevel {
            name: level.name.clone(),
            difficulty: level.difficulty.clone(),
            rules: level.rules,
            content: level.content.arithmetic.clone(),
        })
        .collect()
}

impl AppConfig {
    pub(super) fn validate_profiles(&self) -> Result<(), AppConfigError> {
        let mut ids = BTreeSet::new();
        for (index, preset) in self.difficulties.iter().enumerate() {
            if let Some(id) = &preset.id
                && (id.trim().is_empty() || id.trim() != id || !ids.insert(id))
            {
                return Err(AppConfigError::Invalid(format!(
                    "difficulties[{index}].id {id:?}: IDs must be nonblank, unique, and have no surrounding whitespace"
                )));
            }
        }
        for (id, profile) in &self.difficulty_profiles {
            if id.trim().is_empty() || id.trim() != id {
                return Err(AppConfigError::Invalid(format!(
                    "difficulty_profiles: invalid profile ID {id:?}"
                )));
            }
            if profile.label.trim().is_empty() {
                return Err(AppConfigError::Invalid(format!(
                    "difficulty_profiles[{id:?}].label must not be blank"
                )));
            }
            Arithmetic {
                problems: profile.problems.clone(),
            }
            .generator(id)
            .map_err(|error| {
                AppConfigError::Invalid(format!("difficulty_profiles[{id:?}].problems: {error:?}"))
            })?;
        }
        Ok(())
    }

    /// Finish the config boundary: build every selectable campaign so menu
    /// selection is infallible. Labels never participate in identity lookup.
    pub fn prepare_campaigns(
        &self,
        levels: &[AuthoredLevel],
    ) -> Result<PreparedCampaigns, AppConfigError> {
        self.validate()?;
        let inline = inline_levels(levels);
        let initial = self.compile_campaign(&inline, self.starting_lives, "inline levels")?;
        let difficulties = self
            .difficulties
            .iter()
            .enumerate()
            .map(|(index, preset)| {
                let mut selected = scaled_levels(&inline, preset.time_percent);
                for (level_index, (authored, selected)) in
                    levels.iter().zip(&mut selected).enumerate()
                {
                    self.apply_profile(authored, selected, preset, level_index)?;
                }
                Ok(PreparedDifficulty {
                    label: preset.label.clone(),
                    campaign: self.compile_campaign(
                        &selected,
                        preset.starting_lives,
                        &format!("difficulties[{index}]"),
                    )?,
                })
            })
            .collect::<Result<_, AppConfigError>>()?;
        Ok(PreparedCampaigns {
            initial,
            difficulties,
        })
    }

    fn apply_profile(
        &self,
        authored: &AuthoredLevel,
        selected: &mut MathLevel,
        preset: &DifficultyPreset,
        index: usize,
    ) -> Result<(), AppConfigError> {
        let Some(mode) = &preset.id else {
            return Ok(());
        };
        if authored.content.profiles_by_mode.is_empty() {
            return Ok(());
        }
        let context = format!(
            "levels[{index}] ({:?}).profiles_by_mode[{mode:?}]",
            authored.name
        );
        let id = authored.content.profiles_by_mode.get(mode).ok_or_else(|| {
            AppConfigError::Invalid(format!("{context}: missing profile mapping"))
        })?;
        let profile = self
            .difficulty_profiles
            .get(id)
            .ok_or_else(|| AppConfigError::Invalid(format!("{context}: unknown profile {id:?}")))?;
        selected.difficulty.clone_from(&profile.label);
        selected.content.problems.clone_from(&profile.problems);
        Ok(())
    }

    fn compile_campaign(
        &self,
        levels: &[MathLevel],
        lives: u32,
        context: &str,
    ) -> Result<MathgameCampaign, AppConfigError> {
        MathgameCampaign::from_levels(levels, lives)
            .and_then(|campaign| campaign.with_scoring(self.scoring.clone()))
            .map(|campaign| campaign.with_continues(self.continues))
            .map_err(|error| AppConfigError::Invalid(format!("{context}: {error}")))
    }
}

/// Preserve the legacy time multiplier, including untimed levels and saturation.
fn scaled_levels(levels: &[MathLevel], time_percent: u32) -> Vec<MathLevel> {
    levels
        .iter()
        .map(|level| {
            let mut level = level.clone();
            let scaled = u64::from(level.rules.time_limit_frames) * u64::from(time_percent) / 100;
            level.rules.time_limit_frames = u32::try_from(scaled).unwrap_or(u32::MAX);
            level
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scaled_levels_scale_only_the_authored_time_limits() {
        use mathgame_app::{Arithmetic, OperatorConfig, ProblemSpec};
        use ratgames::LevelSpec;

        let level = |frames: u32| MathLevel {
            name: "L".to_string(),
            difficulty: "EASY".to_string(),
            rules: LevelSpec {
                time_limit_frames: frames,
                ..LevelSpec::default()
            },
            content: Arithmetic {
                problems: vec![ProblemSpec {
                    operator: OperatorConfig::Add,
                    min: 0,
                    max: 9,
                    max_distance: None,
                    weight: 1,
                    multiplier_min: None,
                    multiplier_max: None,
                    denominator_min: None,
                    denominator_max: None,
                }],
            },
        };
        let levels = vec![level(600), level(0), level(u32::MAX)];

        let easier = scaled_levels(&levels, 150);
        assert_eq!(easier[0].rules.time_limit_frames, 900);
        assert_eq!(
            easier[1].rules.time_limit_frames, 0,
            "untimed stays untimed"
        );
        assert_eq!(easier[2].rules.time_limit_frames, u32::MAX, "saturates");

        let harder = scaled_levels(&levels, 75);
        assert_eq!(harder[0].rules.time_limit_frames, 450);

        let as_authored = scaled_levels(&levels, 100);
        assert_eq!(as_authored[0].rules.time_limit_frames, 600);
        // Everything but the time limit is untouched.
        assert_eq!(as_authored[0].name, "L");
        assert_eq!(
            as_authored[0].rules.required_successes,
            levels[0].rules.required_successes
        );
    }
}
