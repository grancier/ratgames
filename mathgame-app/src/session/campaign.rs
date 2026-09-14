//! Validate and compile the immutable content shared by independent runs.
use mathgame_core::{GeneratorError, Rng};
use ratgames::{Campaign, ContinueRules, GameRun, ScoringRules};
use std::rc::Rc;

use super::{Level, MathgameSession, MathgameSessionError, make_problem};
use crate::MathLevel;

/// A validated campaign whose immutable generators can be shared by fresh runs.
/// Construction is fallible; starting a run with an explicit seed is infallible.
#[derive(Debug)]
pub struct MathgameCampaign {
    game_run: GameRun,
    levels: Rc<[Level]>,
}

impl MathgameCampaign {
    /// Compile every level and validate the run's goals, modes, and lives.
    ///
    /// # Errors
    /// [`MathgameSessionError`] for an invalid generator or campaign.
    pub fn from_levels(
        levels: &[MathLevel],
        starting_lives: u32,
    ) -> Result<Self, MathgameSessionError> {
        let built = levels
            .iter()
            .map(|config| {
                Ok(Level {
                    generator: config.content.generator(&config.name)?,
                    name: config.name.clone(),
                    difficulty: config.difficulty.clone(),
                })
            })
            .collect::<Result<Vec<_>, GeneratorError>>()?;
        let game_run = GameRun::from_campaign(&Campaign {
            starting_lives,
            levels: levels.iter().map(|config| config.rules).collect(),
        })?;
        Ok(Self {
            game_run,
            levels: built.into(),
        })
    }

    /// Configure scoring for every run started from this campaign.
    ///
    /// # Errors
    /// [`MathgameSessionError::Scoring`] for invalid rules or a lives-cap conflict.
    pub fn with_scoring(mut self, scoring: ScoringRules) -> Result<Self, MathgameSessionError> {
        self.game_run.set_scoring(scoring)?;
        Ok(self)
    }

    /// Configure continues for every run started from this campaign.
    #[must_use]
    pub fn with_continues(mut self, continues: ContinueRules) -> Self {
        self.game_run.set_continues(continues);
        self
    }

    /// Start at level one with independent run/RNG state and shared generators.
    #[must_use]
    pub fn start(&self, seed: u64) -> MathgameSession {
        let mut rng = Rng::new(seed);
        let current = make_problem(
            &self.levels[0].generator,
            &mut rng,
            self.game_run.current_level_spec().answer_mode,
        );
        MathgameSession {
            game_run: self.game_run.clone(),
            rng,
            levels: Rc::clone(&self.levels),
            current,
            last_result: None,
        }
    }
}

#[cfg(test)]
mod tests;
