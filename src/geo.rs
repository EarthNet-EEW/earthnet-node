//! Minimal geospatial helpers for consensus correlation.

const BASE32: &[u8] = b"0123456789bcdefghjkmnpqrstuvwxyz";

/// Decodes a geohash to its approximate center `(lat, lon)` in degrees.
/// Returns None on an invalid character.
pub fn decode_geohash(geohash: &str) -> Option<(f64, f64)> {
    let mut lat = (-90.0_f64, 90.0_f64);
    let mut lon = (-180.0_f64, 180.0_f64);
    let mut even = true;
    for c in geohash.bytes() {
        let idx = BASE32.iter().position(|&b| b == c)?;
        for bit in (0..5).rev() {
            let set = (idx >> bit) & 1 == 1;
            if even {
                let mid = (lon.0 + lon.1) / 2.0;
                if set {
                    lon.0 = mid;
                } else {
                    lon.1 = mid;
                }
            } else {
                let mid = (lat.0 + lat.1) / 2.0;
                if set {
                    lat.0 = mid;
                } else {
                    lat.1 = mid;
                }
            }
            even = !even;
        }
    }
    Some(((lat.0 + lat.1) / 2.0, (lon.0 + lon.1) / 2.0))
}

/// Encodes `(lat, lon)` to a geohash of `len` characters.
pub fn encode_geohash(lat: f64, lon: f64, len: usize) -> String {
    let mut lat_lo = -90.0;
    let mut lat_hi = 90.0;
    let mut lon_lo = -180.0;
    let mut lon_hi = 180.0;
    let mut out = String::with_capacity(len);
    let mut bit = 0u8;
    let mut ch = 0usize;
    let mut even = true;
    while out.len() < len {
        if even {
            let mid = (lon_lo + lon_hi) / 2.0;
            if lon > mid {
                ch |= 1 << (4 - bit);
                lon_lo = mid;
            } else {
                lon_hi = mid;
            }
        } else {
            let mid = (lat_lo + lat_hi) / 2.0;
            if lat > mid {
                ch |= 1 << (4 - bit);
                lat_lo = mid;
            } else {
                lat_hi = mid;
            }
        }
        even = !even;
        if bit < 4 {
            bit += 1;
        } else {
            out.push(BASE32[ch] as char);
            bit = 0;
            ch = 0;
        }
    }
    out
}

/// Great-circle distance between two `(lat, lon)` points, in kilometers.
pub fn haversine_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    const R: f64 = 6371.0;
    let (lat1, lon1) = a;
    let (lat2, lon2) = b;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let h = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * R * h.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_known_geohash() {
        // "66jd2" is in northern Chile; check it lands in a sane bounding box.
        let (lat, lon) = decode_geohash("66jd2").unwrap();
        assert!((-35.0..-15.0).contains(&lat), "lat={lat}");
        assert!((-75.0..-65.0).contains(&lon), "lon={lon}");
    }

    #[test]
    fn invalid_char_returns_none() {
        assert!(decode_geohash("abcil").is_none()); // 'i', 'l' not in base32
    }

    #[test]
    fn encode_decode_roundtrip() {
        let gh = encode_geohash(-21.3, -69.9, 7);
        let (lat, lon) = decode_geohash(&gh).unwrap();
        assert!((lat - -21.3).abs() < 0.05, "lat={lat}");
        assert!((lon - -69.9).abs() < 0.05, "lon={lon}");
    }

    #[test]
    fn haversine_zero_and_known() {
        assert!(haversine_km((0.0, 0.0), (0.0, 0.0)) < 1e-9);
        // ~1 degree of latitude ~= 111 km
        let d = haversine_km((0.0, 0.0), (1.0, 0.0));
        assert!((d - 111.0).abs() < 2.0, "d={d}");
    }
}
