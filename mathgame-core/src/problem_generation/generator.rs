//! Generator.
use super::Problem;
use crate::rng::Rng;

/// Produces [`Problem`]s, drawing randomness from a seeded [`Rng`].
pub trait Generator {
    fn generate(&self, rng: &mut Rng) -> Problem;
}

/// Why a generator could not be constructed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneratorError {
    /// The operand range is empty (`start > end`).
    EmptyRange,
    /// The operand range — or a mix's summed weights — admits values whose
    /// arithmetic overflows `i64`.
    RangeOverflows,
    /// A maximum operand distance was set for division, where it has no
    /// meaning: the shown numbers are dividend and divisor, and the dividend is
    /// the divisor times the quotient — not an independent draw that can be
    /// held near its partner.
    DistanceUnsupported,
    /// A mix was built with no generators to draw from.
    EmptyMix,
    /// A mix entry's weight was zero — it could never be drawn; remove the
    /// entry instead of weighting it out.
    ZeroMixWeight,
    /// The generator does not pose this operator (fraction arithmetic covers
    /// addition and multiplication; differences could go negative and
    /// quotients are a different drill).
    OperatorUnsupported,
}

/// The `expect` message for fraction arithmetic that construction-time
/// validation has already proven safe: parts cannot overflow and denominators
/// are nonzero.
pub(super) const FRACTION_INVARIANT: &str =
    "fraction ranges validated at construction; parts cannot overflow and denominators are nonzero";
