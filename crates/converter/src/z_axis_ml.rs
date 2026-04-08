use extended_isolation_forest::{Forest, ForestOptions};

pub trait Analyzer {
    fn analyze(&self, data_point: &[f64; 2]) -> Option<&'static str>;
}

pub struct ZAxisAnomalyDetector {
    model: Forest<f64, 2>,
    anomaly_threshold: f64,
}

impl ZAxisAnomalyDetector {
    pub fn new(training_data: &[[f64; 2]]) -> Result<Self, &'static str> {
        if training_data.is_empty() {
            return Err("System Error: Cannot train on empty flight data.");
        }

        let options = ForestOptions {
            n_trees: 100,
            sample_size: training_data.len().min(256),
            max_tree_depth: None,
            extension_level: 0,
        };

        match Forest::from_slice(training_data, &options) {
            Ok(forest) => Ok(Self {
                model: forest,
                anomaly_threshold: 0.60,
            }),
            Err(_) => Err("System Error: Matrix compilation failed."),
        }
    }
}

impl Analyzer for ZAxisAnomalyDetector {
    fn analyze(&self, data_point: &[f64; 2]) -> Option<&'static str> {
        let score = self.model.score(data_point);
        if score > self.anomaly_threshold {
            Some("CRITICAL: Z-Axis Anomaly")
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_stable_baseline() -> Vec<[f64; 2]> {
        vec![
            [1.0, 1.1], [1.1, 1.0], [1.0, 1.0], [1.2, 1.1],
            [1.0, 1.2], [1.1, 1.1], [1.0, 0.9], [0.9, 1.0],
            [1.1, 1.2], [1.05, 1.05], [0.95, 0.95], [1.15, 1.15]
        ]
    }

    #[test]
    fn test_normal_flight_passes() {
        let detector = ZAxisAnomalyDetector::new(&get_stable_baseline()).unwrap();
        let normal_point = [1.1, 1.1];
        assert_eq!(detector.analyze(&normal_point), None);
    }

    #[test]
    fn test_crash_triggers_anomaly() {
        let detector = ZAxisAnomalyDetector::new(&get_stable_baseline()).unwrap();
        let crash_point = [9.5, 9.8];
        assert_eq!(detector.analyze(&crash_point), Some("CRITICAL: Z-Axis Anomaly"));
    }
}

fn main() {
    println!("Z-Axis ML Module compiled successfully. Run 'cargo test' to execute diagnostic suite...");
}
