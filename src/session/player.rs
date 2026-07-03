//! The player's profile — a name today, the seam for high scores / unlocks later.

/// Who is playing. Kept minimal on purpose; a session owns one and threads it
/// into the HUD and any per-player progression.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlayerProfile {
    name: String,
}

impl PlayerProfile {
    /// A profile for `name`.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    /// The player's name (empty for the default profile).
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Rename the player (e.g. after the name-entry screen).
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_carries_and_updates_a_name() {
        let mut p = PlayerProfile::new("Ada");
        assert_eq!(p.name(), "Ada");
        p.set_name("Grace");
        assert_eq!(p.name(), "Grace");
    }

    #[test]
    fn default_profile_is_nameless() {
        assert_eq!(PlayerProfile::default().name(), "");
    }
}
