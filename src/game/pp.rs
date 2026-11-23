use crate::osu::replay::MOD_EASY;

pub fn mania_pp(stars: f64, counts: [u32; 6], mods: u32) -> f64 {
    let total: u32 = counts.iter().sum();
    if total == 0 {
        return 0.0;
    }
    let weighted = 320.0 * counts[0] as f64
        + 300.0 * counts[1] as f64
        + 200.0 * counts[2] as f64
        + 100.0 * counts[3] as f64
        + 50.0 * counts[4] as f64;
    let accuracy = weighted / (320.0 * total as f64);
    let strain = (stars - 0.15).max(0.05).powf(2.2)
        * (5.0 * accuracy - 4.0).max(0.0)
        * (1.0 + 0.1 * (total as f64 / 1500.0).min(1.0));
    let mut multiplier = 8.0;
    if mods & 1 != 0 {
        multiplier *= 0.75;
    }
    if mods & MOD_EASY != 0 {
        multiplier *= 0.5;
    }
    strain * multiplier
}
