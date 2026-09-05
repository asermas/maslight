use maslight_core::AutoRule;
use maslight_rules::{evaluate, in_range, RuleContext, RuleWatcher};

fn context() -> RuleContext {
    RuleContext {
        processes: vec![
            String::from("explorer.exe"),
            String::from("witcher3.exe"),
            String::from("code.exe"),
        ],
        fullscreen: false,
        minutes: 12 * 60,
        on_battery: false,
    }
}

#[test]
fn a_time_range_inside_one_day_is_straightforward() {
    // 09:00 to 17:00
    assert!(in_range(10 * 60, 9 * 60, 17 * 60));
    assert!(!in_range(8 * 60, 9 * 60, 17 * 60));
    assert!(in_range(9 * 60, 9 * 60, 17 * 60), "the start is inclusive");
    assert!(!in_range(17 * 60, 9 * 60, 17 * 60), "the end is not");
}

#[test]
fn a_time_range_can_cross_midnight() {
    // 23:00 to 07:00 has to mean the night, not eight minutes.
    let from = 23 * 60;
    let to = 7 * 60;
    assert!(in_range(23 * 60 + 30, from, to), "half eleven is the night");
    assert!(in_range(2 * 60, from, to), "so is two in the morning");
    assert!(!in_range(12 * 60, from, to), "noon is not");
    assert!(!in_range(22 * 60, from, to), "nor is ten at night");
}

#[test]
fn an_empty_range_never_matches() {
    assert!(!in_range(0, 60, 60));
    assert!(!in_range(60, 60, 60));
}

#[test]
fn process_matching_is_a_substring_of_the_executable() {
    let c = context();
    assert!(
        c.has_process("witcher"),
        "people type the game, not the exe"
    );
    assert!(c.has_process("WITCHER3.EXE"), "case should not matter");
    assert!(!c.has_process("cyberpunk"));
    assert!(
        !c.has_process(""),
        "an empty pattern must not match everything"
    );
    assert!(!c.has_process("   "));
}

#[test]
fn the_first_matching_rule_wins() {
    // The list is the priority: a game at midnight stays on the game profile.
    let rules = vec![
        AutoRule::ProcessRunning {
            process: String::from("witcher"),
            profile: String::from("game"),
        },
        AutoRule::TimeRange {
            from_minutes: 0,
            to_minutes: 24 * 60 - 1,
            profile: String::from("evening"),
        },
    ];
    assert_eq!(evaluate(&rules, &context()), Some(String::from("game")));

    // Reverse them and the other one wins, which is what makes the order the
    // control people actually reach for.
    let reversed: Vec<AutoRule> = rules.into_iter().rev().collect();
    assert_eq!(
        evaluate(&reversed, &context()),
        Some(String::from("evening"))
    );
}

#[test]
fn nothing_matches_when_nothing_applies() {
    let rules = vec![
        AutoRule::Fullscreen {
            profile: String::from("cinema"),
        },
        AutoRule::OnBattery {
            profile: String::from("saver"),
        },
    ];
    assert_eq!(evaluate(&rules, &context()), None);
}

#[test]
fn fullscreen_and_battery_each_match_on_their_own() {
    let rules = vec![AutoRule::Fullscreen {
        profile: String::from("cinema"),
    }];
    let mut c = context();
    c.fullscreen = true;
    assert_eq!(evaluate(&rules, &c), Some(String::from("cinema")));

    let rules = vec![AutoRule::OnBattery {
        profile: String::from("saver"),
    }];
    let mut c = context();
    c.on_battery = true;
    assert_eq!(evaluate(&rules, &c), Some(String::from("saver")));
}

#[test]
fn the_watcher_only_speaks_up_when_the_answer_changes() {
    // This is what lets someone override a profile by hand and keep it: the
    // rules stay quiet until the world actually moves.
    let rules = vec![AutoRule::Fullscreen {
        profile: String::from("cinema"),
    }];
    let mut watcher = RuleWatcher::default();
    let mut c = context();

    assert_eq!(
        watcher.apply(&rules, c.clone()),
        None,
        "nothing matches yet"
    );
    assert_eq!(watcher.apply(&rules, c.clone()), None, "still nothing");

    c.fullscreen = true;
    assert_eq!(
        watcher.apply(&rules, c.clone()),
        Some(String::from("cinema")),
        "the first fullscreen frame switches"
    );
    assert_eq!(
        watcher.apply(&rules, c.clone()),
        None,
        "staying fullscreen must not keep re-applying"
    );

    c.fullscreen = false;
    assert_eq!(
        watcher.apply(&rules, c.clone()),
        None,
        "leaving fullscreen reports no match rather than a profile"
    );

    c.fullscreen = true;
    assert_eq!(
        watcher.apply(&rules, c),
        Some(String::from("cinema")),
        "and going back in switches again"
    );
}

#[test]
fn resetting_makes_the_watcher_speak_again() {
    let rules = vec![AutoRule::OnBattery {
        profile: String::from("saver"),
    }];
    let mut watcher = RuleWatcher::default();
    let mut c = context();
    c.on_battery = true;

    assert_eq!(
        watcher.apply(&rules, c.clone()),
        Some(String::from("saver"))
    );
    assert_eq!(watcher.apply(&rules, c.clone()), None);

    // Editing the rule list has to re-apply, or a new rule would not take
    // effect until the condition happened to change.
    watcher.reset();
    assert_eq!(watcher.apply(&rules, c), Some(String::from("saver")));
}

#[test]
fn an_empty_rule_list_is_never_polled() {
    let mut watcher = RuleWatcher::default();
    assert_eq!(watcher.poll(&[]), None);
}

#[test]
fn sampling_the_real_system_does_not_panic() {
    // The probes are best effort, but they must never fall over: a rule that
    // cannot be answered should be quiet, not fatal.
    let rules = vec![
        AutoRule::ProcessRunning {
            process: String::from("definitely-not-running-xyzzy"),
            profile: String::from("p"),
        },
        AutoRule::Fullscreen {
            profile: String::from("p"),
        },
        AutoRule::OnBattery {
            profile: String::from("p"),
        },
    ];
    let mut watcher = RuleWatcher::new(std::time::Duration::from_millis(0));
    watcher.poll(&rules);
    let context = watcher.context();
    assert!(context.minutes < 24 * 60, "the clock should be plausible");
    assert!(
        !context.has_process("definitely-not-running-xyzzy"),
        "that process is not running"
    );
}
