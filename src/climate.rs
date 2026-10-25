//! Köppen-Geiger classification from monthly climate normals.
//!
//! Monthly precipitation here is a deterministic procedural estimate, not measured
//! climatology. The classifier follows the standard temperature/rainfall thresholds;
//! the planet's climate inputs still require calibration against real climate datasets.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MonthlyClimateNormals {
    /// Mean temperature for January through December, in degrees Celsius.
    pub temperature_c: [f64; 12],
    /// Mean precipitation for January through December, in millimetres.
    pub precipitation_mm: [f64; 12],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KoppenClass {
    Af, Am, Aw, As,
    BWh, BWk, BSh, BSk,
    Csa, Csb, Csc, Cwa, Cwb, Cwc, Cfa, Cfb, Cfc,
    Dsa, Dsb, Dsc, Dsd, Dwa, Dwb, Dwc, Dwd, Dfa, Dfb, Dfc, Dfd,
    ET, EF, Highland,
}

impl KoppenClass {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Af => "Af", Self::Am => "Am", Self::Aw => "Aw", Self::As => "As",
            Self::BWh => "BWh", Self::BWk => "BWk", Self::BSh => "BSh", Self::BSk => "BSk",
            Self::Csa => "Csa", Self::Csb => "Csb", Self::Csc => "Csc",
            Self::Cwa => "Cwa", Self::Cwb => "Cwb", Self::Cwc => "Cwc",
            Self::Cfa => "Cfa", Self::Cfb => "Cfb", Self::Cfc => "Cfc",
            Self::Dsa => "Dsa", Self::Dsb => "Dsb", Self::Dsc => "Dsc", Self::Dsd => "Dsd",
            Self::Dwa => "Dwa", Self::Dwb => "Dwb", Self::Dwc => "Dwc", Self::Dwd => "Dwd",
            Self::Dfa => "Dfa", Self::Dfb => "Dfb", Self::Dfc => "Dfc", Self::Dfd => "Dfd",
            Self::ET => "ET", Self::EF => "EF", Self::Highland => "H",
        }
    }

    pub const fn is_tropical(self) -> bool {
        matches!(self, Self::Af | Self::Am | Self::Aw | Self::As)
    }

    pub const fn is_arid(self) -> bool {
        matches!(self, Self::BWh | Self::BWk | Self::BSh | Self::BSk)
    }

    pub const fn is_temperate(self) -> bool {
        matches!(self, Self::Csa | Self::Csb | Self::Csc | Self::Cwa | Self::Cwb | Self::Cwc | Self::Cfa | Self::Cfb | Self::Cfc)
    }

    pub const fn is_continental(self) -> bool {
        matches!(self, Self::Dsa | Self::Dsb | Self::Dsc | Self::Dsd | Self::Dwa | Self::Dwb | Self::Dwc | Self::Dwd | Self::Dfa | Self::Dfb | Self::Dfc | Self::Dfd)
    }
}

impl MonthlyClimateNormals {
    pub fn classify(&self, elevation_m: f64) -> KoppenClass {
        let t = self.temperature_c;
        let p = self.precipitation_mm;
        if t.iter().chain(p.iter()).any(|v| !v.is_finite()) {
            return KoppenClass::Highland;
        }
        let warmest = t.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let coldest = t.iter().copied().fold(f64::INFINITY, f64::min);
        let annual_temp = t.iter().sum::<f64>() / 12.0;
        let annual_precip = p.iter().map(|v| v.max(0.0)).sum::<f64>();
        let driest = p.iter().copied().fold(f64::INFINITY, f64::min).max(0.0);
        let wettest = p.iter().copied().fold(f64::NEG_INFINITY, f64::max).max(0.0);
        // Approximate summer half-year from the warmer six monthly means.
        let mut sorted: Vec<(f64, f64)> = t.iter().copied().zip(p.iter().copied()).collect();
        sorted.sort_by(|a, b| b.0.total_cmp(&a.0));
        let summer_rain = sorted.iter().take(6).map(|(_, rain)| rain.max(0.0)).sum::<f64>();
        let summer_fraction = if annual_precip > 0.0 { summer_rain / annual_precip } else { 0.5 };
        let aridity_threshold = 20.0 * annual_temp + if summer_fraction >= 0.7 { 280.0 } else if summer_fraction >= 0.3 { 140.0 } else { 0.0 };

        if elevation_m >= 3500.0 { return KoppenClass::Highland; }
        if warmest < 0.0 { return KoppenClass::EF; }
        if warmest < 10.0 { return KoppenClass::ET; }

        if annual_precip < aridity_threshold {
            let desert = annual_precip < aridity_threshold * 0.5;
            return match (desert, annual_temp >= 18.0) {
                (true, true) => KoppenClass::BWh,
                (true, false) => KoppenClass::BWk,
                (false, true) => KoppenClass::BSh,
                (false, false) => KoppenClass::BSk,
            };
        }

        if coldest >= 18.0 {
            if driest >= 60.0 { return KoppenClass::Af; }
            if driest >= 100.0 - annual_precip / 25.0 { return KoppenClass::Am; }
            return if summer_fraction >= 0.5 { KoppenClass::Aw } else { KoppenClass::As };
        }

        let dry_summer = {
            let summer_dry = p.iter().zip(t.iter()).filter(|(_, temp)| **temp > annual_temp)
                .map(|(rain, _)| *rain).fold(f64::INFINITY, f64::min);
            let winter_wet = p.iter().zip(t.iter()).filter(|(_, temp)| **temp <= annual_temp)
                .map(|(rain, _)| *rain).fold(0.0_f64, f64::max);
            summer_dry < 40.0 && winter_wet > summer_dry * 3.0
        };
        let dry_winter = {
            let winter_dry = p.iter().zip(t.iter()).filter(|(_, temp)| **temp <= annual_temp)
                .map(|(rain, _)| *rain).fold(f64::INFINITY, f64::min);
            let summer_wet = p.iter().zip(t.iter()).filter(|(_, temp)| **temp > annual_temp)
                .map(|(rain, _)| *rain).fold(0.0_f64, f64::max);
            winter_dry < summer_wet / 10.0
        };
        let group_c = coldest > -3.0;
        let third = if warmest >= 22.0 { 'a' } else if t.iter().filter(|v| **v > 10.0).count() >= 4 { 'b' } else { 'c' };
        let group = if group_c { 'C' } else { 'D' };
        match (group, dry_summer, dry_winter, third) {
            ('C', true, _, 'a') => KoppenClass::Csa, ('C', true, _, 'b') => KoppenClass::Csb, ('C', true, _, _) => KoppenClass::Csc,
            ('C', _, true, 'a') => KoppenClass::Cwa, ('C', _, true, 'b') => KoppenClass::Cwb, ('C', _, true, _) => KoppenClass::Cwc,
            ('C', _, _, 'a') => KoppenClass::Cfa, ('C', _, _, 'b') => KoppenClass::Cfb, ('C', _, _, _) => KoppenClass::Cfc,
            ('D', true, _, 'a') => KoppenClass::Dsa, ('D', true, _, 'b') => KoppenClass::Dsb, ('D', true, _, _) => KoppenClass::Dsc,
            ('D', _, true, 'a') => KoppenClass::Dwa, ('D', _, true, 'b') => KoppenClass::Dwb, ('D', _, true, _) => KoppenClass::Dwc,
            ('D', _, _, 'a') => KoppenClass::Dfa, ('D', _, _, 'b') => KoppenClass::Dfb, ('D', _, _, _) => KoppenClass::Dfc,
            _ => KoppenClass::Dfd,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normals(temp: [f64; 12], rain: [f64; 12]) -> MonthlyClimateNormals {
        MonthlyClimateNormals { temperature_c: temp, precipitation_mm: rain }
    }

    #[test]
    fn recognizes_tropical_rainforest() {
        assert_eq!(normals([26.0; 12], [120.0; 12]).classify(50.0), KoppenClass::Af);
    }

    #[test]
    fn recognizes_hot_desert() {
        assert_eq!(normals([30.0; 12], [2.0; 12]).classify(100.0), KoppenClass::BWh);
    }

    #[test]
    fn recognizes_tundra_and_ice_cap() {
        assert_eq!(normals([-12.0; 12], [-1.0; 12]).classify(0.0), KoppenClass::EF);
        let mut t = [-2.0; 12]; t[6] = 7.0;
        assert_eq!(normals(t, [20.0; 12]).classify(0.0), KoppenClass::ET);
    }

    #[test]
    fn high_elevation_has_explicit_highland_class() {
        assert_eq!(normals([10.0; 12], [50.0; 12]).classify(3600.0), KoppenClass::Highland);
    }
}
