//! Applying a `UiModel` to the compact window (desktop compact mode and ESP32-S3).

use alloc::vec::Vec;

use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel};

use crate::model::UiModel;
use crate::{CompactWindow, DetailUi, Theme};

/// `DetailKind` indices shown on the details page, in order: wind, humidity, UV,
/// pressure.
const METRIC_KINDS: [i32; 4] = [0, 2, 5, 4];

/// The details page's four tiles; a missing detail becomes an empty tile.
pub fn metrics(m: &UiModel) -> Vec<DetailUi> {
    METRIC_KINDS
        .iter()
        .map(|kind| {
            m.details
                .iter()
                .find(|d| d.kind == *kind)
                .cloned()
                .unwrap_or(DetailUi {
                    kind: *kind,
                    title: SharedString::new(),
                    value: SharedString::from("—"),
                    caption: SharedString::new(),
                    level: -1,
                    gauge: -1.0,
                    direction: -1.0,
                    icon: -1,
                })
        })
        .collect()
}

fn model<T: Clone + 'static>(items: &[T]) -> ModelRc<T> {
    ModelRc::new(VecModel::from(items.to_vec()))
}

/// Sets the window from `m` on the sky (`top`, `bottom`, `text`).
pub fn apply_compact(ui: &CompactWindow, m: &UiModel, (top, bottom, text): (Color, Color, Color)) {
    let theme = ui.global::<Theme>();
    theme.set_text(text);
    theme.set_text_muted(text.with_alpha(0.72));
    ui.set_sky_top(top);
    ui.set_sky_bottom(bottom);
    ui.set_has_data(m.has_data);
    ui.set_current(m.current.clone());
    ui.set_hours(model(&m.hours));
    ui.set_days(model(&m.days));
    ui.set_metrics(model(&metrics(m)));
    ui.set_status(m.status.clone());
    ui.set_alerts(model(&m.alerts));
    ui.set_attribution(m.attribution.clone());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_are_wind_humidity_uv_pressure() {
        let m = crate::model::to_ui(&crate::model::tests::sample_vm());
        let kinds: Vec<i32> = metrics(&m).iter().map(|d| d.kind).collect();
        assert_eq!(kinds, METRIC_KINDS);
        let empty = metrics(&UiModel::default());
        assert!(empty.iter().all(|d| d.value == "—"));
    }
}
