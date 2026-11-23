use crate::osu::beatmap::Beatmap;

const INDIVIDUAL_DECAY: f64 = 0.125;
const OVERALL_DECAY: f64 = 0.30;
const RELEASE_THRESHOLD: f64 = 30.0;
const SECTION: f64 = 400.0;
const STAR_SCALING: f64 = 0.018;

fn decay(value: f64, dt: f64, base: f64) -> f64 {
    value * base.powf(dt / 1000.0)
}

fn bigger(a: f64, b: f64) -> bool {
    a - b > 1.0
}

pub fn star_rating(map: &Beatmap, rate: f64) -> f64 {
    star_rating_until(map, rate, f64::MAX)
}

pub fn star_rating_until(map: &Beatmap, rate: f64, until: f64) -> f64 {
    let keys = map.keys.max(1);
    let mut notes: Vec<(f64, f64, usize)> = map
        .notes
        .iter()
        .filter(|n| (n.time as f64) <= until)
        .map(|n| {
            (
                n.time as f64 / rate,
                n.end() as f64 / rate,
                n.column.min(keys - 1),
            )
        })
        .collect();
    notes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.2.cmp(&b.2)));
    if notes.len() < 2 {
        return 0.0;
    }
    let mut start_times = vec![0.0f64; keys];
    let mut end_times = vec![0.0f64; keys];
    let mut individual_strains = vec![0.0f64; keys];
    let mut individual = 0.0f64;
    let mut overall = 1.0f64;
    let mut peaks: Vec<f64> = Vec::new();
    let mut section_end = (notes[1].0 / SECTION).ceil() * SECTION;
    let mut peak = 0.0f64;
    let mut prev_start = notes[0].0;

    for i in 1..notes.len() {
        let (start, end, column) = notes[i];
        let delta = start - notes[i - 1].0;
        while start > section_end {
            peaks.push(peak);
            peak = decay(individual, section_end - prev_start, INDIVIDUAL_DECAY)
                + decay(overall, section_end - prev_start, OVERALL_DECAY);
            section_end += SECTION;
        }
        let mut overlapping = false;
        let mut closest_end = (end - start).abs();
        let mut hold_factor = 1.0;
        let mut hold_addition = 0.0;
        for c in 0..keys {
            overlapping |= bigger(end_times[c], start)
                && bigger(end, end_times[c])
                && bigger(start, start_times[c]);
            if bigger(end_times[c], end) && bigger(start, start_times[c]) {
                hold_factor = 1.25;
            }
            closest_end = closest_end.min((end - end_times[c]).abs());
        }
        if overlapping {
            hold_addition = 1.0 / (1.0 + (0.5 * (RELEASE_THRESHOLD - closest_end)).exp());
        }
        individual_strains[column] = decay(
            individual_strains[column],
            start - start_times[column],
            INDIVIDUAL_DECAY,
        ) + 2.0 * hold_factor;
        individual = if delta <= 1.0 {
            individual.max(individual_strains[column])
        } else {
            individual_strains[column]
        };
        overall = decay(overall, delta, OVERALL_DECAY) + (1.0 + hold_addition) * hold_factor;
        start_times[column] = start;
        end_times[column] = end;
        peak = peak.max(individual + overall);
        prev_start = start;
    }
    peaks.push(peak);
    peaks.sort_by(|a, b| b.partial_cmp(a).unwrap());
    let mut weight = 1.0;
    let mut difficulty = 0.0;
    for p in peaks {
        difficulty += p * weight;
        weight *= 0.9;
    }
    difficulty * STAR_SCALING
}
