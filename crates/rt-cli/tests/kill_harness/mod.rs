//! Kill harness shared by the V-1 and V-4 tests (UD-45 section 3).
//! Standard library only; test code, with no hook in production code.
//!
//! Each trial starts the real executable, kills it with `Child::kill`
//! (SIGKILL on Unix, TerminateProcess on Windows) after a delay drawn from a
//! fixed-seed SplitMix64 sequence, and classifies what the kill left on disk.
//! A scenario then has to show that the next run accepts or recovers that
//! state. The result is statistical evidence, not a proof for every instant.
//!
//! The delay range comes from uncut runs. A poller thread watches the
//! scenario's directory while the executable runs, so the range starts a
//! little before the first moment a write file becomes visible and ends a
//! little after the end of the whole run.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::lifeline_rig::{spawn_direct, Scratch, Target};
use std::collections::BTreeMap;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

/// Trials per scenario and platform (UD-45: at least 100).
pub const TRIALS: u32 = 110;
const WARMUPS: u32 = 2;
const MEASURED_RUNS: u32 = 10;

/// SplitMix64 (Steele, Lea and Flood), with the published constants.
pub struct SplitMix64(u64);

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// Where the state left by a kill lies relative to the write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Before,
    During,
    After,
}

/// What a kill left on disk. `detail` names the exact files and their state.
#[derive(Clone, Debug)]
pub struct Observation {
    pub phase: Phase,
    pub detail: String,
}

/// How a trial's process ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ended {
    Killed,
    /// Exited with status 0 before the kill, so the trial is not a kill.
    Completed,
}

/// The result of a scenario's acceptance check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Accepted,
    /// A refusal that the design documents (see the scenario). It is
    /// counted and printed separately and never hidden in `Accepted`.
    RefusedByDesign,
}

/// One kill scenario. `trial` selects a variant where a scenario has two.
pub trait Scenario: Sync {
    fn id(&self) -> &'static str;
    fn seed(&self) -> u64;
    /// A fresh scratch directory holding the start state.
    fn prepare(&self, trial: u32) -> Scratch;
    fn target(&self, dir: &Scratch, trial: u32) -> Target;
    /// Cheap check used by the poller: has the process made any write file
    /// visible yet?
    fn touched(&self, dir: &Scratch, trial: u32) -> bool;
    /// Classifies the state after a kill. An `Err` is a state the design
    /// never allows, such as a partial file at a final name.
    fn observe(&self, dir: &Scratch, trial: u32) -> Result<Observation, String>;
    /// The next run must accept or recover the state.
    fn accept(
        &self,
        dir: &Scratch,
        trial: u32,
        seen: &Observation,
        ended: Ended,
    ) -> Result<Verdict, String>;
}

/// The delay range of the trials, from uncut runs.
#[derive(Clone, Copy, Debug)]
pub struct Range {
    pub lo: Duration,
    pub hi: Duration,
    pub run_min: Duration,
    pub run_median: Duration,
    pub run_max: Duration,
    pub onset_min: Duration,
    pub onset_max: Duration,
}

impl Range {
    pub fn draw(&self, rng: &mut SplitMix64) -> Duration {
        let span = u64::try_from((self.hi - self.lo).as_nanos()).unwrap() + 1;
        self.lo + Duration::from_nanos(rng.next_u64() % span)
    }
}

pub fn say(line: &str) {
    drop(writeln!(std::io::stdout(), "{line}"));
}

/// `os-arch`, for the summary line.
pub fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

// Sleeps for most of the delay and spins for the rest, because a sleep alone
// can overshoot by more than the whole write phase.
fn wait_until(start: Instant, delay: Duration) {
    let coarse = delay.saturating_sub(Duration::from_micros(700));
    if let Some(rest) = coarse.checked_sub(start.elapsed()) {
        thread::sleep(rest);
    }
    while start.elapsed() < delay {
        std::hint::spin_loop();
    }
}

fn is_kill_status(code: Option<i32>) -> bool {
    // Unix: no code (signal). Windows: TerminateProcess leaves exit code 1.
    code.is_none() || (cfg!(windows) && code == Some(1))
}

/// Starts the target and kills it after `delay`.
pub fn kill_after(target: &Target, delay: Duration) -> Result<Ended, String> {
    let mut run = spawn_direct(target);
    let start = Instant::now();
    wait_until(start, delay);
    if let Some(status) = run.child.try_wait().map_err(|e| e.to_string())? {
        return if status.code() == Some(0) {
            Ok(Ended::Completed)
        } else {
            Err(format!(
                "the process ended by itself with {status}: {}",
                run.stderr()
            ))
        };
    }
    run.child.kill().map_err(|e| e.to_string())?;
    let status = run.child.wait().map_err(|e| e.to_string())?;
    if status.code() == Some(0) {
        Ok(Ended::Completed)
    } else if is_kill_status(status.code()) {
        Ok(Ended::Killed)
    } else {
        Err(format!(
            "the process ended with {status} instead of the kill"
        ))
    }
}

/// Uncut runs: whole duration and the first moment a write file shows.
pub fn measure(s: &dyn Scenario) -> Range {
    let mut durations = Vec::new();
    let mut onsets = Vec::new();
    for k in 0..WARMUPS + MEASURED_RUNS {
        let trial = 1000 + k;
        let dir = s.prepare(trial);
        let target = s.target(&dir, trial);
        let seen: Mutex<Option<Instant>> = Mutex::new(None);
        let stop = AtomicBool::new(false);
        let (start, elapsed) = thread::scope(|scope| {
            scope.spawn(|| {
                while !stop.load(Ordering::SeqCst) {
                    if s.touched(&dir, trial) {
                        *seen.lock().unwrap() = Some(Instant::now());
                        break;
                    }
                    std::hint::spin_loop();
                }
            });
            let mut run = spawn_direct(&target);
            let start = Instant::now();
            let status = run.child.wait().unwrap();
            let elapsed = start.elapsed();
            stop.store(true, Ordering::SeqCst);
            assert_eq!(
                status.code(),
                Some(0),
                "{} uncut run failed: {}",
                s.id(),
                run.stderr()
            );
            (start, elapsed)
        });
        if k >= WARMUPS {
            durations.push(elapsed);
            let onset = seen
                .lock()
                .unwrap()
                .map(|t| t.saturating_duration_since(start));
            onsets.push(onset.unwrap_or(elapsed));
        }
    }
    let mut sorted = durations.clone();
    sorted.sort();
    let run_median = *sorted.get(sorted.len() / 2).unwrap();
    let run_min = *durations.iter().min().unwrap();
    let run_max = *durations.iter().max().unwrap();
    let onset_min = *onsets.iter().min().unwrap();
    let onset_max = *onsets.iter().max().unwrap();
    // Start at half of the earliest onset, end a tenth after the median
    // run (one slow outlier must not stretch the range), so that trials
    // also land before and after the write.
    let lo = onset_min / 2;
    let hi = (run_median + run_median / 10).max(lo + Duration::from_millis(1));
    Range {
        lo,
        hi,
        run_min,
        run_median,
        run_max,
        onset_min,
        onset_max,
    }
}

#[derive(Default)]
pub struct Tally {
    pub before: u32,
    pub during: u32,
    pub after: u32,
    pub completed: u32,
    pub refused_by_design: u32,
    pub max_delay: Duration,
    pub details: BTreeMap<String, u32>,
}

/// Runs the trials, fails on the first rejected or unrecoverable state and
/// prints the summary. `INCONCLUSIVE` is printed, not failed, when no trial
/// landed during the write.
pub fn drive(s: &dyn Scenario, trials: u32) -> Tally {
    assert!(trials >= 100, "UD-45 requires at least 100 trials");
    let seed = s.seed();
    let range = measure(s);
    say(&format!(
        "kill-harness {} range lo={:?} hi={:?} run min={:?} median={:?} max={:?} onset min={:?} max={:?}",
        s.id(),
        range.lo,
        range.hi,
        range.run_min,
        range.run_median,
        range.run_max,
        range.onset_min,
        range.onset_max
    ));
    let mut rng = SplitMix64::new(seed);
    let mut tally = Tally::default();
    for trial in 1..=trials {
        let delay = range.draw(&mut rng);
        let at = |what: &str| {
            format!(
                "{} trial {trial} seed {seed:#x} delay {delay:?}: {what}",
                s.id()
            )
        };
        let dir = s.prepare(trial);
        let target = s.target(&dir, trial);
        let ended = kill_after(&target, delay).unwrap_or_else(|e| panic!("{}", at(&e)));
        let seen = s
            .observe(&dir, trial)
            .unwrap_or_else(|e| panic!("{}", at(&format!("state left by the kill: {e}"))));
        let verdict = s.accept(&dir, trial, &seen, ended).unwrap_or_else(|e| {
            panic!(
                "{}",
                at(&format!(
                    "{} ({}): {e}",
                    seen.detail,
                    phase_name(seen.phase)
                ))
            )
        });
        tally.max_delay = tally.max_delay.max(delay);
        match ended {
            Ended::Completed => {
                assert_eq!(
                    seen.phase,
                    Phase::After,
                    "{}",
                    at("a process that exited by itself left a state that is not final")
                );
                tally.completed += 1;
            }
            Ended::Killed => {
                match seen.phase {
                    Phase::Before => tally.before += 1,
                    Phase::During => tally.during += 1,
                    Phase::After => tally.after += 1,
                }
                *tally.details.entry(seen.detail.clone()).or_insert(0) += 1;
            }
        }
        if verdict == Verdict::RefusedByDesign {
            tally.refused_by_design += 1;
        }
    }
    summarize(s, trials, &range, &tally);
    tally
}

fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Before => "before",
        Phase::During => "during",
        Phase::After => "after",
    }
}

fn summarize(s: &dyn Scenario, trials: u32, range: &Range, t: &Tally) {
    let killed = t.before + t.during + t.after;
    say(&format!(
        "kill-harness {} platform={} seed={:#x} N={trials} killed={killed} before={} during={} after={} completed={} refused_by_design={} max_delay={:?} (range {:?}..{:?})",
        s.id(),
        platform(),
        s.seed(),
        t.before,
        t.during,
        t.after,
        t.completed,
        t.refused_by_design,
        t.max_delay,
        range.lo,
        range.hi
    ));
    for (detail, count) in &t.details {
        say(&format!(
            "kill-harness {} state {count:>4} x {detail}",
            s.id()
        ));
    }
    if t.during == 0 {
        say(&format!(
            "INCONCLUSIVE {} platform={}: no trial landed during the write ({} kills)",
            s.id(),
            platform(),
            killed
        ));
    }
}

/// Checks of the harness itself, run by both scenario binaries.
pub fn self_checks() -> Vec<(&'static str, fn())> {
    vec![
        ("splitmix64_matches_an_independent_implementation", || {
            // Values from an independent Python implementation of the
            // published algorithm, seed 0.
            let mut rng = SplitMix64::new(0);
            assert_eq!(rng.next_u64(), 0xe220_a839_7b1d_cdaf);
            assert_eq!(rng.next_u64(), 0x6e78_9e6a_a1b9_65f4);
            assert_eq!(rng.next_u64(), 0x06c4_5d18_8009_454f);
        }),
        ("delays_stay_inside_the_range_and_repeat_for_a_seed", || {
            let range = Range {
                lo: Duration::from_micros(100),
                hi: Duration::from_micros(900),
                run_min: Duration::ZERO,
                run_median: Duration::ZERO,
                run_max: Duration::ZERO,
                onset_min: Duration::ZERO,
                onset_max: Duration::ZERO,
            };
            let draw = |seed| {
                let mut rng = SplitMix64::new(seed);
                (0..500).map(|_| range.draw(&mut rng)).collect::<Vec<_>>()
            };
            let first = draw(7);
            assert_eq!(first, draw(7));
            assert_ne!(first, draw(8));
            assert!(first.iter().all(|d| *d >= range.lo && *d <= range.hi));
            // The draws spread over the range instead of clustering.
            let low = first
                .iter()
                .filter(|d| **d < Duration::from_micros(300))
                .count();
            let high = first
                .iter()
                .filter(|d| **d > Duration::from_micros(700))
                .count();
            assert!(low > 80 && high > 80, "low {low} high {high}");
        }),
    ]
}
