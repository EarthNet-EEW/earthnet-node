//! Travel-time phase association / hypocenter location.
//!
//! Grid-searches the hypocenter (lat, lon, **depth**) + origin time that best
//! fit P picks under a 1-D layered crustal model with straight rays
//! (`travel_time`), returning the fit and its RMS residual. A low RMS means the
//! picks are consistent with one earthquake; a high RMS means they are not
//! (random coincidence) — that is how association rejects false positives, and
//! the rejection sharpens as picks become over-determined. The layered model
//! (vs a single Vp) lets the fit also resolve source depth. Deterministic; no ML
//! (DESIGN guardrail). Full ray bending (taup-grade) is a later refinement.

use crate::geo::haversine_km;

/// 1-D layered P-velocity model: (layer top depth km, velocity km/s).
const LAYERS: [(f64, f64); 4] = [(0.0, 5.0), (5.0, 6.2), (25.0, 7.0), (60.0, 8.0)];

/// Candidate source depths (km) for the grid search.
const DEPTHS_KM: [f64; 7] = [0.0, 5.0, 15.0, 30.0, 50.0, 80.0, 120.0];

/// A located source.
#[derive(Debug, Clone, Copy)]
pub struct Hypocenter {
    pub lat: f64,
    pub lon: f64,
    pub depth_km: f64,
    pub origin_ns: i64,
    pub rms_s: f64,
    pub n: usize,
}

/// Vertical one-way travel time (s) from the surface to `depth_km` through the
/// layered model.
fn vertical_time(depth_km: f64) -> f64 {
    let mut t = 0.0;
    for (i, &(top, v)) in LAYERS.iter().enumerate() {
        if top >= depth_km {
            break;
        }
        let bottom = LAYERS.get(i + 1).map(|l| l.0).unwrap_or(f64::INFINITY);
        let seg = depth_km.min(bottom) - top;
        if seg > 0.0 {
            t += seg / v;
        }
    }
    t
}

/// Straight-ray P travel time (s) from a source at (`epi_km` epicentral
/// distance, `depth_km` depth) to a surface station.
pub fn travel_time(epi_km: f64, depth_km: f64) -> f64 {
    if depth_km <= 0.0 {
        return epi_km / LAYERS[0].1;
    }
    let path = (epi_km * epi_km + depth_km * depth_km).sqrt();
    path / depth_km * vertical_time(depth_km)
}

/// Locates the best-fitting hypocenter for picks `(lat, lon, t_ns)`.
/// Returns None for fewer than 3 picks.
pub fn locate(picks: &[(f64, f64, i64)]) -> Option<Hypocenter> {
    if picks.len() < 3 {
        return None;
    }
    let t0 = picks.iter().map(|p| p.2).min().unwrap();
    let obs: Vec<(f64, f64, f64)> = picks
        .iter()
        .map(|&(la, lo, t)| (la, lo, (t - t0) as f64 / 1e9))
        .collect();
    let clat = obs.iter().map(|o| o.0).sum::<f64>() / obs.len() as f64;
    let clon = obs.iter().map(|o| o.1).sum::<f64>() / obs.len() as f64;

    // Seed with the centroid fit (best depth) so spatially-unconstrained picks
    // keep the centroid rather than drifting to a grid corner.
    let (mut blat, mut blon, mut bdep, mut brms, mut borigin) = {
        let (depth, rms, origin) = best_depth(&obs, clat, clon);
        (clat, clon, depth, rms, origin)
    };
    // Stage 1: coarse +-3 deg @ 0.25; stage 2: fine +-0.25 @ 0.02.
    for &(span, step) in &[(3.0_f64, 0.25_f64), (0.25, 0.02)] {
        let (cla, clo) = (blat, blon);
        let steps = (2.0 * span / step) as i64;
        for i in 0..=steps {
            let la = cla - span + i as f64 * step;
            for j in 0..=steps {
                let lo = clo - span + j as f64 * step;
                let (depth, rms, origin) = best_depth(&obs, la, lo);
                if rms < brms {
                    brms = rms;
                    blat = la;
                    blon = lo;
                    bdep = depth;
                    borigin = origin;
                }
            }
        }
    }
    Some(Hypocenter {
        lat: blat,
        lon: blon,
        depth_km: bdep,
        origin_ns: t0 + (borigin * 1e9) as i64,
        rms_s: brms,
        n: picks.len(),
    })
}

/// Best (depth, rms, origin) over the depth grid for a candidate epicenter.
fn best_depth(obs: &[(f64, f64, f64)], la: f64, lo: f64) -> (f64, f64, f64) {
    let mut best = (0.0, f64::INFINITY, 0.0);
    for &depth in &DEPTHS_KM {
        let (rms, origin) = fit(obs, la, lo, depth);
        if rms < best.1 {
            best = (depth, rms, origin);
        }
    }
    best
}

/// For a candidate (epicenter, depth): best origin time and RMS residual.
fn fit(obs: &[(f64, f64, f64)], la: f64, lo: f64, depth: f64) -> (f64, f64) {
    let preds: Vec<f64> = obs
        .iter()
        .map(|&(sla, slo, t)| t - travel_time(haversine_km((la, lo), (sla, slo)), depth))
        .collect();
    let origin = preds.iter().sum::<f64>() / preds.len() as f64;
    let ss = preds.iter().map(|p| (p - origin).powi(2)).sum::<f64>();
    ((ss / preds.len() as f64).sqrt(), origin)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synth(
        lat: f64,
        lon: f64,
        depth: f64,
        origin_s: f64,
        stations: &[(f64, f64)],
    ) -> Vec<(f64, f64, i64)> {
        stations
            .iter()
            .map(|&(la, lo)| {
                let t = origin_s + travel_time(haversine_km((lat, lon), (la, lo)), depth);
                (la, lo, (t * 1e9) as i64)
            })
            .collect()
    }

    const STATIONS: [(f64, f64); 5] = [
        (-21.0, -69.5),
        (-21.3, -69.9),
        (-20.1, -69.2),
        (-22.0, -70.0),
        (-19.8, -69.7),
    ];

    #[test]
    fn locates_consistent_source_with_low_rms_and_depth() {
        let picks = synth(-21.0, -69.8, 30.0, 1000.0, &STATIONS);
        let h = locate(&picks).expect("should locate");
        assert!(h.rms_s < 0.3, "rms={}", h.rms_s);
        assert!(
            haversine_km((h.lat, h.lon), (-21.0, -69.8)) < 40.0,
            "epicenter off"
        );
        assert!((h.depth_km - 30.0).abs() <= 20.0, "depth={}", h.depth_km);
    }

    #[test]
    fn inconsistent_times_give_high_rms() {
        let picks: Vec<(f64, f64, i64)> = STATIONS
            .iter()
            .enumerate()
            .map(|(i, &(la, lo))| {
                (
                    la,
                    lo,
                    (1000.0 + [0.0, 9.0, 2.0, 11.0, 4.0][i]) as i64 * 1_000_000_000,
                )
            })
            .collect();
        let h = locate(&picks).expect("best fit");
        assert!(h.rms_s > 1.5, "incoherent picks rms={}", h.rms_s);
    }

    #[test]
    fn deeper_source_resolved() {
        let picks = synth(-21.0, -69.8, 80.0, 500.0, &STATIONS);
        let h = locate(&picks).expect("locate");
        assert!(h.rms_s < 0.3);
        assert!(
            h.depth_km >= 50.0,
            "should resolve a deep source, got {}",
            h.depth_km
        );
    }

    #[test]
    fn too_few_picks_is_none() {
        assert!(locate(&[(-21.0, -69.5, 0), (-21.3, -69.9, 1_000_000_000)]).is_none());
    }
}
