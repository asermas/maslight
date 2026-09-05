//! # maslight-rules
//!
//! Automatic profile switching.
//!
//! A rule is a condition and a profile. Rules are checked in order and the
//! first match wins, which makes the list itself the priority: put the game
//! rule above the evening rule and a game at midnight still gets the game
//! profile.
//!
//! Evaluation is pure and takes a [`RuleContext`], so the logic is testable
//! without a desktop. Gathering that context is the only platform code, and it
//! is deliberately cheap: the sampler runs once every couple of seconds, never
//! in the frame loop.

use std::time::{Duration, Instant};

use maslight_core::AutoRule;

mod platform;

pub use platform::Sampler;

/// A snapshot of the things rules can ask about.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuleContext {
    /// Lowercase executable names of running processes.
    pub processes: Vec<String>,
    /// Some window is covering a whole monitor.
    pub fullscreen: bool,
    /// Local time as minutes past midnight.
    pub minutes: u16,
    /// The machine is running from its battery.
    pub on_battery: bool,
}

impl RuleContext {
    /// True when any running process name contains `needle`.
    ///
    /// Substring rather than equality because people write `witcher` and mean
    /// `witcher3.exe`, and because Linux truncates `comm` to fifteen
    /// characters.
    pub fn has_process(&self, needle: &str) -> bool {
        let needle = needle.trim().to_lowercase();
        if needle.is_empty() {
            return false;
        }
        self.processes.iter().any(|p| p.contains(&needle))
    }
}

/// The profile the first matching rule asks for.
pub fn evaluate(rules: &[AutoRule], context: &RuleContext) -> Option<String> {
    rules.iter().find_map(|rule| match rule {
        AutoRule::ProcessRunning { process, profile } => {
            context.has_process(process).then(|| profile.clone())
        }
        AutoRule::Fullscreen { profile } => context.fullscreen.then(|| profile.clone()),
        AutoRule::TimeRange {
            from_minutes,
            to_minutes,
            profile,
        } => in_range(context.minutes, *from_minutes, *to_minutes).then(|| profile.clone()),
        AutoRule::OnBattery { profile } => context.on_battery.then(|| profile.clone()),
    })
}

/// Half open `[from, to)`, wrapping over midnight.
///
/// A range of 23:00 to 07:00 has to mean the night, not eight minutes.
pub fn in_range(now: u16, from: u16, to: u16) -> bool {
    let day = 24 * 60;
    let now = now % day;
    let from = from % day;
    let to = to % day;
    if from == to {
        return false;
    }
    if from < to {
        now >= from && now < to
    } else {
        now >= from || now < to
    }
}

/// Watches the system and reports when the answer changes.
///
/// The engine only acts on a change. That is what lets someone pick a profile
/// by hand and keep it: the rules do not fight them until the world actually
/// moves.
pub struct RuleWatcher {
    sampler: Sampler,
    interval: Duration,
    last_sample: Option<Instant>,
    last_match: Option<Option<String>>,
    context: RuleContext,
}

impl Default for RuleWatcher {
    fn default() -> Self {
        Self::new(Duration::from_secs(2))
    }
}

impl RuleWatcher {
    pub fn new(interval: Duration) -> Self {
        Self {
            sampler: Sampler::default(),
            interval,
            last_sample: None,
            last_match: None,
            context: RuleContext::default(),
        }
    }

    /// The most recent snapshot, for the interface.
    pub fn context(&self) -> &RuleContext {
        &self.context
    }

    /// Forget what was matched last, so the next poll acts even if the answer
    /// has not changed. Call this after the rule list is edited.
    pub fn reset(&mut self) {
        self.last_match = None;
    }

    /// Sample the system if it is time, and return a profile id when the
    /// answer has changed since the last poll.
    pub fn poll(&mut self, rules: &[AutoRule]) -> Option<String> {
        if rules.is_empty() {
            self.last_match = None;
            return None;
        }
        let due = self
            .last_sample
            .map(|t| t.elapsed() >= self.interval)
            .unwrap_or(true);
        if !due {
            return None;
        }
        self.last_sample = Some(Instant::now());
        let context = self.sampler.sample(rules);
        self.apply(rules, context)
    }

    /// Evaluate against a context that has already been gathered.
    ///
    /// Separated from sampling so the change detection can be tested without a
    /// desktop, and so a caller with its own source of truth can drive it.
    pub fn apply(&mut self, rules: &[AutoRule], context: RuleContext) -> Option<String> {
        self.context = context;
        let matched = evaluate(rules, &self.context);
        let changed = self.last_match.as_ref() != Some(&matched);
        self.last_match = Some(matched.clone());
        if changed {
            matched
        } else {
            None
        }
    }
}
