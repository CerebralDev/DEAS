//! Read-only discovery for UI preselection, not an installation readiness check.
use crate::install::torch_profile::{self, Hardware};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareDetection {
    pub backend: Option<DetectedBackend>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hardware: Option<Hardware>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DetectedBackend {
    Nvidia,
}

impl HardwareDetection {
    fn from_probe(result: Result<Hardware, String>) -> Self {
        match result {
            Ok(hardware) => Self {
                backend: Some(DetectedBackend::Nvidia),
                hardware: Some(hardware),
                warnings: vec![],
            },
            Err(warning) => Self {
                backend: None,
                hardware: None,
                warnings: vec![warning],
            },
        }
    }
}

async fn detect_with<F>(probe: F, timeout: std::time::Duration) -> HardwareDetection
where
    F: FnOnce() -> Result<Hardware, String> + Send + 'static,
{
    let task = tauri::async_runtime::spawn_blocking(probe);
    let result = match tokio::time::timeout(timeout, task).await {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => Err(format!("NVIDIA_GPU: discovery worker failed: {error}")),
        Err(_) => Err("NVIDIA_GPU: hardware discovery timed out".into()),
    };
    HardwareDetection::from_probe(result)
}

/// No arguments, target paths, plan state, downloads or service changes.
/// Uses the existing five-second, 16 KiB NVIDIA query only. Detection does not
/// imply compatible wheels or verified GPU computation. A failed probe does not
/// imply CPU, AMD or Intel. The worker retains its own process cleanup deadline
/// even if the outer six-second response deadline expires.
#[tauri::command]
pub async fn native_detect_hardware() -> HardwareDetection {
    detect_with(torch_profile::probe, std::time::Duration::from_secs(6)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn async_detection_timeout_returns_no_backend() {
        let start = std::time::Instant::now();
        let result = detect_with(
            || {
                std::thread::sleep(std::time::Duration::from_millis(100));
                torch_profile::parse("GPU-fixture, Fixture, 580.1, 12.0")
            },
            std::time::Duration::from_millis(10),
        )
        .await;
        assert!(result.backend.is_none());
        assert!(result.hardware.is_none());
        assert!(result.warnings[0].contains("timed out"));
        assert!(start.elapsed() < std::time::Duration::from_millis(90));
    }

    #[tokio::test]
    async fn failed_discovery_never_assumes_another_backend() {
        for output in [
            "",
            "GPU-a, A, 580.1, 12.0\nGPU-b, B, 580.1, 12.0",
            "GPU-a, A, N/A, 12.0",
            "not a hardware response",
        ] {
            let result = detect_with(
                move || torch_profile::parse(output),
                std::time::Duration::from_secs(1),
            )
            .await;
            let value = serde_json::to_value(result).unwrap();
            assert!(value["backend"].is_null());
            assert!(value.get("hardware").is_none());
            assert_eq!(value["warnings"].as_array().unwrap().len(), 1);
        }
        let failure = HardwareDetection::from_probe(Err("probe unavailable".into()));
        assert_eq!(
            serde_json::to_value(failure).unwrap(),
            serde_json::json!({
                "backend": null, "warnings": ["probe unavailable"]
            })
        );
    }

    #[tokio::test]
    async fn worker_failure_returns_warning_not_a_detected_backend() {
        let result = detect_with(
            || panic!("fixture worker failure"),
            std::time::Duration::from_secs(1),
        )
        .await;
        assert!(result.backend.is_none());
        assert!(result.hardware.is_none());
        assert!(result.warnings[0].contains("worker failed"));
    }

    #[test]
    fn command_has_exact_local_permission_and_registration() {
        let permission = include_str!("../permissions/hardware.toml");
        assert!(permission.contains("identifier = \"allow-native-detect-hardware\""));
        assert!(permission.contains(
            "commands.allow = [\"native_detect_hardware\", \"native_monitoring_sample\"]"
        ));
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/main.json")).unwrap();
        assert_eq!(capability["local"], true);
        assert_eq!(capability["windows"], serde_json::json!(["main"]));
        assert!(capability.get("remote").is_none());
        assert!(capability["permissions"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("allow-native-detect-hardware")));
        assert!(include_str!("main.rs").contains("deas::native_detect_hardware,"));
        assert!(include_str!("main.rs").contains("deas::native_monitoring_sample,"));
    }

    #[tokio::test]
    #[ignore = "Read-only live discovery; no installation or GPU computation"]
    async fn live_read_only_hardware_detection() {
        let start = std::time::Instant::now();
        let result = native_detect_hardware().await;
        let value = serde_json::to_value(result).unwrap();
        if value["backend"] == "nvidia" {
            assert_eq!(value["hardware"]["vendor"], "NVIDIA");
            assert!(value["hardware"]["uuid"]
                .as_str()
                .unwrap()
                .starts_with("GPU-"));
        } else {
            assert!(value["backend"].is_null());
            assert!(value.get("hardware").is_none());
        }
        println!("{value}");
        println!("elapsed_ms={}", start.elapsed().as_millis());
        assert!(start.elapsed() < std::time::Duration::from_secs(7));
    }

    #[test]
    fn detected_nvidia_preserves_exact_existing_metadata() {
        // Discovery must not apply install-time driver/Torch gates.
        let hardware = torch_profile::parse("GPU-fixture, GTX 1080, 520.1.0, 6.1").unwrap();
        let result = HardwareDetection::from_probe(Ok(hardware));
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            serde_json::json!({
                "backend": "nvidia", "hardware": {
                    "vendor": "NVIDIA", "name": "GTX 1080", "uuid": "GPU-fixture",
                    "driverVersion": "520.1.0", "computeCapability": "6.1"
                }
            })
        );
    }
}
