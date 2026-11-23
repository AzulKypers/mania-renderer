pub mod pp;
pub mod stars;

use crate::osu::beatmap::Beatmap;
use crate::osu::replay::{KeyEvent, MOD_EASY, MOD_HALF_TIME, MOD_HARD_ROCK};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Judgement {
    Max,
    Great,
    Good,
    Ok,
    Meh,
    Miss,
}

impl Judgement {
    pub fn index(self) -> usize {
        match self {
            Judgement::Max => 0,
            Judgement::Great => 1,
            Judgement::Good => 2,
            Judgement::Ok => 3,
            Judgement::Meh => 4,
            Judgement::Miss => 5,
        }
    }
    fn hit_value(self) -> f64 {
        [320.0, 300.0, 200.0, 100.0, 50.0, 0.0][self.index()]
    }
    fn bonus_value(self) -> f64 {
        [32.0, 32.0, 16.0, 8.0, 4.0, 0.0][self.index()]
    }
    fn bonus_change(self) -> f64 {
        [2.0, 1.0, -8.0, -24.0, -44.0, -100.0][self.index()]
    }
    fn acc_value(self) -> f64 {
        [300.0, 300.0, 200.0, 100.0, 50.0, 0.0][self.index()]
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Windows {
    pub max: f64,
    pub great: f64,
    pub good: f64,
    pub ok: f64,
    pub meh: f64,
    pub miss: f64,
}

impl Windows {
    pub fn new(od: f64, mods: u32) -> Windows {
        let rate = crate::osu::replay::rate_for_mods(mods);
        let mut w = Windows {
            max: 16.0,
            great: 64.0 - 3.0 * od,
            good: 97.0 - 3.0 * od,
            ok: 127.0 - 3.0 * od,
            meh: 151.0 - 3.0 * od,
            miss: 188.0 - 3.0 * od,
        };
        let factor = if mods & MOD_HARD_ROCK != 0 {
            1.0 / 1.4
        } else if mods & MOD_EASY != 0 {
            1.4
        } else {
            1.0
        };
        for v in [
            &mut w.max,
            &mut w.great,
            &mut w.good,
            &mut w.ok,
            &mut w.meh,
            &mut w.miss,
        ] {
            *v = (*v * factor).floor() * rate;
        }
        w
    }

    fn judge(&self, offset: f64) -> Judgement {
        let a = offset.abs();
        if a <= self.max {
            Judgement::Max
        } else if a <= self.great {
            Judgement::Great
        } else if a <= self.good {
            Judgement::Good
        } else if a <= self.ok {
            Judgement::Ok
        } else if a <= self.meh {
            Judgement::Meh
        } else {
            Judgement::Miss
        }
    }

    fn judge_hold(&self, head: f64, tail: f64) -> Judgement {
        let (h, t) = (head.abs(), tail.abs());
        if h <= self.max * 1.2 && h + t <= self.max * 2.4 {
            Judgement::Max
        } else if h <= self.great * 1.1 && h + t <= self.great * 2.2 {
            Judgement::Great
        } else if h <= self.good && h + t <= self.good * 2.0 {
            Judgement::Good
        } else if h <= self.ok && h + t <= self.ok * 2.0 {
            Judgement::Ok
        } else {
            Judgement::Meh
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct JudgeEvent {
    pub time: f64,
    pub column: usize,
    pub kind: Judgement,
    pub offset: Option<f64>,
    pub note: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoteState {
    pub head_hit: Option<f64>,
    pub released: Option<f64>,
    pub done: Option<f64>,
    pub missed: bool,
    pub broken: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Snapshot {
    pub time: f64,
    pub score: f64,
    pub combo: u32,
    pub max_combo: u32,
    pub counts: [u32; 6],
    pub accuracy: f64,
    pub ur: f64,
    pub hp: f64,
}

pub struct Simulation {
    pub windows: Windows,
    pub judgements: Vec<JudgeEvent>,
    pub notes: Vec<NoteState>,
    pub snapshots: Vec<Snapshot>,
    pub presses: Vec<f64>,
    pub key_events: Vec<KeyEvent>,
    pub ticks: Vec<f64>,
}

pub fn simulate(map: &Beatmap, events: &[KeyEvent], mods: u32) -> Simulation {
    let w = Windows::new(map.od, mods);
    let mut states = vec![NoteState::default(); map.notes.len()];
    let mut judgements: Vec<JudgeEvent> = Vec::new();
    let mut per_column: Vec<Vec<usize>> = vec![Vec::new(); map.keys];
    for (i, note) in map.notes.iter().enumerate() {
        if note.column < map.keys {
            per_column[note.column].push(i);
        }
    }
    let mut column_events: Vec<Vec<KeyEvent>> = vec![Vec::new(); map.keys];
    for e in events {
        if e.column < map.keys {
            column_events[e.column].push(*e);
        }
    }
    for column in 0..map.keys {
        let notes = &per_column[column];
        let mut next = 0usize;
        let mut holding: Option<(usize, f64)> = None;
        let mut evs = column_events[column].clone();
        evs.push(KeyEvent {
            time: i32::MAX / 2,
            column,
            down: false,
        });
        for e in evs {
            let t = e.time as f64;
            loop {
                if let Some((ni, head)) = holding {
                    let end = map.notes[ni].end() as f64;
                    if t > end + w.meh {
                        let kind = w.judge_hold(head, w.meh);
                        states[ni].done = Some(end + w.meh);
                        states[ni].released = Some(end + w.meh);
                        judgements.push(JudgeEvent {
                            time: end + w.meh,
                            column,
                            kind,
                            offset: Some(head),
                            note: ni,
                        });
                        holding = None;
                        next += 1;
                        continue;
                    }
                    break;
                }
                if next < notes.len() {
                    let ni = notes[next];
                    let late = map.notes[ni].time as f64 + w.meh;
                    if t > late {
                        states[ni].missed = true;
                        states[ni].done = Some(late);
                        judgements.push(JudgeEvent {
                            time: late,
                            column,
                            kind: Judgement::Miss,
                            offset: None,
                            note: ni,
                        });
                        next += 1;
                        continue;
                    }
                }
                break;
            }
            if e.time >= i32::MAX / 2 {
                break;
            }
            if e.down {
                if holding.is_some() || next >= notes.len() {
                    continue;
                }
                let ni = notes[next];
                let note = &map.notes[ni];
                let offset = t - note.time as f64;
                if offset < -w.miss {
                    continue;
                }
                if offset > w.ok && next + 1 < notes.len() {
                    let nj = notes[next + 1];
                    let next_offset = t - map.notes[nj].time as f64;
                    if next_offset.abs() < offset && next_offset >= -w.meh {
                        states[ni].missed = true;
                        states[ni].done = Some(t);
                        judgements.push(JudgeEvent {
                            time: t,
                            column,
                            kind: Judgement::Miss,
                            offset: None,
                            note: ni,
                        });
                        next += 1;
                        let note = &map.notes[nj];
                        let offset = next_offset;
                        states[nj].head_hit = Some(t);
                        if note.is_long() {
                            holding = Some((nj, offset));
                        } else {
                            states[nj].done = Some(t);
                            judgements.push(JudgeEvent {
                                time: t,
                                column,
                                kind: w.judge(offset),
                                offset: Some(offset),
                                note: nj,
                            });
                            next += 1;
                        }
                        continue;
                    }
                }
                if offset < -w.meh {
                    states[ni].missed = true;
                    states[ni].done = Some(t);
                    judgements.push(JudgeEvent {
                        time: t,
                        column,
                        kind: Judgement::Miss,
                        offset: None,
                        note: ni,
                    });
                    next += 1;
                    continue;
                }
                states[ni].head_hit = Some(t);
                if note.is_long() {
                    holding = Some((ni, offset));
                } else {
                    states[ni].done = Some(t);
                    judgements.push(JudgeEvent {
                        time: t,
                        column,
                        kind: w.judge(offset),
                        offset: Some(offset),
                        note: ni,
                    });
                    next += 1;
                }
            } else if let Some((ni, head)) = holding {
                let end = map.notes[ni].end() as f64;
                let tail = t - end;
                let kind = if tail < -w.meh {
                    states[ni].broken = true;
                    Judgement::Meh
                } else {
                    w.judge_hold(head, tail)
                };
                states[ni].released = Some(t);
                states[ni].done = Some(t);
                judgements.push(JudgeEvent {
                    time: t,
                    column,
                    kind,
                    offset: Some(head),
                    note: ni,
                });
                holding = None;
                next += 1;
            }
        }
    }
    judgements.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());

    let mut ticks = Vec::new();
    for (i, note) in map.notes.iter().enumerate() {
        let Some(end) = note.end_time else { continue };
        let st = &states[i];
        let Some(head) = st.head_hit else { continue };
        let stop = st.released.unwrap_or(end as f64).min(end as f64);
        let spacing = 100.0;
        let mut t = head + spacing;
        while t <= stop {
            ticks.push(t);
            t += spacing;
        }
    }
    ticks.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let total = map.notes.len().max(1) as f64;
    let multiplier = if mods & (MOD_EASY | MOD_HALF_TIME) != 0 {
        0.5
    } else {
        1.0
    };
    let unit = 1_000_000.0 * multiplier * 0.5 / total;
    let mut snapshots = Vec::with_capacity(judgements.len() + ticks.len() + 1);
    let mut s = Snapshot {
        accuracy: 100.0,
        hp: 1.0,
        ..Default::default()
    };
    let drain = map.hp;
    snapshots.push(s);
    let mut bonus = 100.0f64;
    let mut base = 0.0;
    let mut extra = 0.0;
    let mut acc_sum = 0.0;
    let mut offsets: Vec<f64> = Vec::new();
    let (mut ji, mut ti) = (0usize, 0usize);
    while ji < judgements.len() || ti < ticks.len() {
        let take_tick =
            ti < ticks.len() && (ji >= judgements.len() || ticks[ti] < judgements[ji].time);
        if take_tick {
            s.time = ticks[ti];
            s.combo += 1;
            ti += 1;
        } else {
            let j = judgements[ji];
            ji += 1;
            s.time = j.time;
            let k = j.kind;
            bonus = (bonus + k.bonus_change()).clamp(0.0, 100.0);
            base += unit * k.hit_value() / 320.0;
            extra += unit * k.bonus_value() * bonus.sqrt() / 320.0;
            s.score = base + extra;
            s.counts[k.index()] += 1;
            let delta = match k {
                Judgement::Max => 0.006,
                Judgement::Great => 0.005,
                Judgement::Good => 0.002,
                Judgement::Ok => 0.0,
                Judgement::Meh => -0.02 - drain * 0.002,
                Judgement::Miss => -0.04 - drain * 0.008,
            };
            s.hp = (s.hp + delta).clamp(0.0, 1.0);
            acc_sum += k.acc_value();
            let judged: u32 = s.counts.iter().sum();
            s.accuracy = acc_sum / (300.0 * judged as f64) * 100.0;
            let broken = states[j.note].broken;
            if k == Judgement::Miss || broken {
                s.combo = 0;
            } else {
                s.combo += 1;
            }
            if let Some(o) = j.offset {
                offsets.push(o / crate::osu::replay::rate_for_mods(mods));
                let n = offsets.len() as f64;
                let mean = offsets.iter().sum::<f64>() / n;
                let var = offsets.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
                s.ur = var.sqrt() * 10.0;
            }
        }
        s.max_combo = s.max_combo.max(s.combo);
        snapshots.push(s);
    }

    let presses: Vec<f64> = events
        .iter()
        .filter(|e| e.down)
        .map(|e| e.time as f64)
        .collect();
    Simulation {
        windows: w,
        judgements,
        notes: states,
        snapshots,
        presses,
        key_events: events.to_vec(),
        ticks,
    }
}

impl Simulation {
    pub fn snapshot_at(&self, time: f64) -> Snapshot {
        let idx = self.snapshots.partition_point(|s| s.time <= time);
        self.snapshots[idx.saturating_sub(1).max(0)]
    }

    pub fn final_snapshot(&self) -> Snapshot {
        *self.snapshots.last().unwrap()
    }

    pub fn kps_at(&self, time: f64) -> (usize, usize) {
        let total = self.presses.partition_point(|&p| p <= time);
        let start = self.presses.partition_point(|&p| p <= time - 1000.0);
        (total - start, total)
    }

    pub fn last_judgement_before(&self, time: f64) -> Option<&JudgeEvent> {
        let idx = self.judgements.partition_point(|j| j.time <= time);
        if idx == 0 {
            None
        } else {
            Some(&self.judgements[idx - 1])
        }
    }

    pub fn key_down(&self, column: usize, time: f64) -> bool {
        let mut down = false;
        for e in &self.key_events {
            if e.time as f64 > time {
                break;
            }
            if e.column == column {
                down = e.down;
            }
        }
        down
    }
}

pub fn autoplay_events(map: &Beatmap) -> Vec<KeyEvent> {
    let mut events = Vec::new();
    let mut by_column: Vec<Vec<&crate::osu::beatmap::Note>> = vec![Vec::new(); map.keys];
    for n in &map.notes {
        by_column[n.column].push(n);
    }
    for (column, notes) in by_column.iter().enumerate() {
        for (i, n) in notes.iter().enumerate() {
            let next = notes.get(i + 1).map(|m| m.time).unwrap_or(i32::MAX);
            let release = match n.end_time {
                Some(e) => e,
                None => (n.time + 40).min(next - 1).max(n.time + 1),
            };
            events.push(KeyEvent {
                time: n.time,
                column,
                down: true,
            });
            events.push(KeyEvent {
                time: release,
                column,
                down: false,
            });
        }
    }
    events.sort_by_key(|e| (e.time, e.down));
    events
}
