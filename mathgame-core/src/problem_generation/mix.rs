//! Mix.
use super::{Generator, GeneratorError, Problem};
use crate::rng::Rng;
use std::fmt;

/// A weighted mix of generators: every [`generate`](Generator::generate) call
/// picks one entry by weight from the seeded [`Rng`], then delegates to it —
/// how a single level drills several operators at once (e.g. addition and
/// subtraction at 40 each with multiplication at 20, an 80/20 split).
///
/// Weights are relative shares, not percentages. One draw decides the entry and
/// the chosen generator then draws as usual, so a mixed drill replays
/// identically for a seed. Composes with anything implementing [`Generator`],
/// including another `Mix`.
pub struct Mix {
    entries: Vec<(u32, Box<dyn Generator>)>,
    /// Cached sum of the weights; validated non-zero and within `i64`.
    total_weight: i64,
}

impl fmt::Debug for Mix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let weights: Vec<u32> = self.entries.iter().map(|(weight, _)| *weight).collect();
        f.debug_struct("Mix")
            .field("weights", &weights)
            .finish_non_exhaustive()
    }
}

impl Mix {
    /// A weighted mix over `entries` (`(weight, generator)` pairs).
    ///
    /// Errors with [`GeneratorError::EmptyMix`] when `entries` is empty,
    /// [`GeneratorError::ZeroMixWeight`] when any entry's weight is zero (it
    /// could never be drawn — remove the entry instead), and
    /// [`GeneratorError::RangeOverflows`] when the summed weights exceed `i64`
    /// (unreachable with realistic mixes).
    pub fn new(entries: Vec<(u32, Box<dyn Generator>)>) -> Result<Self, GeneratorError> {
        if entries.is_empty() {
            return Err(GeneratorError::EmptyMix);
        }
        if entries.iter().any(|(weight, _)| *weight == 0) {
            return Err(GeneratorError::ZeroMixWeight);
        }
        let total_weight = entries
            .iter()
            .try_fold(0_i64, |sum, (weight, _)| {
                sum.checked_add(i64::from(*weight))
            })
            .ok_or(GeneratorError::RangeOverflows)?;
        Ok(Self {
            entries,
            total_weight,
        })
    }
}

/// The `expect` message for the mix draw, whose construction-time validation
/// ([`Mix::new`]) has already proven the entries non-empty.
const MIX_INVARIANT: &str = "mix validated non-empty at construction";

impl Generator for Mix {
    fn generate(&self, rng: &mut Rng) -> Problem {
        // A ticket below the total weight lands in exactly one entry's band;
        // walking the bands keeps the pick a single rng draw.
        let mut ticket = rng.int_range(0..=self.total_weight - 1);
        let (last, rest) = self.entries.split_last().expect(MIX_INVARIANT);
        for (weight, generator) in rest {
            let weight = i64::from(*weight);
            if ticket < weight {
                return generator.generate(rng);
            }
            ticket -= weight;
        }
        last.1.generate(rng)
    }
}

#[cfg(test)]
mod tests;
