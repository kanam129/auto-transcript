use super::{looks_like_loopback, SourceInfo, SourceKind};
use crate::error::{AppError, Result};
use cpal::traits::{DeviceTrait, HostTrait};

/// cpal's `DeviceId` has a text form that is stable across reboots, so that is what settings
/// store — not the device name, which changes if someone renames their hardware.
fn device_id_string(device: &cpal::Device) -> Option<String> {
    device.id().ok().map(|id| id.to_string())
}

fn device_name(device: &cpal::Device, fallback: &str) -> String {
    device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| fallback.to_string())
}

fn supports_input(device: &cpal::Device) -> bool {
    device
        .description()
        .map(|d| d.supports_input())
        .unwrap_or(false)
}

pub fn list_sources() -> Result<Vec<SourceInfo>> {
    let host = cpal::default_host();
    let default_in = host
        .default_input_device()
        .and_then(|d| device_id_string(&d))
        .unwrap_or_default();
    let default_out = host
        .default_output_device()
        .and_then(|d| device_id_string(&d))
        .unwrap_or_default();

    let mut out = Vec::new();

    // --- input devices: microphones, and virtual loopback drivers such as BlackHole ---
    let inputs = host
        .input_devices()
        .map_err(|e| AppError::Audio(format!("could not read input device list: {e}")))?;
    for device in inputs {
        let Some(id) = device_id_string(&device) else {
            continue;
        };
        let name = device_name(&device, &id);
        let Ok(cfg) = device.default_input_config() else {
            tracing::debug!("device '{name}' has no input config, skipping");
            continue;
        };
        out.push(SourceInfo {
            kind: if looks_like_loopback(&name) {
                SourceKind::VirtualLoopback
            } else {
                SourceKind::Mic
            },
            is_default: id == default_in,
            sample_rate: cfg.sample_rate(),
            channels: cfg.channels(),
            id,
            name,
        });
    }

    // --- output devices, tapped directly ---
    //
    // Only devices with NO input side are listed here. If a device has both (a USB headset,
    // say) cpal takes the ordinary input path when asked to record — so presenting it as a
    // system source would be misleading, and it already appears in the list above.
    let outputs = host
        .output_devices()
        .map_err(|e| AppError::Audio(format!("could not read output device list: {e}")))?;
    for device in outputs {
        if supports_input(&device) {
            continue;
        }
        let Some(id) = device_id_string(&device) else {
            continue;
        };
        let name = device_name(&device, &id);
        let Ok(cfg) = device.default_output_config() else {
            continue;
        };
        out.push(SourceInfo {
            kind: SourceKind::SystemOutput,
            is_default: id == default_out,
            sample_rate: cfg.sample_rate(),
            channels: cfg.channels(),
            id,
            name,
        });
    }

    // System sources first, with the driver-free ones at the very top.
    out.sort_by_key(|s| {
        let rank = match s.kind {
            SourceKind::SystemOutput => 0,
            SourceKind::VirtualLoopback => 1,
            SourceKind::Mic => 2,
        };
        (rank, !s.is_default, s.name.to_lowercase())
    });
    Ok(out)
}

pub fn find_device(id: &str) -> Result<cpal::Device> {
    let host = cpal::default_host();
    for list in [host.input_devices(), host.output_devices()] {
        let Ok(devices) = list else { continue };
        for device in devices {
            if device_id_string(&device).as_deref() == Some(id) {
                return Ok(device);
            }
        }
    }
    Err(AppError::NotFound(format!("audio device '{id}'")))
}

pub fn device_label(id: &str) -> String {
    find_device(id)
        .ok()
        .map(|d| device_name(&d, id))
        .unwrap_or_else(|| id.to_string())
}

pub fn source_kind(id: &str) -> Option<SourceKind> {
    list_sources().ok()?.into_iter().find(|s| s.id == id).map(|s| s.kind)
}

/// First-run guesses, before any settings exist.
///
/// For system audio, tapping an output device directly is preferred: it needs no driver, it
/// does not change the user's output device, and volume keys keep working. A virtual driver
/// is only used when direct capture is unavailable.
pub fn guess_defaults(sources: &[SourceInfo]) -> (Option<String>, Option<String>) {
    let system = sources
        .iter()
        .find(|s| s.kind == SourceKind::SystemOutput && s.is_default)
        .or_else(|| sources.iter().find(|s| s.kind == SourceKind::SystemOutput))
        .or_else(|| sources.iter().find(|s| s.kind == SourceKind::VirtualLoopback))
        .map(|s| s.id.clone());
    let mic = sources
        .iter()
        .find(|s| s.kind == SourceKind::Mic && s.is_default)
        .or_else(|| sources.iter().find(|s| s.kind == SourceKind::Mic))
        .map(|s| s.id.clone());
    (system, mic)
}
