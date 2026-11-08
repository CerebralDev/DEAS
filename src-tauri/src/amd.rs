//! AMD ROCm provisioning baseline, ported from the DaSiWa ComfyUI installer
//! (internal/native/torch/amd.go, amd_runtime.go). The gfx target map and its
//! failure semantics are copied deliberately: unknown families must never
//! silently receive RDNA3 kernels.
use crate::install::torch_profile::Hardware;

const AMD_ALLOWED_TARGETS: [&str; 7] = [
    "gfx1030", "gfx1100", "gfx1101", "gfx1102", "gfx1151", "gfx1200", "gfx1201",
];
pub const AMD_INDEX_URL: &str = "https://stable.repo.amd.com/rocm/whl-next/";

// Explicit desktop/APU mappings; keep in sync with the installer's amdModelTargets.
const AMD_MODEL_TARGETS: [(&str, &str); 18] = [
    ("RX6800", "gfx1030"),
    ("RX6800XT", "gfx1030"),
    ("RX6900XT", "gfx1030"),
    ("RX6950XT", "gfx1030"),
    ("W6800", "gfx1030"),
    ("RX7600", "gfx1102"),
    ("RX7600XT", "gfx1102"),
    ("RX7700XT", "gfx1101"),
    ("RX7800XT", "gfx1101"),
    ("RX7900XT", "gfx1100"),
    ("RX7900XTX", "gfx1100"),
    ("W7800", "gfx1100"),
    ("W7900", "gfx1100"),
    ("RX9060", "gfx1200"),
    ("RX9060XT", "gfx1200"),
    ("RX9070", "gfx1201"),
    ("RX9070XT", "gfx1201"),
    ("R9700", "gfx1201"),
];
// APU model tokens (8040S/8050S/8060S) match as exact tokens, gfx1151.
const AMD_APU_TARGETS: [(&str, &str); 3] = [
    ("8040S", "gfx1151"),
    ("8050S", "gfx1151"),
    ("8060S", "gfx1151"),
];

fn scan_tokens(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            cur.push(ch.to_ascii_uppercase());
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// (digits, trailing letters) for a token starting with ASCII digits.
fn split_digits(token: &str) -> Option<(&str, &str)> {
    let index = token
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(token.len());
    (index > 0).then(|| (&token[..index], &token[index..]))
}

/// Port of the installer's AMDTarget: model map plus optional explicit gfx
/// tokens; conflicting models or name/target mismatches fail closed.
pub fn amd_target(name: &str) -> Result<String, String> {
    let tokens = scan_tokens(name);
    let mut target = String::new();
    let mut i = 0;
    while i < tokens.len() {
        let token = tokens[i].as_str();
        if token == "MOBILE" || token == "LAPTOP" {
            return Err(format!(
                "AMD mobile GPU {name:?} is not in the DEAS support list"
            ));
        }
        if let Some(rest) = token.strip_prefix("GFX") {
            if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_hexdigit()) {
                let gfx = token.to_ascii_lowercase();
                if !AMD_ALLOWED_TARGETS.contains(&gfx.as_str()) {
                    return Err(format!(
                        "AMD GPU target {gfx} is not in the DEAS support list"
                    ));
                }
                if !target.is_empty() && target != gfx {
                    return Err(format!(
                        "AMD GPU name/target mismatch in {name:?}: {target} versus {gfx}"
                    ));
                }
                target = gfx;
                i += 1;
                continue;
            }
        }
        let mut model: Option<(String, usize)> = None;
        if token == "RX" {
            if let Some(next) = tokens.get(i + 1).map(String::as_str) {
                if let Some((digits, suffix)) = split_digits(next) {
                    match suffix {
                        "M" | "S" => {
                            return Err(format!(
                                "AMD mobile GPU {name:?} is not in the DEAS support list"
                            ))
                        }
                        "XT" | "XTX" | "" => {
                            let mut key = format!("RX{digits}{suffix}");
                            let mut consumed = 2;
                            if suffix.is_empty()
                                && tokens
                                    .get(i + 2)
                                    .is_some_and(|t2| t2 == "XT" || t2 == "XTX")
                            {
                                key.push_str(&tokens[i + 2]);
                                consumed = 3;
                            }
                            model = Some((key, consumed));
                        }
                        _ => {}
                    }
                }
            }
        } else if let Some(rest) = token.strip_prefix("RX") {
            if let Some((digits, suffix)) = split_digits(rest) {
                match suffix {
                    "M" | "S" => {
                        return Err(format!(
                            "AMD mobile GPU {name:?} is not in the DEAS support list"
                        ))
                    }
                    "XT" | "XTX" | "" => model = Some((format!("RX{digits}{suffix}"), 1)),
                    _ => {}
                }
            }
        }
        let model = model.or_else(|| {
            let mapped = |key: &str| Some((key.to_owned(), 1));
            if let Some(rest) = token.strip_prefix('W') {
                if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()) {
                    return mapped(token);
                }
            }
            if let Some(rest) = token.strip_prefix('R') {
                if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()) {
                    return mapped(token);
                }
            }
            if AMD_APU_TARGETS.iter().any(|(model, _)| *model == token) {
                return mapped(token);
            }
            None
        });
        if let Some((key, consumed)) = model {
            let mapped = AMD_MODEL_TARGETS
                .iter()
                .chain(AMD_APU_TARGETS.iter())
                .find(|(model, _)| *model == key.as_str())
                .map(|(_, target)| (*target).to_owned())
                .ok_or_else(|| format!("AMD GPU model {key} is not in the DEAS support list"))?;
            if !target.is_empty() && target != mapped {
                return Err(format!("conflicting AMD GPU models in {name:?}"));
            }
            target = mapped;
            i += consumed;
            continue;
        }
        i += 1;
    }
    if target.is_empty() {
        return Err(format!(
            "AMD GPU {name:?} is not recognized; select a supported exact model or gfx target ({})",
            AMD_ALLOWED_TARGETS.join(", ")
        ));
    }
    Ok(target)
}

/// Pure parser for `lspci -nn` output; `driver_version` is injected so tests
/// need no kernel state. shortcut: assumes one display device; multi-GPU
/// selection is an open PLAN.md item and the NVIDIA probe refuses it too.
#[cfg(any(target_os = "linux", test))]
pub fn parse_amd_lspci(output: &str, driver_version: &str) -> Result<Hardware, String> {
    let mut found: Option<(String, String)> = None;
    for line in output.lines() {
        let is_display = ["VGA compatible", "Display controller", "3D controller"]
            .iter()
            .any(|class| line.contains(class));
        if !is_display {
            continue;
        }
        if !["Advanced Micro Devices", "ATI Technologies", "[AMD/ATI]"]
            .iter()
            .any(|vendor| line.contains(vendor))
        {
            return Err("AMD_GPU: a non-AMD display device is visible; DEAS supports single-vendor GPU hosts — choose CPU or the other vendor's backend".into());
        }
        if found.is_some() {
            return Err("AMD_GPU: expected exactly one AMD display device; multi-GPU selection is not ported. Select CPU explicitly or use a single-GPU host".into());
        }
        let (_, after) = line
            .split_once("]: ")
            .ok_or("AMD_GPU: unparseable lspci line; repair pciutils and re-plan")?;
        let devid = bracket_pci_id(after).ok_or(
            "AMD_GPU: lspci line carries no [vendor:device] id; repair pciutils and re-plan",
        )?;
        let description = after
            .rsplit_once(" [")
            .map_or_else(|| after.trim(), |(description, _)| description.trim())
            .to_owned();
        found = Some((description, format!("AMD-{devid}")));
    }
    let (description, uuid) = found
        .ok_or("AMD_GPU: no AMD display device found by lspci; select CPU or another backend")?;
    let compute_capability = amd_target(&description)?;
    Ok(Hardware {
        vendor: "AMD".into(),
        uuid,
        name: description,
        driver_version: driver_version.into(),
        compute_capability,
    })
}

/// Last `[hhhh:hhhh]` bracket in an lspci description tail.
#[cfg(any(target_os = "linux", test))]
fn bracket_pci_id(tail: &str) -> Option<&str> {
    let (_, rest) = tail.rsplit_once("[")?;
    let id = rest.split_once(']')?.0;
    let (vendor, device) = id.split_once(':')?;
    let hex4 = |part: &str| part.len() == 4 && part.bytes().all(|b| b.is_ascii_hexdigit());
    (hex4(vendor) && hex4(device)).then_some(id)
}

#[cfg(target_os = "linux")]
pub fn amdgpu_driver_version() -> Result<String, String> {
    std::fs::read_to_string("/sys/module/amdgpu/version")
        .map(|version| version.trim().to_owned())
        .map_err(|_| "AMD_DRIVER: the amdgpu kernel module is not loaded; install the ROCm driver and re-plan".into())
}

/// shortcut: Linux only for now — AMD's ROCm wheels resolve for Windows in a
/// `uv --dry-run` (2026-11-08) but there is no Windows AMD runtime acceptance
/// yet; plan() keeps Backend::Amd Linux-gated until one exists.
#[cfg(target_os = "linux")]
pub fn probe_amd() -> Result<Hardware, String> {
    let driver_version = amdgpu_driver_version()?;
    let output = std::process::Command::new("lspci")
        .arg("-nn")
        .output()
        .map_err(|_| "AMD_GPU: lspci is unavailable; install pciutils and re-plan".to_string())?;
    if !output.status.success() {
        return Err(format!(
            "AMD_GPU: lspci failed ({})",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    parse_amd_lspci(&String::from_utf8_lossy(&output.stdout), &driver_version)
}

/// Cross-platform compile stub: plan() never requests an AMD backend off Linux,
/// so this only answers honestly if a stale or hand-crafted request arrives.
#[cfg(not(target_os = "linux"))]
pub fn probe_amd() -> Result<Hardware, String> {
    Err("AMD_BACKEND: AMD backends are only offered on Linux".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amd_target_map_matches_installer() {
        for (name, want) in [
            ("Radeon RX 6800 XT", "gfx1030"),
            ("Radeon RX 6900 XT", "gfx1030"),
            ("Radeon RX 6950 XT", "gfx1030"),
            ("Radeon PRO W6800", "gfx1030"),
            ("Radeon RX 7600 XT", "gfx1102"),
            ("Radeon RX 7700 XT", "gfx1101"),
            ("Radeon RX 7800 XT", "gfx1101"),
            ("Radeon RX 7900 XT", "gfx1100"),
            ("Radeon PRO W7900", "gfx1100"),
            ("Radeon PRO W7800", "gfx1100"),
            ("Radeon RX 9060 XT", "gfx1200"),
            ("Radeon RX 9070 XT", "gfx1201"),
            ("Radeon AI PRO R9700", "gfx1201"),
            ("Radeon 8060S", "gfx1151"),
            ("Radeon 8050S", "gfx1151"),
            ("Radeon 8040S", "gfx1151"),
            ("AMD GPU gfx1100", "gfx1100"),
            ("Radeon RX 7900 XTX GFX1100", "gfx1100"),
            ("Radeon RX7900XTX", "gfx1100"),
            ("Radeon RX7800XT", "gfx1101"),
        ] {
            assert_eq!(amd_target(name).as_deref(), Ok(want), "{name}");
        }
    }

    #[test]
    fn amd_rejects_unknown_mobile_or_conflicting_targets() {
        for name in [
            "Manual: AMD",
            "Radeon RX 7000",
            "Radeon RX 9000",
            "Radeon RX 580",
            "Radeon RX 580 gfx1100",
            "Radeon RX 7900 GRE gfx1100",
            "Radeon RX 7900M",
            "Radeon RX 7600M XT",
            "Radeon RX 6800 XT gfx1100",
            "AMD gfx900",
            "AMD gfx1100 gfx1201",
            "Radeon RX 9070 Laptop GPU",
        ] {
            assert!(amd_target(name).is_err(), "{name} must fail closed");
        }
    }

    #[test]
    fn lspci_amd_line_resolves_gfx_and_pci_id() {
        let line = "01:00.0 VGA compatible controller [0300]: Advanced Micro Devices, Inc. [AMD/ATI] Navi 31 [Radeon RX 7900 XT/XTX/REX] [1002:744c] (rev c0)";
        let hardware = parse_amd_lspci(line, "6.16.13").unwrap();
        assert_eq!(hardware.vendor, "AMD");
        assert_eq!(hardware.uuid, "AMD-1002:744c");
        assert_eq!(hardware.compute_capability, "gfx1100");
        assert_eq!(hardware.driver_version, "6.16.13");
    }

    #[test]
    fn lspci_rejects_mixed_vendors_multi_amd_and_absent_amd() {
        let intel = "00:02.0 VGA compatible controller [0300]: Intel Corporation Arrow [8086:a780]";
        let amd = "01:00.0 VGA compatible controller [0300]: Advanced Micro Devices, Inc. [AMD/ATI] Navi 31 [Radeon RX 7900 XT/XTX/REX] [1002:744c] (rev c0)";
        assert!(parse_amd_lspci(&format!("{intel}\n{amd}"), "6.16.13").is_err());
        assert!(parse_amd_lspci(&format!("{amd}\n{amd}"), "6.16.13").is_err());
        assert!(parse_amd_lspci(intel, "6.16.13").is_err());
        assert!(parse_amd_lspci("", "6.16.13").is_err());
    }

    #[test]
    #[ignore = "read-only AMD live discovery; no AMD hardware on this host"]
    fn live_amd_probe() {
        #[cfg(target_os = "linux")]
        {
            let hardware = probe_amd().unwrap();
            println!("{}", serde_json::to_string(&hardware).unwrap());
            assert_eq!(hardware.vendor, "AMD");
            assert!(hardware.compute_capability.starts_with("gfx"));
        }
    }
}
