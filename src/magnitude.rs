//! First-order magnitude estimation.
//!
//! Official sources carry an authoritative magnitude (used directly). For
//! phone-only consensus there is none, so we estimate from peak ground
//! acceleration and distance via an inverted GMPE-lite relation:
//!
//!   M ≈ A·log10(PGA[cm/s²]) + B·log10(R[km]) + C
//!
//! ⚠️ PROVISIONAL: the coefficients are illustrative, anchored to a couple of
//! plausible points, NOT regionally calibrated. They produce sane-magnitude
//! numbers for sane inputs but MUST be calibrated against real catalogs before
//! any operational use. A wrong magnitude erodes trust, so consensus estimates
//! are reported with a large uncertainty.

const A: f64 = 1.94;
const B: f64 = 1.5;
const C: f64 = 0.01;
const G_TO_CM_S2: f64 = 980.665;

/// Provisional magnitude from PGA (in g) and hypocentral distance (km).
/// Returns 0.0 when PGA is non-positive.
pub fn estimate_from_pga(pga_g: f32, distance_km: f64) -> f32 {
    if pga_g <= 0.0 {
        return 0.0;
    }
    let pga = pga_g as f64 * G_TO_CM_S2;
    let r = distance_km.max(1.0);
    let m = A * pga.log10() + B * r.log10() + C;
    m.clamp(0.0, 10.0) as f32
}

/// Uncertainty (magnitude units) attached to a provisional PGA estimate.
pub const PROVISIONAL_UNCERT: f32 = 0.8;
/// Uncertainty attached to an authoritative official magnitude.
pub const OFFICIAL_UNCERT: f32 = 0.2;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_pga_yields_zero() {
        assert_eq!(estimate_from_pga(0.0, 30.0), 0.0);
    }

    #[test]
    fn monotonic_in_pga() {
        let weak = estimate_from_pga(0.01, 30.0);
        let strong = estimate_from_pga(0.3, 30.0);
        assert!(strong > weak, "weak={weak} strong={strong}");
    }

    #[test]
    fn produces_sane_range() {
        // a moderate near-ish shake should land in a believable magnitude band
        let m = estimate_from_pga(0.05, 35.0);
        assert!((3.0..=8.0).contains(&m), "m={m}");
    }

    #[test]
    fn clamped_to_ten() {
        assert!(estimate_from_pga(50.0, 1.0) <= 10.0);
    }
}
