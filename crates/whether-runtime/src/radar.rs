//! The radar pass: for each location the NEXRAD composite covers, fetch the
//! latest frames and the OpenStreetMap base, decode the tiles and publish one mosaic
//! per frame as `LocationState.radar`. Runs on its own thread every
//! [`RADAR_REFRESH`], so the fetch service never waits for imagery.

use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant, SystemTime};

use whether_core::presentation::WidgetKind;

use whether_display::{RadarFrame, RadarImages, SharedRadar, StateWriter};
use whether_providers::radar::{self, ATTRIBUTION, RadarWindow, TILE};

use crate::transport::Transport;

/// How often the radar is fetched; the composite updates every 5 minutes.
pub const RADAR_REFRESH: Duration = Duration::from_secs(300);
/// How soon to try again when a location had no position yet or its fetch failed.
pub const RADAR_RETRY: Duration = Duration::from_secs(30);
/// How long base-map tiles are kept on disk.
const BASE_MAX_AGE: Duration = Duration::from_hours(7 * 24);

/// Decodes a PNG into RGBA (palette, gray and RGB are expanded).
pub fn decode_png(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|[r, g, b]| [*r, *g, *b, 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|[g, a]| [*g, *g, *g, *a])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    Some((info.width, info.height, rgba))
}

/// Pastes [`TILE`]-sized RGBA tiles into a mosaic of `cols × rows` tiles; missing
/// tiles stay `fill`.
fn mosaic(cols: u32, rows: u32, tiles: &[(u32, u32, Option<Vec<u8>>)], fill: [u8; 4]) -> Vec<u8> {
    let width = (cols * TILE) as usize;
    let mut out: Vec<u8> = fill
        .iter()
        .copied()
        .cycle()
        .take(width * (rows * TILE) as usize * 4)
        .collect();
    let tile = TILE as usize;
    for (col, row, pixels) in tiles {
        let Some(pixels) = pixels.as_ref().filter(|p| p.len() == tile * tile * 4) else {
            continue;
        };
        for y in 0..tile {
            let dst = ((*row as usize * tile + y) * width + *col as usize * tile) * 4;
            out[dst..dst + tile * 4].copy_from_slice(&pixels[y * tile * 4..(y + 1) * tile * 4]);
        }
    }
    out
}

/// GETs `url`; the body of a 200 response.
fn get(transport: &dyn Transport, url: String) -> Option<Vec<u8>> {
    let resp = transport.send(&radar::request(url)).ok()?;
    (resp.status == 200).then_some(resp.body)
}

/// A tile decoded to [`TILE`]×[`TILE`] RGBA.
fn tile(bytes: &[u8]) -> Option<Vec<u8>> {
    decode_png(bytes).and_then(|(w, h, rgba)| (w == TILE && h == TILE).then_some(rgba))
}

/// A base-map tile, from the disk cache while it is younger than a week.
fn base_tile(
    transport: &dyn Transport,
    cache: Option<&Path>,
    w: &RadarWindow,
    x: u32,
    y: u32,
) -> Option<Vec<u8>> {
    let file = cache.map(|dir| dir.join(format!("osm/{}/{x}/{y}.png", w.zoom)));
    if let Some(file) = &file
        && let Ok(meta) = std::fs::metadata(file)
        && meta
            .modified()
            .ok()
            .and_then(|m| SystemTime::now().duration_since(m).ok())
            .is_some_and(|age| age < BASE_MAX_AGE)
        && let Ok(bytes) = std::fs::read(file)
    {
        return tile(&bytes);
    }
    let bytes = get(transport, w.base_url(x, y))?;
    if let Some(file) = &file {
        let stored = file
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(file, &bytes));
        if let Err(e) = stored {
            tracing::debug!("radar base tile not cached: {e}");
        }
    }
    tile(&bytes)
}

/// Fetches the radar loop around a location. `None` outside the composite or when
/// its metadata cannot be read; missing tiles are left blank.
pub fn fetch(
    transport: &dyn Transport,
    cache: Option<&Path>,
    latitude: f64,
    longitude: f64,
    zoom: u8,
) -> Option<RadarImages> {
    if !radar::covers(latitude, longitude) {
        return None;
    }
    let body = get(transport, radar::meta_request().url)?;
    let latest = radar::parse_valid(&body)
        .map_err(|e| tracing::debug!("radar metadata: {e}"))
        .ok()?;
    let w = RadarWindow::new(latitude, longitude, zoom);
    let (width, height) = w.size();
    let cells = w.tiles();
    let base: Vec<_> = cells
        .iter()
        .map(|&(col, row, x, y)| (col, row, base_tile(transport, cache, &w, x, y)))
        .collect();
    let frames = radar::frames(latest)
        .into_iter()
        .map(|(ago, valid)| {
            let tiles: Vec<_> = cells
                .iter()
                .map(|&(col, row, x, y)| {
                    (
                        col,
                        row,
                        get(transport, w.frame_url(ago, x, y)).and_then(|b| tile(&b)),
                    )
                })
                .collect();
            RadarFrame {
                valid,
                rgba: mosaic(radar::WINDOW, radar::WINDOW, &tiles, [0, 0, 0, 0]).into(),
            }
        })
        .collect();
    Some(RadarImages {
        width,
        height,
        base: mosaic(radar::WINDOW, radar::WINDOW, &base, [0xE5, 0xE3, 0xDF, 255]).into(),
        frames,
        marker: w.marker,
        attribution: ATTRIBUTION,
    })
}

/// One pass over every location: fetch and publish its radar. Returns whether
/// every location is done: covered ones have fresh imagery, the rest are outside
/// the composite. A location still being geocoded, or a failed fetch, is not.
pub fn pass(
    writer: &StateWriter,
    transport: &dyn Transport,
    cache: Option<&Path>,
    zoom: u8,
) -> bool {
    let state = writer.current();
    let mut done = true;
    for (i, location) in state.locations.iter().enumerate() {
        let configured = location.config.latitude.zip(location.config.longitude);
        let Some((latitude, longitude)) = location
            .resolved
            .as_ref()
            .map(|r| (r.latitude, r.longitude))
            .or(configured)
        else {
            done = false;
            continue;
        };
        if !radar::covers(latitude, longitude) {
            continue;
        }
        let Some(images) = fetch(transport, cache, latitude, longitude, zoom) else {
            done = false;
            continue;
        };
        let name = location.name.clone();
        let images = SharedRadar(Arc::new(images));
        writer.update(|s| {
            // The locations may have changed meanwhile (a reload).
            if let Some(l) = s.locations.get_mut(i).filter(|l| l.name == name) {
                l.radar = Some(images);
            }
        });
    }
    done
}

/// How often the radar thread checks whether radar is wanted, at which zoom, and
/// for which locations; a change starts a pass at once (settings, reloads, added
/// places).
const RADAR_TICK: Duration = Duration::from_secs(2);

/// Runs passes every [`RADAR_REFRESH`] ([`RADAR_RETRY`] after an incomplete one)
/// while a presentation shows the radar, until `stop` disconnects. `wanted` and
/// `zoom` apply until the service publishes its configuration; then its
/// `[radar] zoom` and presentations decide.
pub fn run(
    writer: &StateWriter,
    transport: &dyn Transport,
    cache: Option<&Path>,
    (wanted, zoom): (bool, u8),
    stop: &Receiver<()>,
) {
    let mut next = Instant::now();
    let mut seen = None;
    loop {
        let state = writer.current();
        let (wanted, zoom) = state.config.as_ref().map_or((wanted, zoom), |c| {
            (c.0.shows_anywhere(WidgetKind::Radar), c.0.radar.zoom)
        });
        let names: Vec<String> = state.locations.iter().map(|l| l.name.clone()).collect();
        let key = (wanted, zoom, names);
        if seen.as_ref() != Some(&key) {
            seen = Some(key);
            next = Instant::now();
        }
        if !wanted {
            if state.locations.iter().any(|l| l.radar.is_some()) {
                writer.update(|s| s.locations.iter_mut().for_each(|l| l.radar = None));
            }
        } else if Instant::now() >= next {
            let done = pass(writer, transport, cache, zoom);
            next = Instant::now() + if done { RADAR_REFRESH } else { RADAR_RETRY };
        }
        match stop.recv_timeout(RADAR_TICK) {
            Err(RecvTimeoutError::Timeout) => {}
            Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use whether_display::{AppState, LocationState, StatePublisher};
    use whether_providers::HttpRequest;

    use crate::transport::{OwnedResponse, TransportError};

    /// A PNG of one RGBA color, `TILE` square.
    fn png_tile(rgba: [u8; 4]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, TILE, TILE);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut writer = enc.write_header().unwrap();
            let data: Vec<u8> = rgba
                .iter()
                .copied()
                .cycle()
                .take((TILE * TILE * 4) as usize)
                .collect();
            writer.write_image_data(&data).unwrap();
        }
        out
    }

    /// Answers the metadata, gray base tiles, and red radar tiles except for the
    /// top-left one; records the URLs.
    struct Fake {
        seen: Mutex<Vec<String>>,
    }

    impl Transport for Fake {
        fn send(&self, req: &HttpRequest) -> Result<OwnedResponse, TransportError> {
            self.seen.lock().unwrap().push(req.url.clone());
            let url = &req.url;
            if url.ends_with("n0q_0.json") {
                return Ok(OwnedResponse::new(
                    200,
                    br#"{"meta": {"valid": "2026-10-10T20:15:00Z"}}"#.to_vec(),
                ));
            }
            if url.contains("openstreetmap") {
                return Ok(OwnedResponse::new(200, png_tile([200, 200, 200, 255])));
            }
            if url.ends_with("/8/74/95.png") {
                return Ok(OwnedResponse::new(404, Vec::new()));
            }
            Ok(OwnedResponse::new(200, png_tile([255, 0, 0, 128])))
        }
    }

    #[test]
    fn a_loop_is_six_mosaics_over_a_cached_base() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake {
            seen: Mutex::new(Vec::new()),
        };
        let images = fetch(&fake, Some(dir.path()), 40.7143, -74.0060, 8).unwrap();
        assert_eq!((images.width, images.height), (768, 768));
        assert_eq!(images.frames.len(), 6);
        assert_eq!(
            images.frames[5].valid.0 - images.frames[0].valid.0,
            25 * 60,
            "oldest first"
        );
        let px = |rgba: &[u8], x: usize, y: usize| {
            rgba[(y * 768 + x) * 4..(y * 768 + x) * 4 + 4].to_vec()
        };
        assert_eq!(px(&images.base, 400, 400), [200, 200, 200, 255]);
        let frame = &images.frames[0].rgba;
        assert_eq!(px(frame, 400, 400), [255, 0, 0, 128], "radar echo");
        assert_eq!(
            px(frame, 10, 10),
            [0, 0, 0, 0],
            "a missing tile stays clear"
        );
        let seen = fake.seen.lock().unwrap().clone();
        assert_eq!(
            seen.iter().filter(|u| u.contains("openstreetmap")).count(),
            9
        );
        assert_eq!(seen.iter().filter(|u| u.contains("nexrad-n0q")).count(), 54);

        // The base map comes from the disk cache the next time.
        let again = Fake {
            seen: Mutex::new(Vec::new()),
        };
        fetch(&again, Some(dir.path()), 40.7143, -74.0060, 8).unwrap();
        assert!(
            again
                .seen
                .lock()
                .unwrap()
                .iter()
                .all(|u| !u.contains("openstreetmap"))
        );
    }

    #[test]
    fn locations_outside_the_composite_get_nothing() {
        let fake = Fake {
            seen: Mutex::new(Vec::new()),
        };
        assert!(fetch(&fake, None, 40.614, -1.659, 8).is_none());
        assert!(fake.seen.lock().unwrap().is_empty(), "no requests");
    }

    #[test]
    fn a_pass_publishes_per_location() {
        let mut location = LocationState::new(whether_config::LocationConfig {
            name: Some("New York".into()),
            ..Default::default()
        });
        location.resolved = Some(whether_core::model::ResolvedLocation {
            name: "New York".into(),
            latitude: 40.7143,
            longitude: -74.0060,
            ..Default::default()
        });
        let (publisher, handle) = StatePublisher::new(AppState {
            locations: vec![location],
            ..AppState::default()
        });
        let fake = Fake {
            seen: Mutex::new(Vec::new()),
        };
        assert!(pass(&publisher.writer(), &fake, None, 8), "complete");
        let state = handle.current();
        let radar = state.locations[0].radar.as_ref().unwrap();
        assert_eq!(radar.0.frames.len(), 6);
        assert_eq!(radar.0.attribution, ATTRIBUTION);
    }

    #[test]
    fn passes_use_configured_coordinates_and_retry_unknown_places() {
        let configured = LocationState::new(whether_config::LocationConfig {
            name: Some("New York".into()),
            latitude: Some(40.7143),
            longitude: Some(-74.0060),
            ..Default::default()
        });
        let unresolved = LocationState::new(whether_config::LocationConfig {
            name: Some("Brooklyn".into()),
            ..Default::default()
        });
        let (publisher, handle) = StatePublisher::new(AppState {
            locations: vec![configured, unresolved],
            ..AppState::default()
        });
        let fake = Fake {
            seen: Mutex::new(Vec::new()),
        };
        assert!(
            !pass(&publisher.writer(), &fake, None, 8),
            "Brooklyn has no position yet"
        );
        assert!(handle.current().locations[0].radar.is_some());
    }

    #[test]
    fn the_thread_idles_and_clears_imagery_while_radar_is_off() {
        let mut location = LocationState::new(whether_config::LocationConfig {
            name: Some("New York".into()),
            latitude: Some(40.7143),
            longitude: Some(-74.0060),
            ..Default::default()
        });
        location.radar = Some(SharedRadar(Arc::new(RadarImages {
            width: 1,
            height: 1,
            base: Arc::from(vec![0; 4]),
            frames: Vec::new(),
            marker: (0, 0),
            attribution: ATTRIBUTION,
        })));
        let (publisher, handle) = StatePublisher::new(AppState {
            locations: vec![location],
            ..AppState::default()
        });
        let writer = publisher.writer();
        let fake = &Fake {
            seen: Mutex::new(Vec::new()),
        };
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
        std::thread::scope(|scope| {
            scope.spawn(move || run(&writer, fake, None, (false, 8), &stop_rx));
            let start = Instant::now();
            while handle.current().locations[0].radar.is_some() {
                assert!(start.elapsed() < Duration::from_secs(5), "cleared");
                std::thread::sleep(Duration::from_millis(10));
            }
            drop(stop_tx);
        });
        assert!(fake.seen.lock().unwrap().is_empty(), "nothing fetched");
    }

    #[test]
    fn palette_and_gray_pngs_decode_to_rgba() {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, 2, 1);
            enc.set_color(png::ColorType::Indexed);
            enc.set_depth(png::BitDepth::Eight);
            enc.set_palette(vec![10, 20, 30, 40, 50, 60]);
            let mut w = enc.write_header().unwrap();
            w.write_image_data(&[1, 0]).unwrap();
        }
        assert_eq!(
            decode_png(&out),
            Some((2, 1, vec![40, 50, 60, 255, 10, 20, 30, 255]))
        );
        assert_eq!(decode_png(b"not a png"), None);
    }
}
