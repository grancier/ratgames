//! JSON-facing math content and its translation into core generators.
use mathgame_core::{
    DirectArithmetic, FractionArithmetic, Generator, GeneratorError, Mix, Operator,
    SimplifyFraction,
};
use ratgames::LevelConfig;

/// A kind of question as named in a level file: the four whole-number
/// operators, plus the fraction kinds (`simplify`, `fraction_add`,
/// `fraction_multiply`). A config-facing enum because `mathgame_core` is
/// dependency-free and so carries no serde of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorConfig {
    Add,
    Subtract,
    Multiply,
    Divide,
    /// Reduce a fraction to lowest terms: `125/500 = ?`.
    Simplify,
    /// Add two proper fractions: `212/325 + 128/225 = ?`.
    FractionAdd,
    /// Multiply two proper fractions.
    FractionMultiply,
}

impl OperatorConfig {
    /// The core operator this names, for the four whole-number kinds; the
    /// fraction kinds build their own generators and have no single core
    /// operator.
    #[must_use]
    pub fn operator(self) -> Option<Operator> {
        match self {
            Self::Add => Some(Operator::Add),
            Self::Subtract => Some(Operator::Subtract),
            Self::Multiply => Some(Operator::Multiply),
            Self::Divide => Some(Operator::Divide),
            Self::Simplify | Self::FractionAdd | Self::FractionMultiply => None,
        }
    }
}

/// The coarse skill band the core records on a problem for an operator. The app
/// displays the equation itself, not the band, so this is just sensible metadata.
fn operator_band(operator: Operator) -> &'static str {
    match operator {
        Operator::Add => "addition",
        Operator::Subtract => "subtraction",
        Operator::Multiply => "multiplication",
        Operator::Divide => "division",
    }
}

/// One kind of question a level asks: an operator (or fraction kind) over an
/// inclusive operand range, an optional cap on how far apart the operands may
/// drift, and this entry's relative share of the level's questions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub struct ProblemSpec {
    /// The kind of question this entry drills.
    pub operator: OperatorConfig,
    /// Inclusive lower bound of the primary range: the operands for the
    /// whole-number kinds, the reduced base fraction's parts for `simplify`,
    /// the numerators for the fraction-arithmetic kinds.
    pub min: i64,
    /// Inclusive upper bound of the primary range.
    pub max: i64,
    /// Operands lie at most this far apart (`|lhs − rhs|`); omitted, the range
    /// alone constrains them. Meaningless for division and the fraction kinds,
    /// which reject it.
    #[serde(default)]
    pub max_distance: Option<u64>,
    /// Relative share of the level's questions this entry poses — shares, not
    /// percentages (`40/40/20` and `2/2/1` are the same mix). Omitted, `1`, so
    /// weightless entries mix evenly.
    #[serde(default = "default_weight")]
    pub weight: u32,
    /// `simplify` only: lowest scale factor applied to the base fraction.
    /// Omitted, `2` (the smallest factor that guarantees a reducible prompt).
    #[serde(default)]
    pub multiplier_min: Option<i64>,
    /// `simplify` only: highest scale factor (`125` turns a base `1/4` into
    /// `125/500`). Omitted, `12`.
    #[serde(default)]
    pub multiplier_max: Option<i64>,
    /// Fraction-arithmetic kinds only: lowest denominator. Omitted, `2`.
    #[serde(default)]
    pub denominator_min: Option<i64>,
    /// Fraction-arithmetic kinds only: highest denominator. Omitted, `9`.
    #[serde(default)]
    pub denominator_max: Option<i64>,
}

/// The weight of a [`ProblemSpec`] that names none: an even share.
fn default_weight() -> u32 {
    1
}

impl ProblemSpec {
    /// Build this entry's generator, named `name` (the level's display name).
    fn generator(&self, name: &str) -> Result<Box<dyn Generator>, GeneratorError> {
        match self.operator {
            OperatorConfig::Add => self.whole_number(name, Operator::Add),
            OperatorConfig::Subtract => self.whole_number(name, Operator::Subtract),
            OperatorConfig::Multiply => self.whole_number(name, Operator::Multiply),
            OperatorConfig::Divide => self.whole_number(name, Operator::Divide),
            OperatorConfig::Simplify => {
                self.no_distance()?;
                let multiplier =
                    self.multiplier_min.unwrap_or(2)..=self.multiplier_max.unwrap_or(12);
                Ok(Box::new(SimplifyFraction::new(
                    name,
                    "fractions",
                    self.min..=self.max,
                    multiplier,
                )?))
            }
            OperatorConfig::FractionAdd => self.fraction(name, Operator::Add),
            OperatorConfig::FractionMultiply => self.fraction(name, Operator::Multiply),
        }
    }

    /// A whole-number arithmetic generator over the primary range, with the
    /// optional operand-distance cap.
    fn whole_number(
        &self,
        name: &str,
        operator: Operator,
    ) -> Result<Box<dyn Generator>, GeneratorError> {
        let generator =
            DirectArithmetic::new(name, operator_band(operator), operator, self.min..=self.max)?;
        Ok(match self.max_distance {
            Some(distance) => Box::new(generator.with_max_distance(distance)?),
            None => Box::new(generator),
        })
    }

    /// A fraction-arithmetic generator: the primary range supplies numerators,
    /// the denominator fields (or their defaults) the denominators.
    fn fraction(
        &self,
        name: &str,
        operator: Operator,
    ) -> Result<Box<dyn Generator>, GeneratorError> {
        self.no_distance()?;
        let denominators = self.denominator_min.unwrap_or(2)..=self.denominator_max.unwrap_or(9);
        Ok(Box::new(FractionArithmetic::new(
            name,
            "fractions",
            operator,
            self.min..=self.max,
            denominators,
        )?))
    }

    /// The operand-distance cap belongs to whole-number arithmetic; a fraction
    /// entry carrying one is a config mistake.
    fn no_distance(&self) -> Result<(), GeneratorError> {
        if self.max_distance.is_some() {
            return Err(GeneratorError::DistanceUnsupported);
        }
        Ok(())
    }
}

/// The arithmetic a level of the gauntlet drills: a weighted mix of
/// [`ProblemSpec`] entries, so one level can pose several operators in
/// proportion (double-digit add/sub with a 20% share of multiplication). This
/// is `mathgame-app`'s level *content* — the math half of a `level_<n>.json`
/// file, flattened alongside the reusable rules by [`LevelConfig`]. The toolkit
/// stays math-free, so the operators and ranges live here, not in `ratgames`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Arithmetic {
    /// The kinds of questions this level asks, mixed by weight.
    pub problems: Vec<ProblemSpec>,
}

impl Arithmetic {
    /// Build the weighted problem mix this level drills, named `name` (the
    /// level's display name, carried on the [`LevelConfig`] shell).
    ///
    /// # Errors
    /// [`GeneratorError`] if `problems` is empty, an entry's weight is zero, an
    /// operand range is empty or would overflow, or a `max_distance` is set on
    /// division.
    pub fn generator(&self, name: &str) -> Result<Mix, GeneratorError> {
        let entries = self
            .problems
            .iter()
            .map(|spec| Ok((spec.weight, spec.generator(name)?)))
            .collect::<Result<Vec<_>, GeneratorError>>()?;
        Mix::new(entries)
    }
}

/// One level of the gauntlet, as authored in a `level_<n>.json` file: the
/// reusable [`LevelConfig`] shell (display name, difficulty label, and the
/// [`ratgames::LevelSpec`] win-condition/reward/input rules) carrying this app's
/// [`Arithmetic`] content. Both halves are flattened, so the file stays one flat
/// object — e.g. `{"name":"NUMBER YARD","difficulty":"EASY",
/// "problems":[{"operator":"add","min":1,"max":2}],"required_successes":5,...}`.
/// Omitted rule fields fall back to [`ratgames::LevelSpec`] defaults; the name,
/// difficulty, and `problems` entries are required (each entry's `weight`
/// defaults to an even share and `max_distance` to unconstrained).
pub type MathLevel = LevelConfig<Arithmetic>;

#[cfg(test)]
mod tests;
