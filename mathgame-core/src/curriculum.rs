//! Curriculum: the skill graph — what to learn, and in what order.
//!
//! Skills are grouped into ordered [`Band`]s (number sense → single-digit
//! add/sub → … → percentages) and connected by prerequisite edges into a DAG.
//! The curriculum is pure *structure*: it names skills, their prerequisites, and
//! their objectives, and answers ordering/eligibility queries. It does not
//! generate problems, evaluate answers, or track mastery — those are separate
//! modules that consult this graph.
//!
//! The concrete standard progression is populated alongside the generators that
//! teach each skill; this module is the model and the queries over it.

use std::collections::{HashMap, HashSet};
use std::fmt;

/// A stable identifier for a [`Skill`] (e.g. `"sums-to-10"`). String-backed so
/// lesson packs can name skills declaratively later.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SkillId(String);

impl SkillId {
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for SkillId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for SkillId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// A stable identifier for a [`Band`], a coarse stage of the progression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BandId(String);

impl BandId {
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for BandId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for BandId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// A coarse stage of the progression that groups related skills, with an
/// explicit `order` for presentation (lesson select walks bands in order).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Band {
    id: BandId,
    name: String,
    order: u32,
}

impl Band {
    #[must_use]
    pub fn new(id: impl Into<BandId>, name: impl Into<String>, order: u32) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            order,
        }
    }

    #[must_use]
    pub fn id(&self) -> &BandId {
        &self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn order(&self) -> u32 {
        self.order
    }
}

/// One learnable skill: its identity, display name, band, prerequisites, and
/// learning objectives. Built fluently — prerequisites and objectives default
/// empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    id: SkillId,
    name: String,
    band: BandId,
    prerequisites: Vec<SkillId>,
    objectives: Vec<String>,
}

impl Skill {
    #[must_use]
    pub fn new(id: impl Into<SkillId>, name: impl Into<String>, band: impl Into<BandId>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            band: band.into(),
            prerequisites: Vec::new(),
            objectives: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_prerequisite(mut self, id: impl Into<SkillId>) -> Self {
        self.prerequisites.push(id.into());
        self
    }

    #[must_use]
    pub fn with_prerequisites<I>(mut self, ids: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<SkillId>,
    {
        self.prerequisites.extend(ids.into_iter().map(Into::into));
        self
    }

    #[must_use]
    pub fn with_objective(mut self, objective: impl Into<String>) -> Self {
        self.objectives.push(objective.into());
        self
    }

    #[must_use]
    pub fn with_objectives<I>(mut self, objectives: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        self.objectives
            .extend(objectives.into_iter().map(Into::into));
        self
    }

    #[must_use]
    pub fn id(&self) -> &SkillId {
        &self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn band(&self) -> &BandId {
        &self.band
    }

    #[must_use]
    pub fn prerequisites(&self) -> &[SkillId] {
        &self.prerequisites
    }

    #[must_use]
    pub fn objectives(&self) -> &[String] {
        &self.objectives
    }
}

/// Failure building a [`Curriculum`] from bands and skills.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CurriculumError {
    /// Two skills share an id.
    DuplicateSkill(SkillId),
    /// A skill names a band that is not in the curriculum.
    UnknownBand(BandId),
    /// A skill names a prerequisite that is not a skill in the curriculum.
    UnknownPrerequisite {
        skill: SkillId,
        prerequisite: SkillId,
    },
    /// The prerequisite edges contain a cycle.
    Cycle,
}

impl fmt::Display for CurriculumError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CurriculumError::DuplicateSkill(id) => write!(f, "duplicate skill: {}", id.as_str()),
            CurriculumError::UnknownBand(id) => {
                write!(f, "skill references unknown band: {}", id.as_str())
            }
            CurriculumError::UnknownPrerequisite {
                skill,
                prerequisite,
            } => write!(
                f,
                "skill {} requires unknown prerequisite {}",
                skill.as_str(),
                prerequisite.as_str()
            ),
            CurriculumError::Cycle => f.write_str("prerequisite cycle"),
        }
    }
}

impl std::error::Error for CurriculumError {}

/// The skill graph: ordered bands, skills keyed by id, and validated
/// prerequisite edges (a DAG). Immutable once built.
#[derive(Debug, Clone)]
pub struct Curriculum {
    bands: Vec<Band>,
    skills: Vec<Skill>, // insertion order — the basis for deterministic queries
    index: HashMap<SkillId, usize>, // id -> position in `skills`
    dependents: HashMap<SkillId, Vec<SkillId>>, // reverse edges
}

impl Curriculum {
    /// Build and validate a curriculum. Errors on a duplicate skill id, a skill
    /// referencing an unknown band or prerequisite, or a prerequisite cycle.
    pub fn new(bands: Vec<Band>, skills: Vec<Skill>) -> Result<Self, CurriculumError> {
        let mut index: HashMap<SkillId, usize> = HashMap::with_capacity(skills.len());
        for (i, skill) in skills.iter().enumerate() {
            if index.insert(skill.id.clone(), i).is_some() {
                return Err(CurriculumError::DuplicateSkill(skill.id.clone()));
            }
        }

        let band_ids: HashSet<BandId> = bands.iter().map(|b| b.id.clone()).collect();
        for skill in &skills {
            if !band_ids.contains(&skill.band) {
                return Err(CurriculumError::UnknownBand(skill.band.clone()));
            }
            for prerequisite in &skill.prerequisites {
                if !index.contains_key(prerequisite) {
                    return Err(CurriculumError::UnknownPrerequisite {
                        skill: skill.id.clone(),
                        prerequisite: prerequisite.clone(),
                    });
                }
            }
        }

        let mut dependents: HashMap<SkillId, Vec<SkillId>> = HashMap::new();
        for skill in &skills {
            for prerequisite in &skill.prerequisites {
                dependents
                    .entry(prerequisite.clone())
                    .or_default()
                    .push(skill.id.clone());
            }
        }

        let curriculum = Self {
            bands,
            skills,
            index,
            dependents,
        };
        // A full topological pass proves the graph is acyclic.
        if curriculum.kahn().len() != curriculum.skills.len() {
            return Err(CurriculumError::Cycle);
        }
        Ok(curriculum)
    }

    /// The bands, in the order given at construction.
    #[must_use]
    pub fn bands(&self) -> &[Band] {
        &self.bands
    }

    /// Every skill, in insertion order.
    #[must_use]
    pub fn skills(&self) -> &[Skill] {
        &self.skills
    }

    /// Look up a skill by id.
    #[must_use]
    pub fn skill(&self, id: &SkillId) -> Option<&Skill> {
        self.index.get(id).map(|&i| &self.skills[i])
    }

    /// The skills belonging to `band`, in insertion order.
    #[must_use]
    pub fn skills_in_band(&self, band: &BandId) -> Vec<&Skill> {
        self.skills.iter().filter(|s| &s.band == band).collect()
    }

    /// A skill's direct prerequisites (empty for an unknown skill).
    #[must_use]
    pub fn prerequisites(&self, id: &SkillId) -> &[SkillId] {
        self.skill(id)
            .map(|s| s.prerequisites.as_slice())
            .unwrap_or_default()
    }

    /// The skills that name `id` as a direct prerequisite, in insertion order.
    #[must_use]
    pub fn dependents(&self, id: &SkillId) -> &[SkillId] {
        self.dependents
            .get(id)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// Whether `id` is ready to practise: it exists, is not already mastered, and
    /// all of its direct prerequisites are mastered.
    #[must_use]
    pub fn is_eligible(&self, id: &SkillId, mastered: &HashSet<SkillId>) -> bool {
        match self.skill(id) {
            None => false,
            Some(skill) => {
                !mastered.contains(id) && skill.prerequisites.iter().all(|p| mastered.contains(p))
            }
        }
    }

    /// Every skill that is eligible given the `mastered` set, in insertion order.
    #[must_use]
    pub fn eligible(&self, mastered: &HashSet<SkillId>) -> Vec<&SkillId> {
        self.skills
            .iter()
            .map(|s| &s.id)
            .filter(|id| self.is_eligible(id, mastered))
            .collect()
    }

    /// The skills in a prerequisites-before-dependents order (deterministic:
    /// ties break by insertion order).
    #[must_use]
    pub fn topological_order(&self) -> Vec<&SkillId> {
        self.kahn()
            .into_iter()
            .map(|i| &self.skills[i].id)
            .collect()
    }

    /// Kahn's algorithm over skill positions. Returns positions in topological
    /// order; a length below `skills.len()` means a cycle. Deterministic.
    fn kahn(&self) -> Vec<usize> {
        let n = self.skills.len();
        let mut indegree = vec![0usize; n];
        let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (position, skill) in self.skills.iter().enumerate() {
            indegree[position] = skill.prerequisites.len();
            for prerequisite in &skill.prerequisites {
                adjacency[self.index[prerequisite]].push(position);
            }
        }

        let mut queue: Vec<usize> = (0..n).filter(|&i| indegree[i] == 0).collect();
        let mut order = Vec::with_capacity(n);
        let mut head = 0;
        while head < queue.len() {
            let node = queue[head];
            head += 1;
            order.push(node);
            for &dependent in &adjacency[node] {
                indegree[dependent] -= 1;
                if indegree[dependent] == 0 {
                    queue.push(dependent);
                }
            }
        }
        order
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mastered(ids: &[&str]) -> HashSet<SkillId> {
        ids.iter().map(|s| SkillId::from(*s)).collect()
    }

    /// A small two-band arithmetic curriculum used across the tests.
    fn sample() -> Curriculum {
        let bands = vec![
            Band::new("number-sense", "Number sense", 0),
            Band::new("addition", "Single-digit addition", 1),
        ];
        let skills = vec![
            Skill::new("digits", "Digit recognition", "number-sense")
                .with_objective("recognize digits 0-9"),
            Skill::new("equality", "Equality", "number-sense").with_prerequisite("digits"),
            Skill::new("sums-to-10", "Sums to 10", "addition").with_prerequisite("equality"),
            Skill::new("sums-to-20", "Sums to 20", "addition").with_prerequisite("sums-to-10"),
            Skill::new("missing-addend", "Missing addend", "addition")
                .with_prerequisite("sums-to-10"),
        ];
        Curriculum::new(bands, skills).unwrap()
    }

    #[test]
    fn skill_builder_collects_prerequisites_and_objectives() {
        let skill = Skill::new("s", "S", "b")
            .with_prerequisites(["a", "b"])
            .with_objectives(["one", "two"]);
        assert_eq!(skill.id(), &SkillId::from("s"));
        assert_eq!(skill.band(), &BandId::from("b"));
        assert_eq!(
            skill.prerequisites(),
            &[SkillId::from("a"), SkillId::from("b")]
        );
        assert_eq!(skill.objectives(), &["one".to_string(), "two".to_string()]);
    }

    #[test]
    fn lookups_by_id_and_band() {
        let c = sample();
        assert_eq!(c.bands().len(), 2);
        assert_eq!(c.skill(&"sums-to-10".into()).unwrap().name(), "Sums to 10");
        assert!(c.skill(&"nope".into()).is_none());

        let addition: Vec<&str> = c
            .skills_in_band(&"addition".into())
            .iter()
            .map(|s| s.id().as_str())
            .collect();
        assert_eq!(addition, ["sums-to-10", "sums-to-20", "missing-addend"]);
    }

    #[test]
    fn prerequisites_and_dependents_are_direct_edges() {
        let c = sample();
        assert_eq!(
            c.prerequisites(&"sums-to-10".into()),
            &[SkillId::from("equality")]
        );
        // Two skills depend on sums-to-10, reported in insertion order.
        assert_eq!(
            c.dependents(&"sums-to-10".into()),
            &[SkillId::from("sums-to-20"), SkillId::from("missing-addend")]
        );
        assert!(c.dependents(&"missing-addend".into()).is_empty());
        assert!(c.prerequisites(&"digits".into()).is_empty());
    }

    #[test]
    fn eligibility_follows_the_mastered_set() {
        let c = sample();
        // Nothing mastered: only the prerequisite-free root is eligible.
        assert_eq!(c.eligible(&mastered(&[])), vec![&SkillId::from("digits")]);
        // digits mastered: equality unlocks; digits itself is no longer eligible.
        assert_eq!(
            c.eligible(&mastered(&["digits"])),
            vec![&SkillId::from("equality")]
        );
        // Through sums-to-10: both dependents unlock together.
        assert_eq!(
            c.eligible(&mastered(&["digits", "equality", "sums-to-10"])),
            vec![
                &SkillId::from("sums-to-20"),
                &SkillId::from("missing-addend")
            ]
        );
        assert!(!c.is_eligible(&"sums-to-20".into(), &mastered(&["digits"])));
    }

    #[test]
    fn topological_order_puts_prerequisites_first() {
        let c = sample();
        let order: Vec<&str> = c.topological_order().iter().map(|id| id.as_str()).collect();
        assert_eq!(
            order,
            [
                "digits",
                "equality",
                "sums-to-10",
                "sums-to-20",
                "missing-addend"
            ]
        );
    }

    #[test]
    fn duplicate_skill_is_rejected() {
        let bands = vec![Band::new("b", "B", 0)];
        let skills = vec![Skill::new("x", "X", "b"), Skill::new("x", "X2", "b")];
        assert_eq!(
            Curriculum::new(bands, skills).unwrap_err(),
            CurriculumError::DuplicateSkill(SkillId::from("x"))
        );
    }

    #[test]
    fn unknown_band_is_rejected() {
        let skills = vec![Skill::new("x", "X", "ghost")];
        assert_eq!(
            Curriculum::new(vec![], skills).unwrap_err(),
            CurriculumError::UnknownBand(BandId::from("ghost"))
        );
    }

    #[test]
    fn unknown_prerequisite_is_rejected() {
        let bands = vec![Band::new("b", "B", 0)];
        let skills = vec![Skill::new("x", "X", "b").with_prerequisite("ghost")];
        assert_eq!(
            Curriculum::new(bands, skills).unwrap_err(),
            CurriculumError::UnknownPrerequisite {
                skill: SkillId::from("x"),
                prerequisite: SkillId::from("ghost"),
            }
        );
    }

    #[test]
    fn a_cycle_is_rejected() {
        let bands = vec![Band::new("b", "B", 0)];
        let skills = vec![
            Skill::new("a", "A", "b").with_prerequisite("c"),
            Skill::new("c", "C", "b").with_prerequisite("a"),
        ];
        assert_eq!(
            Curriculum::new(bands, skills).unwrap_err(),
            CurriculumError::Cycle
        );
    }
}
