use extended_isolation_forest::{Forest, ForestOptions};

const DEFAULT_ANOMALY_THRESHOLD: f64 = 0.70;
const DEFAULT_N_TREES: usize = 100;
const DEFAULT_SAMPLE_SIZE: usize = 64;
const DEFAULT_MAX_DEPTH: usize = 8;

pub struct ZAxisAnomalyDetector {
    pub model: Forest<f64, 2>,
    pub anomaly_threshold: f64,
}

impl ZAxisAnomalyDetector {
    pub fn new(training_data: &[[f64; 2]]) -> Result<Self, String> {
        if training_data.is_empty() {
            return Err("Incomplete data: training set is empty".into());
        }

        let options = ForestOptions {
            n_trees: DEFAULT_N_TREES,
            sample_size: training_data.len().min(DEFAULT_SAMPLE_SIZE),
            max_tree_depth: Some(DEFAULT_MAX_DEPTH),
            extension_level: 1,
        };

        let forest = Forest::from_slice(training_data, &options)
            .map_err(|e| format!("Forest initialization failed: {:?}", e))?;

        Ok(Self {
            model: forest,
            anomaly_threshold: DEFAULT_ANOMALY_THRESHOLD,
        })
    }

    pub fn analyze(&self, point: [f64; 2]) -> Option<String> {
        let score = self.model.score(&point);
        if score > self.anomaly_threshold {
            Some(format!("Z-Axis Anomaly (Score: {:.2})", score))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialization_error_on_empty() {
        let result = ZAxisAnomalyDetector::new(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_detection_logic() {
        let mut training_set = Vec::new();
        for i in 0..100 {
            let n = (i % 5) as f64 * 0.01;
            training_set.push([0.1 + n, 9.8 + n]);
        }

        let detector = ZAxisAnomalyDetector::new(&training_set).expect("Failed to train");
        
        assert!(detector.analyze([0.12, 9.81]).is_none());
        assert!(detector.analyze([15.0, 60.0]).is_some());
    }
}
