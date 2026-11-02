//! Choosing Open-Meteo's weather models: which of the configured models
//! a request for a place carries, and how a multi-model response is merged.
//!
//! With `models=a,b,c` every hourly, daily and 15-minute variable comes once per
//! model, suffixed (`temperature_2m_ncep_hrrr_conus`), and is `null` where that
//! model has no data. `current` is not suffixed. When Open-Meteo drops a model it
//! has no data for, the remaining ones may come back unsuffixed.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use serde_json::{Map, Value};
pub use whether_core::models::{BEST_MATCH, CATALOGUE, ModelInfo, Region, lookup};

use crate::regions::in_region;
use crate::url::Coordinates;

/// The most models one request may carry.
pub const MAX_MODELS: usize = 4;

/// Whether model `id` may have data at `loc`. Models the catalogue lacks are
/// assumed to be global.
fn covers(id: &str, loc: &Coordinates) -> bool {
    lookup(id).is_none_or(|m| in_region(m.region, loc))
}

fn is_regional(id: &str) -> bool {
    lookup(id).is_some_and(|m| m.region != Region::Global)
}

/// The catalogue entries that may have data at `loc`.
pub fn covering(loc: &Coordinates) -> impl Iterator<Item = &'static ModelInfo> + '_ {
    CATALOGUE.iter().filter(|m| in_region(m.region, loc))
}

/// The models a request for `loc` carries, in priority order: the configured ones
/// that cover `loc` (at most [`MAX_MODELS`]). A list of two or more that would end
/// with a regional model gets [`BEST_MATCH`] last, so days past a short model's
/// range and places at a rotated grid's ragged edge still get data. Empty means
/// Open-Meteo's default.
pub fn for_location(configured: &[String], loc: &Coordinates) -> Vec<String> {
    let mut models: Vec<String> = configured
        .iter()
        .filter(|id| covers(id, loc))
        .take(MAX_MODELS)
        .cloned()
        .collect();
    if configured.len() >= 2
        && models.last().is_some_and(|id| is_regional(id))
        && !models.iter().any(|id| id == BEST_MATCH)
    {
        models.push(String::from(BEST_MATCH));
    }
    models
}

/// The models that may appear as suffixes in a response to `configured`: the list
/// in order, then [`BEST_MATCH`].
pub(crate) fn candidates(configured: &[String]) -> Vec<&str> {
    let mut out: Vec<&str> = configured.iter().map(String::as_str).collect();
    if !out.contains(&BEST_MATCH) {
        out.push(BEST_MATCH);
    }
    out
}

/// The blocks whose variables carry model suffixes.
const SUFFIXED_BLOCKS: [&str; 3] = ["hourly", "daily", "minutely_15"];

/// Splits `key` into its variable and the index of its model in `order`; an
/// unsuffixed key gets `order.len()`, after every model.
fn split<'k>(key: &'k str, order: &[&str]) -> (&'k str, usize) {
    let mut best: Option<(&'k str, usize, usize)> = None;
    for (i, model) in order.iter().enumerate() {
        let Some(base) = key.strip_suffix(model).and_then(|k| k.strip_suffix('_')) else {
            continue;
        };
        // The longest suffix wins (`ecmwf_ifs025` before `ecmwf_ifs`).
        if best.is_none_or(|(_, _, len)| model.len() > len) {
            best = Some((base, i, model.len()));
        }
    }
    best.map_or((key, order.len()), |(base, i, _)| (base, i))
}

/// Whether `body` has any variable suffixed with one of `order`'s models.
pub(crate) fn is_multi_model(body: &Value, order: &[&str]) -> bool {
    SUFFIXED_BLOCKS.iter().any(|block| {
        body.get(*block)
            .and_then(Value::as_object)
            .is_some_and(|b| b.keys().any(|k| split(k, order).1 < order.len()))
    })
}

/// One variable's columns, by model index (ascending).
type Columns = BTreeMap<usize, Vec<Value>>;

fn group(block: Map<String, Value>, order: &[&str]) -> (Option<Value>, BTreeMap<String, Columns>) {
    let mut time = None;
    let mut vars: BTreeMap<String, Columns> = BTreeMap::new();
    for (key, value) in block {
        if key == "time" {
            time = Some(value);
            continue;
        }
        let Value::Array(column) = value else {
            continue;
        };
        let (base, model) = split(&key, order);
        vars.entry(String::from(base))
            .or_default()
            .insert(model, column);
    }
    (time, vars)
}

/// The first non-null value at `i`, trying `first` and then every model in order.
fn pick(columns: &Columns, i: usize, first: Option<usize>, used: &mut [bool]) -> Value {
    let preferred = first.and_then(|m| columns.get(&m).map(|c| (m, c)));
    for (model, column) in preferred
        .into_iter()
        .chain(columns.iter().map(|(m, c)| (*m, c)))
    {
        if let Some(v) = column.get(i).filter(|v| !v.is_null()) {
            if let Some(u) = used.get_mut(model) {
                *u = true;
            }
            return v.clone();
        }
    }
    Value::Null
}

/// The model each day's values come from: the first with that day's high (or
/// low), so one day never mixes two models' extremes.
fn day_models(vars: &BTreeMap<String, Columns>, n: usize) -> Vec<Option<usize>> {
    let lead = vars
        .get("temperature_2m_max")
        .or_else(|| vars.get("temperature_2m_min"));
    (0..n)
        .map(|i| {
            lead.and_then(|columns| {
                columns
                    .iter()
                    .find(|(_, c)| c.get(i).is_some_and(|v| !v.is_null()))
                    .map(|(m, _)| *m)
            })
        })
        .collect()
}

fn merge_block(block: Map<String, Value>, order: &[&str], daily: bool, used: &mut [bool]) -> Value {
    let (time, vars) = group(block, order);
    let n = time.as_ref().and_then(Value::as_array).map_or(0, Vec::len);
    let days = if daily {
        day_models(&vars, n)
    } else {
        vec![None; n]
    };
    let mut out = Map::new();
    if let Some(time) = time {
        out.insert(String::from("time"), time);
    }
    for (name, columns) in &vars {
        let merged = (0..n).map(|i| pick(columns, i, days[i], used)).collect();
        out.insert(name.clone(), Value::Array(merged));
    }
    Value::Object(out)
}

/// Rewrites a multi-model response so every variable appears once, unsuffixed,
/// taking each value from the first model in `order` that has it (days whole from
/// one model). Returns the models that supplied values, in order.
pub(crate) fn merge(body: &mut Value, order: &[&str]) -> Vec<String> {
    let mut used = vec![false; order.len() + 1];
    if let Some(root) = body.as_object_mut() {
        for block in SUFFIXED_BLOCKS {
            if let Some(Value::Object(b)) = root.remove(block) {
                let merged = merge_block(b, order, block == "daily", &mut used);
                root.insert(String::from(block), merged);
            }
        }
    }
    order
        .iter()
        .zip(&used)
        .filter(|(_, u)| **u)
        .map(|(m, _)| String::from(*m))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use serde_json::json;

    fn at(latitude: f64, longitude: f64) -> Coordinates {
        Coordinates {
            latitude,
            longitude,
        }
    }

    fn list(ids: &[&str]) -> Vec<String> {
        ids.iter().map(ToString::to_string).collect()
    }

    const NEW_YORK: (f64, f64) = (40.7143, -74.0060);
    const OSLO: (f64, f64) = (59.91, 10.75);

    #[test]
    fn regional_models_are_dropped_outside_their_region() {
        let us = list(&["ncep_hrrr_conus", "ncep_nbm_conus", "ecmwf_ifs025"]);
        let (lat, lon) = NEW_YORK;
        assert_eq!(for_location(&us, &at(lat, lon)), us);
        let (lat, lon) = OSLO;
        assert_eq!(for_location(&us, &at(lat, lon)), ["ecmwf_ifs025"]);
        assert_eq!(
            for_location(&list(&["ncep_hrrr_conus", "ncep_nbm_conus"]), &at(lat, lon)),
            Vec::<String>::new()
        );
        // A single model is sent as configured, or not at all.
        assert_eq!(
            for_location(&list(&["ncep_nbm_conus"]), &at(lat, lon)),
            Vec::<String>::new()
        );
        let (lat, lon) = NEW_YORK;
        assert_eq!(
            for_location(&list(&["ncep_nbm_conus"]), &at(lat, lon)),
            ["ncep_nbm_conus"]
        );
    }

    #[test]
    fn regional_lists_end_with_best_match() {
        let (lat, lon) = NEW_YORK;
        let loc = at(lat, lon);
        assert_eq!(
            for_location(&list(&["ncep_hrrr_conus", "ncep_nbm_conus"]), &loc),
            ["ncep_hrrr_conus", "ncep_nbm_conus", BEST_MATCH]
        );
        // Edmonton: HRDPS stays, HRRR goes, best match follows.
        assert_eq!(
            for_location(
                &list(&["ncep_hrrr_conus", "gem_hrdps_continental"]),
                &at(53.55, -113.49)
            ),
            ["gem_hrdps_continental", BEST_MATCH]
        );
        assert_eq!(
            for_location(&list(&["ncep_hrrr_conus", "gfs_seamless"]), &loc),
            ["ncep_hrrr_conus", "gfs_seamless"]
        );
        // Unknown models count as global and are kept.
        assert_eq!(
            for_location(
                &list(&["ncep_hrrr_conus", "knmi_seamless"]),
                &at(OSLO.0, OSLO.1)
            ),
            ["knmi_seamless"]
        );
    }

    #[test]
    fn at_most_four_models_are_sent() {
        let many = list(&[
            "gfs_seamless",
            "ecmwf_ifs025",
            "icon_seamless",
            "gem_seamless",
            "jma_seamless",
        ]);
        assert_eq!(for_location(&many, &at(OSLO.0, OSLO.1)).len(), MAX_MODELS);
    }

    #[test]
    fn suffixes_split_on_the_longest_model() {
        let order = ["ecmwf_ifs", "ecmwf_ifs025"];
        assert_eq!(
            split("temperature_2m_ecmwf_ifs025", &order),
            ("temperature_2m", 1)
        );
        assert_eq!(
            split("temperature_2m_ecmwf_ifs", &order),
            ("temperature_2m", 0)
        );
        assert_eq!(split("temperature_2m", &order), ("temperature_2m", 2));
    }

    #[test]
    fn values_come_from_the_first_model_that_has_them() {
        let order = ["a", "b"];
        let mut body = json!({
            "hourly": {
                "time": [0, 3600, 7200],
                "temperature_2m_a": [1.0, null, null],
                "temperature_2m_b": [10.0, 11.0, null],
                "uv_index_a": [null, null, null],
                "uv_index_b": [2.0, 3.0, 4.0],
                "precipitation": [7.0, 7.0, 7.0]
            }
        });
        assert!(is_multi_model(&body, &order));
        let used = merge(&mut body, &order);
        assert_eq!(used, ["a", "b"]);
        let h = &body["hourly"];
        assert_eq!(h["temperature_2m"], json!([1.0, 11.0, null]));
        assert_eq!(h["uv_index"], json!([2.0, 3.0, 4.0]));
        // Unsuffixed columns are the last resort.
        assert_eq!(h["precipitation"], json!([7.0, 7.0, 7.0]));
        assert_eq!(h["time"], json!([0, 3600, 7200]));
    }

    #[test]
    fn a_day_comes_whole_from_one_model() {
        let order = ["hrrr", "nbm"];
        let mut body = json!({
            "daily": {
                "time": [0, 86400],
                "temperature_2m_max_hrrr": [16.5, null],
                "temperature_2m_min_hrrr": [9.0, 8.0],
                "precipitation_sum_hrrr": [null, 1.0],
                "temperature_2m_max_nbm": [14.8, 14.2],
                "temperature_2m_min_nbm": [7.0, 6.0],
                "precipitation_sum_nbm": [0.5, 2.0]
            }
        });
        merge(&mut body, &order);
        let d = &body["daily"];
        assert_eq!(d["temperature_2m_max"], json!([16.5, 14.2]));
        // Day 2 has no HRRR high, so its low and rain are NBM's too.
        assert_eq!(d["temperature_2m_min"], json!([9.0, 6.0]));
        assert_eq!(d["precipitation_sum"], json!([0.5, 2.0]));
    }

    #[test]
    fn single_model_responses_are_left_alone() {
        let body =
            json!({"hourly": {"time": [0], "temperature_2m": [1.0]}, "current": {"time": 0}});
        assert!(!is_multi_model(&body, &["ncep_nbm_conus", BEST_MATCH]));
    }
}
