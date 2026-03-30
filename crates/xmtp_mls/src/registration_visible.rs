/// Specifies how many nodes must confirm visibility before returning Ok(()).
#[derive(Debug, Clone)]
pub enum Quorum {
    /// Fraction of nodes that must confirm: required = ceil(p * node_count).
    Percentage(f32),
    /// Exact number of nodes that must confirm.
    Absolute(usize),
}

impl Quorum {
    pub fn required_count(&self, total: usize) -> usize {
        match self {
            Quorum::Percentage(p) => ((total as f32) * p).ceil() as usize,
            Quorum::Absolute(n) => *n,
        }
    }
}

#[derive(Debug, Clone)]
pub struct VisibilityConfirmationOptions {
    pub quorum: Quorum,
    pub timeout_ms: u64,
    pub sleep_interval_ms: u64,
}

impl Default for VisibilityConfirmationOptions {
    fn default() -> Self {
        Self {
            quorum: Quorum::Percentage(0.5),
            timeout_ms: 30_000,
            sleep_interval_ms: 500,
        }
    }
}

#[cfg(test)]
mod quorum_tests {
    use super::*;

    #[test]
    fn quorum_percentage_ceiling() {
        let q = Quorum::Percentage(0.5);
        assert_eq!(q.required_count(4), 2);
        assert_eq!(q.required_count(5), 3); // ceil(0.5 * 5) = 3
        assert_eq!(q.required_count(1), 1);
        assert_eq!(q.required_count(0), 0);
    }

    #[test]
    fn quorum_absolute() {
        let q = Quorum::Absolute(3);
        assert_eq!(q.required_count(10), 3);
        assert_eq!(q.required_count(2), 3);
    }

    #[test]
    fn visibility_confirmation_options_defaults() {
        let opts = VisibilityConfirmationOptions::default();
        assert!(matches!(opts.quorum, Quorum::Percentage(p) if (p - 0.5).abs() < f32::EPSILON));
        assert_eq!(opts.timeout_ms, 30_000);
        assert_eq!(opts.sleep_interval_ms, 500);
    }
}
