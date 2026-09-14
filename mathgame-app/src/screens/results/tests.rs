use super::super::test_support::*;
use super::*;

#[test]
fn the_ending_title_prefers_the_earned_rank() {
    let result = copy().result;
    assert_eq!(
        ending_title(RunPhase::Won, Some("NO MISS CHAMP"), &result),
        "NO MISS CHAMP"
    );
    assert_eq!(ending_title(RunPhase::Won, None, &result), "YOU WIN");
    assert_eq!(ending_title(RunPhase::GameOver, None, &result), "GAME OVER");
    // A rank on a lost run (a game may configure one) still shows.
    assert_eq!(
        ending_title(RunPhase::GameOver, Some("GOOD EFFORT"), &result),
        "GOOD EFFORT"
    );
}
