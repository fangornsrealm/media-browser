//! User-facing configuration for similarity scans, persisted with the rest
//! of the app config.

use std::path::PathBuf;

use cosmic::{
    cosmic_config::{self, cosmic_config_derive::CosmicConfigEntry, CosmicConfigEntry},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, CosmicConfigEntry, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct SimilarConfig {
    /// Image sensitivity: 0 (most strict) .. 100 (most permissive).
    /// 100 maps to duplo's default image threshold (-60).
    pub image_sensitivity: u8,
    /// Video sensitivity: 0 (most strict) .. 100 (most permissive).
    /// 100 maps to duplo's default video threshold (-20).
    pub video_sensitivity: u8,
    /// Seconds between video screenshots.
    pub video_screenshot_interval: u32,
    /// Minimum consecutive matching screenshots for a video match.
    pub video_min_sequence: u32,
    /// Number of concurrent ffmpeg screenshot-extraction workers.
    pub video_num_threads: u32,
    /// Max candidates kept in the video comparison cache (~100 MB each).
    pub video_max_cache: usize,
    /// Base directory under which `similar_images/` and `similar_videos/`
    /// are created. Defaults to the scanned directory.
    pub output_dir: Option<PathBuf>,
    /// Recurse into subdirectories when scanning.
    pub recursive: bool,
}

impl Default for SimilarConfig {
    fn default() -> Self {
        Self {
            image_sensitivity: 50,
            video_sensitivity: 50,
            video_screenshot_interval: 10,
            video_min_sequence: 6,
            video_num_threads: 1,
            video_max_cache: 100,
            output_dir: None,
            recursive: false,
        }
    }
}

impl SimilarConfig {
    /// duplo-rs image threshold for the configured sensitivity.
    ///
    /// duplo accepts a match when its score is *below* the threshold, so a
    /// more negative value is more permissive. The mapping keeps the demo
    /// app's tunable range (threshold -60 … -160) and exposes it with
    /// 0 = most strict, 100 = most permissive (duplo's default -60).
    pub fn image_threshold(&self) -> f64 {
        -160.0 + self.image_sensitivity as f64
    }

    /// duplo-rs video threshold for the configured sensitivity
    /// (demo range -20 … -120).
    pub fn video_threshold(&self) -> f64 {
        -120.0 + self.video_sensitivity as f64
    }

    /// Number of concurrent ffmpeg workers, clamped to a sane range.
    pub fn video_num_threads_clamped(&self) -> u32 {
        self.video_num_threads.clamp(1, 16)
    }

    /// Screenshot interval in seconds, clamped to a sane range.
    pub fn video_screenshot_interval_clamped(&self) -> u32 {
        self.video_screenshot_interval.clamp(1, 3600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_plan() {
        let c = SimilarConfig::default();
        assert_eq!(c.image_sensitivity, 50);
        assert_eq!(c.video_sensitivity, 50);
        assert_eq!(c.video_screenshot_interval, 10);
        assert_eq!(c.video_min_sequence, 6);
        assert_eq!(c.video_num_threads, 1);
        assert_eq!(c.video_max_cache, 100);
        assert_eq!(c.output_dir, None);
        assert!(!c.recursive);
    }

    #[test]
    fn thresholds_are_monotonic_and_reach_demo_defaults() {
        let strict = SimilarConfig {
            image_sensitivity: 0,
            video_sensitivity: 0,
            ..Default::default()
        };
        let mid = SimilarConfig::default();
        let loose = SimilarConfig {
            image_sensitivity: 100,
            video_sensitivity: 100,
            ..Default::default()
        };
        assert!(strict.image_threshold() < mid.image_threshold());
        assert!(mid.image_threshold() < loose.image_threshold());
        assert!(strict.video_threshold() < loose.video_threshold());
        // Sensitivity 100 maps to duplo-rs's demo default thresholds.
        assert_eq!(loose.image_threshold(), -60.0);
        assert_eq!(loose.video_threshold(), -20.0);
        assert_eq!(strict.image_threshold(), -160.0);
        assert_eq!(strict.video_threshold(), -120.0);
    }

    #[test]
    fn worker_and_interval_clamps() {
        let c = SimilarConfig {
            video_num_threads: 0,
            video_screenshot_interval: 0,
            ..Default::default()
        };
        assert_eq!(c.video_num_threads_clamped(), 1);
        assert_eq!(c.video_screenshot_interval_clamped(), 1);

        let c = SimilarConfig {
            video_num_threads: 99,
            video_screenshot_interval: 9999,
            ..Default::default()
        };
        assert_eq!(c.video_num_threads_clamped(), 16);
        assert_eq!(c.video_screenshot_interval_clamped(), 3600);
    }

    #[test]
    fn serde_round_trip_and_partial_defaults() {
        let c = SimilarConfig {
            recursive: true,
            ..Default::default()
        };
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(
            serde_json::from_str::<SimilarConfig>(&json).unwrap(),
            c
        );
        // serde(default) keeps old/partial config files loadable.
        assert_eq!(
            serde_json::from_str::<SimilarConfig>("{}").unwrap(),
            SimilarConfig::default()
        );
    }
}
