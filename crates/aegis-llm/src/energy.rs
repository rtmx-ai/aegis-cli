//! Energy-per-token estimation for local and cloud LLM inference.
//!
//! Provides energy consumption estimates based on accelerator TDP
//! (thermal design power) and observed or estimated throughput.
//! Useful for sustainability reporting and cost-of-compute analysis
//! in air-gapped and cloud deployments.

use std::fmt;

/// Source of the energy measurement or estimate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnergySource {
    /// Measured from device power telemetry (nvidia-smi, powermetrics).
    Measured,
    /// Estimated from published TDP for the accelerator.
    EstimatedTDP,
    /// No power data available.
    Unknown,
}

/// Energy consumption estimate for an inference workload.
#[derive(Debug, Clone, PartialEq)]
pub struct EnergyEstimate {
    /// Average power draw in watts.
    pub watts_avg: f64,
    /// Inference throughput in tokens per second.
    pub tokens_per_sec: f64,
    /// Energy cost per token in joules (watts_avg / tokens_per_sec).
    pub joules_per_token: f64,
    /// How the power figure was obtained.
    pub source: EnergySource,
}

impl EnergyEstimate {
    /// Compute an energy estimate from average wattage and throughput.
    ///
    /// Returns `None` if `tokens_per_sec <= 0.0` or `watts_avg < 0.0`,
    /// guarding against division by zero and nonsensical inputs.
    pub fn compute(
        watts_avg: f64,
        tokens_per_sec: f64,
        source: EnergySource,
    ) -> Option<EnergyEstimate> {
        if watts_avg < 0.0 || tokens_per_sec <= 0.0 {
            return None;
        }
        Some(EnergyEstimate {
            watts_avg,
            tokens_per_sec,
            joules_per_token: watts_avg / tokens_per_sec,
            source,
        })
    }
}

impl fmt::Display for EnergySource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EnergySource::Measured => write!(f, "measured"),
            EnergySource::EstimatedTDP => write!(f, "estimated-tdp"),
            EnergySource::Unknown => write!(f, "unknown"),
        }
    }
}

impl fmt::Display for EnergyEstimate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:.4} J/tok ({:.0}W @ {:.0} tok/s, {})",
            self.joules_per_token, self.watts_avg, self.tokens_per_sec, self.source,
        )
    }
}

/// Look up a cloud accelerator's TDP and return an energy estimate.
///
/// Uses a static table of known accelerator TDPs with a default
/// throughput of 100 tok/s. Returns `None` for unknown accelerators.
pub fn cloud_tdp_estimate(accelerator: &str) -> Option<EnergyEstimate> {
    let tdp_watts = match accelerator {
        "a100" => 300.0,
        "h100" => 450.0,
        "l4" => 72.0,
        "t4" => 70.0,
        "a10g" => 150.0,
        _ => return None,
    };
    let default_throughput = 100.0;
    EnergyEstimate::compute(tdp_watts, default_throughput, EnergySource::EstimatedTDP)
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-LLM-033
    #[test]
    fn test_energy_estimate_from_power_and_throughput() {
        let est = EnergyEstimate::compute(300.0, 100.0, EnergySource::Measured).unwrap();
        assert!(
            (est.joules_per_token - 3.0).abs() < 1e-10,
            "Expected 3.0 J/tok, got {}",
            est.joules_per_token,
        );
        assert!((est.watts_avg - 300.0).abs() < f64::EPSILON);
        assert!((est.tokens_per_sec - 100.0).abs() < f64::EPSILON);
        assert_eq!(est.source, EnergySource::Measured);
    }

    // rtmx:req REQ-LLM-033
    #[test]
    fn test_zero_throughput_returns_none() {
        let result = EnergyEstimate::compute(300.0, 0.0, EnergySource::Measured);
        assert!(result.is_none(), "Zero throughput should return None");
    }

    // rtmx:req REQ-LLM-033
    #[test]
    fn test_negative_watts_returns_none() {
        let result = EnergyEstimate::compute(-1.0, 100.0, EnergySource::Measured);
        assert!(result.is_none(), "Negative watts should return None");
    }

    // rtmx:req REQ-LLM-033
    #[test]
    fn test_cloud_tdp_a100() {
        let est = cloud_tdp_estimate("a100").unwrap();
        assert!((est.watts_avg - 300.0).abs() < f64::EPSILON);
        assert_eq!(est.source, EnergySource::EstimatedTDP);
        assert!(
            (est.joules_per_token - 3.0).abs() < 1e-10,
            "A100 at 100 tok/s should be 3.0 J/tok",
        );
    }

    // rtmx:req REQ-LLM-033
    #[test]
    fn test_cloud_tdp_h100() {
        let est = cloud_tdp_estimate("h100").unwrap();
        assert!((est.watts_avg - 450.0).abs() < f64::EPSILON);
        assert_eq!(est.source, EnergySource::EstimatedTDP);
        assert!(
            (est.joules_per_token - 4.5).abs() < 1e-10,
            "H100 at 100 tok/s should be 4.5 J/tok",
        );
    }

    // rtmx:req REQ-LLM-033
    #[test]
    fn test_cloud_tdp_unknown_returns_none() {
        let result = cloud_tdp_estimate("v100");
        assert!(result.is_none(), "Unknown accelerator should return None");
    }

    // rtmx:req REQ-LLM-033
    #[test]
    fn test_display_format() {
        let est = EnergyEstimate::compute(300.0, 100.0, EnergySource::Measured).unwrap();
        let s = format!("{est}");
        assert_eq!(s, "3.0000 J/tok (300W @ 100 tok/s, measured)");
    }

    // rtmx:req REQ-LLM-033
    #[test]
    fn test_energy_source_display() {
        assert_eq!(format!("{}", EnergySource::Measured), "measured");
        assert_eq!(format!("{}", EnergySource::EstimatedTDP), "estimated-tdp");
        assert_eq!(format!("{}", EnergySource::Unknown), "unknown");
    }
}
