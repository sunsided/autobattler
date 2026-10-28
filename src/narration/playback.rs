//! Writes narration beats out with pauses between them.

use super::{Beat, BeatKind};
use std::io::{self, Write};
use std::time::Duration;

/// How long to linger after a beat of the given kind, if at all.
fn pause_after(kind: BeatKind) -> Option<Duration> {
    match kind {
        BeatKind::Action => Some(Duration::from_millis(400)),
        BeatKind::Panel => Some(Duration::from_millis(800)),
        BeatKind::Intro | BeatKind::RoundHeader | BeatKind::Outro | BeatKind::Stats => None,
    }
}

/// Writes every beat to `out`, flushing after each and pausing after actions and panels.
pub fn play<W: Write>(
    beats: &[Beat],
    out: &mut W,
    mut pause: impl FnMut(Duration),
) -> io::Result<()> {
    for beat in beats {
        for line in &beat.lines {
            writeln!(out, "{line}")?;
        }
        out.flush()?;
        if let Some(duration) = pause_after(beat.kind) {
            pause(duration);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    fn beat(kind: BeatKind, lines: &[&str]) -> Beat {
        Beat {
            kind,
            lines: lines.iter().map(|l| l.to_string()).collect(),
            event: None,
        }
    }

    #[test]
    fn pauses_after_actions_and_panels() {
        let beats = vec![
            beat(BeatKind::Intro, &["intro"]),
            beat(BeatKind::RoundHeader, &["", "round 1"]),
            beat(BeatKind::Action, &["first"]),
            beat(BeatKind::Action, &["second", "  note"]),
            beat(BeatKind::Panel, &["", "panel"]),
            beat(BeatKind::Outro, &["", "outro"]),
            beat(BeatKind::Stats, &["stats"]),
        ];
        let mut pauses = vec![];
        let mut out = Vec::new();
        play(&beats, &mut out, |d| pauses.push(d)).unwrap();
        assert_eq!(pauses, vec![ms(400), ms(400), ms(800)]);
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "intro\n\nround 1\nfirst\nsecond\n  note\n\npanel\n\noutro\nstats\n"
        );
    }
}
