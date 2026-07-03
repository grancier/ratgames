//! Mastery: accumulate assessment evidence into a per-skill readiness signal.
//!
//! Thin first cut. Each attempt contributes [`SkillEvidence`](crate::answer_evaluation::SkillEvidence)
//! (from an [`Evaluation`]); a skill is **mastered** when at least
//! `required_correct` of its last `window` attempts were correct. The signal is a
//! coarse [`SkillState`] — `Unseen` / `Practicing` / `Mastered` — and the set of
//! mastered skills feeds [`Curriculum::eligible`](crate::curriculum::Curriculum::eligible)
//! directly, closing the evaluate → master → unlock loop.
//!
//! Thresholds are integer "K of last N" — **no floating point**, matching the
//! exact-arithmetic invariant. The full evidence state machine (Introduced /
//! Practicing / Proficient / RetentionDue / RemediationNeeded, critical-error and
//! mixed-context gates, retention checks) arrives with the richer-content step.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::answer_evaluation::Evaluation;
use crate::curriculum::SkillId;

/// When a skill counts as mastered: at least `required_correct` of the last
/// `window` attempts correct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MasteryPolicy {
    window: usize,
    required_correct: usize,
}

impl MasteryPolicy {
    /// A policy requiring `required_correct` correct out of the last `window`
    /// attempts. `window` is at least 1; `required_correct` is clamped to
    /// `1..=window`.
    #[must_use]
    pub fn new(window: usize, required_correct: usize) -> Self {
        let window = window.max(1);
        Self {
            window,
            required_correct: required_correct.clamp(1, window),
        }
    }

    #[must_use]
    pub fn window(self) -> usize {
        self.window
    }

    #[must_use]
    pub fn required_correct(self) -> usize {
        self.required_correct
    }
}

impl Default for MasteryPolicy {
    /// Four of the last five attempts correct.
    fn default() -> Self {
        Self::new(5, 4)
    }
}

/// A coarse readiness signal for one skill (a thin stand-in for the full state
/// machine).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillState {
    /// No attempts recorded yet.
    Unseen,
    /// Attempted, but not yet meeting the mastery bar.
    Practicing,
    /// Meets the mastery bar.
    Mastered,
}

/// Accumulated evidence for a single skill.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillMastery {
    attempts: u32,
    correct: u32,
    recent: VecDeque<bool>,
}

impl SkillMastery {
    /// Total attempts recorded.
    #[must_use]
    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// Total correct attempts recorded.
    #[must_use]
    pub fn correct(&self) -> u32 {
        self.correct
    }
}

/// Per-skill mastery tracker.
#[derive(Debug, Clone)]
pub struct Mastery {
    policy: MasteryPolicy,
    skills: HashMap<SkillId, SkillMastery>,
}

impl Mastery {
    /// A tracker using `policy`.
    #[must_use]
    pub fn new(policy: MasteryPolicy) -> Self {
        Self {
            policy,
            skills: HashMap::new(),
        }
    }

    /// Record a single attempt at `skill`.
    pub fn record(&mut self, skill: &SkillId, correct: bool) {
        let entry = self.skills.entry(skill.clone()).or_default();
        entry.attempts += 1;
        if correct {
            entry.correct += 1;
        }
        entry.recent.push_back(correct);
        if entry.recent.len() > self.policy.window {
            entry.recent.pop_front();
        }
    }

    /// Record every piece of evidence from an [`Evaluation`].
    pub fn record_evaluation(&mut self, evaluation: &Evaluation) {
        for evidence in evaluation.skill_evidence() {
            self.record(&evidence.skill, evidence.correct);
        }
    }

    /// The accumulated evidence for `skill`, if any attempts were recorded.
    #[must_use]
    pub fn skill(&self, skill: &SkillId) -> Option<&SkillMastery> {
        self.skills.get(skill)
    }

    /// The readiness state of `skill`.
    #[must_use]
    pub fn state(&self, skill: &SkillId) -> SkillState {
        match self.skills.get(skill) {
            None => SkillState::Unseen,
            Some(entry) if self.meets_bar(entry) => SkillState::Mastered,
            Some(_) => SkillState::Practicing,
        }
    }

    /// Whether `skill` is mastered.
    #[must_use]
    pub fn is_mastered(&self, skill: &SkillId) -> bool {
        matches!(self.state(skill), SkillState::Mastered)
    }

    /// The set of mastered skills — ready to hand to
    /// [`Curriculum::eligible`](crate::curriculum::Curriculum::eligible).
    #[must_use]
    pub fn mastered_skills(&self) -> HashSet<SkillId> {
        self.skills
            .iter()
            .filter(|(_, entry)| self.meets_bar(entry))
            .map(|(id, _)| id.clone())
            .collect()
    }

    fn meets_bar(&self, entry: &SkillMastery) -> bool {
        entry.recent.len() >= self.policy.window
            && entry.recent.iter().filter(|&&c| c).count() >= self.policy.required_correct
    }
}

impl Default for Mastery {
    fn default() -> Self {
        Self::new(MasteryPolicy::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curriculum::{Band, Curriculum, Skill};
    use crate::math_core::Operator;
    use crate::problem_generation::{DirectArithmetic, Generator};
    use crate::rng::Rng;

    fn skill(id: &str) -> SkillId {
        SkillId::from(id)
    }

    #[test]
    fn policy_clamps_required_correct_to_the_window() {
        let policy = MasteryPolicy::new(3, 9);
        assert_eq!(policy.window(), 3);
        assert_eq!(policy.required_correct(), 3);
        // A zero window is lifted to one.
        assert_eq!(MasteryPolicy::new(0, 0).window(), 1);
    }

    #[test]
    fn unseen_then_practicing_then_mastered() {
        let mut mastery = Mastery::new(MasteryPolicy::new(3, 3));
        let s = skill("add");
        assert_eq!(mastery.state(&s), SkillState::Unseen);

        mastery.record(&s, true);
        mastery.record(&s, true);
        assert_eq!(mastery.state(&s), SkillState::Practicing); // window not full

        mastery.record(&s, true);
        assert_eq!(mastery.state(&s), SkillState::Mastered);
        assert!(mastery.is_mastered(&s));
        assert_eq!(mastery.skill(&s).unwrap().attempts(), 3);
        assert_eq!(mastery.skill(&s).unwrap().correct(), 3);
    }

    #[test]
    fn a_recent_miss_within_the_window_blocks_mastery() {
        let mut mastery = Mastery::new(MasteryPolicy::new(3, 3));
        let s = skill("sub");
        mastery.record(&s, true);
        mastery.record(&s, true);
        mastery.record(&s, false); // last three: T, T, F
        assert_eq!(mastery.state(&s), SkillState::Practicing);
    }

    #[test]
    fn old_misses_roll_out_of_the_window() {
        let mut mastery = Mastery::new(MasteryPolicy::new(3, 3));
        let s = skill("mul");
        mastery.record(&s, false); // early miss
        mastery.record(&s, true);
        mastery.record(&s, true);
        assert!(!mastery.is_mastered(&s)); // window still holds the miss
        mastery.record(&s, true); // miss rolls out; last three all correct
        assert!(mastery.is_mastered(&s));
    }

    #[test]
    fn recording_evaluations_accumulates_evidence() {
        let generator = DirectArithmetic::new("x", "b", Operator::Add, 0..=10).unwrap();
        let mut rng = Rng::new(1);
        let problem = generator.generate(&mut rng);
        let answer = problem.canonical_solution().to_fraction_string();
        let evaluation = crate::answer_evaluation::evaluate(&problem, &answer);

        let mut mastery = Mastery::new(MasteryPolicy::new(2, 2));
        mastery.record_evaluation(&evaluation);
        mastery.record_evaluation(&evaluation);
        assert!(mastery.is_mastered(&skill("x")));
    }

    #[test]
    fn mastered_skills_unlock_the_next_curriculum_skill() {
        let curriculum = Curriculum::new(
            vec![Band::new("b", "B", 0)],
            vec![
                Skill::new("a", "A", "b"),
                Skill::new("b2", "B2", "b").with_prerequisite("a"),
            ],
        )
        .unwrap();
        let mut mastery = Mastery::new(MasteryPolicy::new(2, 2));

        // Before any mastery, only the prerequisite-free root is eligible.
        assert_eq!(
            curriculum.eligible(&mastery.mastered_skills()),
            vec![&skill("a")]
        );

        mastery.record(&skill("a"), true);
        mastery.record(&skill("a"), true);
        assert!(mastery.is_mastered(&skill("a")));

        // Its dependent unlocks; the mastered skill drops out of the eligible set.
        assert_eq!(
            curriculum.eligible(&mastery.mastered_skills()),
            vec![&skill("b2")]
        );
    }
}
