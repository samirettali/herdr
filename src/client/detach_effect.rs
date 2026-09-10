//! The animation the client plays over its last frame while detaching.
//!
//! Everything here is client presentation: the frames are derived from the
//! frame already on screen and written through the same path as any other
//! frame, so the server never knows the effect exists.

use std::time::Duration;

use crate::config::DetachEffectConfig;
use crate::protocol::{CellData, FrameData};

use super::state::ClientState;

/// How far apart frames are drawn; the configured duration sets how many.
/// 120 frames a second, the refresh rate of the displays this runs on. A
/// terminal that cannot keep up drops frames rather than slowing down.
const FRAME_INTERVAL: Duration = Duration::from_micros(8_333);
/// Fewer frames than this cannot read as an animation, and more than this
/// would hold the detach for a minute: the configured duration is clamped.
const MIN_FRAMES: usize = 6;
const MAX_FRAMES: usize = 2000;
const BOLD: u16 = 0b1;
/// Rows a stream falls before its head stops being the character it grew
/// out of and becomes code like the rest.
const HOLD_ROWS: f32 = 2.0;
/// Streams per column at most, beyond the one every column's topmost
/// character always gets so that nothing above it is left standing.
const EXTRA_STREAMS: usize = 3;
/// Fraction of the effect after which a background nothing has fallen over
/// has faded to black, so painted surfaces do not outlive the rain.
const BACKGROUND_FADE: f32 = 0.55;

/// Play the configured effect, blocking for `duration`. Called right before
/// the detach message, so nothing else runs on the client meanwhile.
pub(super) fn play(state: &mut ClientState, effect: DetachEffectConfig, duration: Duration) {
    let Some(frame) = state.blit_encoder.last_frame().cloned() else {
        return;
    };
    let frames = match effect {
        DetachEffectConfig::None => return,
        DetachEffectConfig::Matrix => {
            matrix_frames(&frame, frame_count(duration), seed_from_clock())
        }
    };
    // Pace against fixed deadlines rather than sleeping after each frame, so
    // the time spent encoding and writing a frame does not stretch the
    // interval. A frame whose deadline has already passed is dropped, except
    // the last one, so the effect always ends on time and on a black screen.
    let started = std::time::Instant::now();
    let last = frames.len().saturating_sub(1);
    for (index, frame) in frames.into_iter().enumerate() {
        let deadline = started + FRAME_INTERVAL * index as u32;
        let now = std::time::Instant::now();
        if now > deadline + FRAME_INTERVAL && index != last {
            continue;
        }
        if let Some(wait) = deadline.checked_duration_since(now) {
            std::thread::sleep(wait);
        }
        state.present_frame(frame);
    }
}

fn frame_count(duration: Duration) -> usize {
    let frames = duration.as_micros() / FRAME_INTERVAL.as_micros();
    usize::try_from(frames)
        .unwrap_or(MAX_FRAMES)
        .clamp(MIN_FRAMES, MAX_FRAMES)
}

fn seed_from_clock() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0x9e37_79b9, |elapsed| elapsed.as_nanos() as u64)
}

/// A small deterministic generator, so the frames are testable and the
/// binary needs no random crate for a second of decoration.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            (self.next() % bound as u64) as usize
        }
    }

    /// Uniform in `[0, 1)`.
    fn unit(&mut self) -> f32 {
        (self.next() % 10_000) as f32 / 10_000.0
    }
}

fn glyph(rng: &mut Lcg) -> String {
    // Half-width katakana are one cell wide, which keeps the grid intact.
    const KATAKANA: (u32, u32) = (0xFF71, 0xFF9D);
    match rng.below(4) {
        0 => char::from(b'0' + rng.below(10) as u8).to_string(),
        _ => char::from_u32(KATAKANA.0 + rng.below((KATAKANA.1 - KATAKANA.0 + 1) as usize) as u32)
            .map_or_else(|| "0".to_string(), |ch| ch.to_string()),
    }
}

fn packed_rgb(r: u8, g: u8, b: u8) -> u32 {
    0x02_00_00_00 | (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
}

/// Green at `intensity` in `[0, 1]`, curved so the tail stays visible for
/// longer before it drops into black.
fn rain_green(intensity: f32) -> u32 {
    let lit = intensity.clamp(0.0, 1.0).powf(0.6);
    packed_rgb(0, (30.0 + 225.0 * lit) as u8, (8.0 + 62.0 * lit) as u8)
}

/// The packed background `bg` faded towards black by `progress` in `[0, 1]`.
/// RGB backgrounds dim smoothly; palette ones, which cannot be scaled, go
/// out at once part way through.
fn fade_background(bg: u32, progress: f32) -> u32 {
    let remaining = 1.0 - (progress / BACKGROUND_FADE).min(1.0);
    if remaining <= 0.0 {
        0
    } else if bg >> 24 == 0x02 {
        let scale = |channel: u32| ((channel & 0xFF) as f32 * remaining) as u8;
        packed_rgb(scale(bg >> 16), scale(bg >> 8), scale(bg))
    } else if remaining < 0.5 {
        0
    } else {
        bg
    }
}

fn blank_cell() -> CellData {
    CellData {
        symbol: " ".to_string(),
        fg: 0,
        bg: 0,
        modifier: 0,
        skip: false,
        hyperlink: None,
    }
}

fn is_blank(cell: &CellData) -> bool {
    cell.symbol.trim().is_empty()
}

struct Grid {
    width: usize,
    height: usize,
}

impl Grid {
    fn of(frame: &FrameData, frames: usize) -> Option<Self> {
        let width = usize::from(frame.width);
        let height = usize::from(frame.height);
        (width > 0 && height > 0 && frames > 0 && frame.cells.len() == width * height)
            .then_some(Self { width, height })
    }

    fn frame(&self, template: &FrameData, cells: Vec<CellData>) -> FrameData {
        FrameData {
            cells,
            width: template.width,
            height: template.height,
            cursor: None,
            hyperlinks: Vec::new(),
            graphics: Vec::new(),
        }
    }
}

/// One stream of rain: the character it grows out of, when it starts, how
/// fast it falls and how long its tail is, all different from its neighbours.
struct Stream {
    column: usize,
    /// The row of the character the stream starts from. The head begins
    /// there and the tail never reaches above it.
    origin: usize,
    /// That character, shown as the head for its first rows before the head
    /// turns into code.
    symbol: String,
    start: usize,
    rows_per_frame: f32,
    tail: usize,
}

/// The frames of the effect: film-style digital rain grown out of the text
/// on screen. Every cell owns a glyph that only occasionally mutates, so the
/// columns read as standing code rather than noise. Each stream starts from
/// a character that is on screen: that character is the head for its first
/// rows, lit white, then the head turns into code and drags a tail that
/// never reaches above where the character was. The topmost character of
/// every column always starts a stream, so nothing is left standing above
/// the rain; backgrounds nothing falls over fade to black on their own; and
/// no stream starts late enough to still be on screen at the last frame.
pub(super) fn matrix_frames(frame: &FrameData, frames: usize, seed: u64) -> Vec<FrameData> {
    let Some(grid) = Grid::of(frame, frames) else {
        return Vec::new();
    };
    let mut rng = Lcg(seed);
    let mut glyphs = (0..frame.cells.len())
        .map(|_| glyph(&mut rng))
        .collect::<Vec<_>>();
    let spawn_window = (frames * 2 / 3).max(1);
    let mut streams = Vec::with_capacity(grid.width * (EXTRA_STREAMS + 1));
    for column in 0..grid.width {
        let text_rows = (0..grid.height)
            .filter(|y| !is_blank(&frame.cells[y * grid.width + column]))
            .collect::<Vec<_>>();
        let Some(&topmost) = text_rows.first() else {
            continue;
        };
        let mut origins = vec![topmost];
        for _ in 0..EXTRA_STREAMS.min(text_rows.len() - 1) {
            let candidate = text_rows[1 + rng.below(text_rows.len() - 1)];
            if !origins.contains(&candidate) {
                origins.push(candidate);
            }
        }
        for origin in origins {
            let start = rng.below(spawn_window);
            let tail = 6 + rng.below(14);
            let travel = (grid.height - origin + tail) as f32;
            let frames_left = (frames - 1).saturating_sub(start).max(1) as f32;
            let escape = travel / frames_left;
            let wanted = 0.5 + rng.unit() * 1.0;
            streams.push(Stream {
                column,
                origin,
                symbol: frame.cells[origin * grid.width + column].symbol.clone(),
                start,
                rows_per_frame: wanted.max(escape),
                tail,
            });
        }
    }

    (0..frames)
        .map(|tick| {
            // A few glyphs flip every frame: the code is alive, not static.
            for _ in 0..(glyphs.len() / 30).max(1) {
                let index = rng.below(glyphs.len());
                glyphs[index] = glyph(&mut rng);
            }
            let progress = tick as f32 / (frames - 1).max(1) as f32;
            // Per cell: the strongest tail covering it, the head that sits
            // on it, and whether any stream has passed and wiped the text.
            let mut intensity = vec![0.0f32; frame.cells.len()];
            let mut head: Vec<Option<&Stream>> = vec![None; frame.cells.len()];
            let mut wiped = vec![false; frame.cells.len()];
            for stream in &streams {
                if tick < stream.start {
                    continue;
                }
                let fallen = (tick - stream.start) as f32 * stream.rows_per_frame;
                let head_row = stream.origin as f32 + fallen;
                let head_cell = head_row as usize;
                for y in stream.origin..grid.height {
                    let index = y * grid.width + stream.column;
                    if y > head_cell {
                        break;
                    }
                    wiped[index] = true;
                    let distance = head_row - y as f32;
                    if distance < stream.tail as f32 {
                        let strength = 1.0 - distance / stream.tail as f32;
                        if strength > intensity[index] {
                            intensity[index] = strength;
                        }
                        if y == head_cell {
                            head[index] = Some(stream);
                        }
                    }
                }
            }
            let cells = frame
                .cells
                .iter()
                .enumerate()
                .map(|(index, original)| {
                    if let Some(stream) = head[index] {
                        // The character itself leads for its first rows,
                        // then the head is code like the rest.
                        let fallen = (tick - stream.start) as f32 * stream.rows_per_frame;
                        let symbol = if fallen < HOLD_ROWS {
                            stream.symbol.clone()
                        } else {
                            glyphs[index].clone()
                        };
                        CellData {
                            symbol,
                            fg: packed_rgb(220, 255, 220),
                            bg: 0,
                            modifier: BOLD,
                            skip: false,
                            hyperlink: None,
                        }
                    } else if intensity[index] > 0.0 {
                        CellData {
                            symbol: glyphs[index].clone(),
                            fg: rain_green(intensity[index]),
                            bg: 0,
                            modifier: 0,
                            skip: false,
                            hyperlink: None,
                        }
                    } else if wiped[index] {
                        blank_cell()
                    } else {
                        let mut cell = original.clone();
                        cell.bg = fade_background(cell.bg, progress);
                        cell
                    }
                })
                .collect();
            grid.frame(frame, cells)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(rows: &[&str]) -> FrameData {
        let width = rows[0].chars().count() as u16;
        let cells = rows
            .iter()
            .flat_map(|row| row.chars())
            .map(|ch| CellData {
                symbol: ch.to_string(),
                fg: 7,
                bg: 0,
                modifier: 0,
                skip: false,
                hyperlink: None,
            })
            .collect();
        FrameData {
            cells,
            width,
            height: rows.len() as u16,
            cursor: None,
            hyperlinks: Vec::new(),
            graphics: Vec::new(),
        }
    }

    fn text(frame: &FrameData) -> String {
        frame
            .cells
            .chunks(usize::from(frame.width))
            .map(|row| {
                row.iter()
                    .map(|cell| cell.symbol.as_str())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn matrix_ends_on_an_empty_screen_and_keeps_the_grid() {
        let source = frame(&["hello world", "  second   ", "third line "]);
        let frames = matrix_frames(&source, 18, 7);

        assert_eq!(frames.len(), 18);
        for frame in &frames {
            assert_eq!(frame.cells.len(), source.cells.len());
            assert_eq!((frame.width, frame.height), (source.width, source.height));
            assert!(frame.cursor.is_none());
        }
        let last = text(frames.last().expect("frames"));
        assert!(last.trim().is_empty(), "last frame: {last:?}");
    }

    #[test]
    fn matrix_keeps_text_under_the_rain_until_a_stream_passes() {
        let source = frame(&["abcd", "efgh", "ijkl", "mnop", "qrst"]);
        let frames = matrix_frames(&source, 40, 9);
        assert_eq!(
            text(&frames[0]),
            text(&source),
            "at the first frame every stream is still its own character"
        );
        let untouched = |frame: &FrameData| {
            frame
                .cells
                .iter()
                .zip(&source.cells)
                .filter(|(cell, original)| cell == original)
                .count()
        };
        assert!(
            untouched(&frames[0]) > 0,
            "text shows under the first drops"
        );
        assert!(
            untouched(&frames[0]) >= untouched(&frames[20]),
            "the rain only ever takes text away"
        );
        assert_eq!(untouched(&frames[39]), 0);
    }

    #[test]
    fn matrix_fades_painted_backgrounds_nothing_falls_over() {
        let mut source = frame(&["    ", "    ", "ab  ", "    "]);
        let painted = packed_rgb(40, 40, 40);
        for cell in &mut source.cells[..8] {
            cell.bg = painted;
        }
        source.cells[12].bg = 7;
        let frames = matrix_frames(&source, 40, 5);
        assert_eq!(
            frames[0].cells[0].bg, painted,
            "the first frame is the screen as it was"
        );
        assert!(
            (0..8).all(|index| frames[39].cells[index].bg == 0),
            "painted rows above the text go black by the end"
        );
        assert_eq!(frames[39].cells[12].bg, 0, "palette backgrounds go out too");
        let mid = frames[10].cells[0].bg & 0xFF;
        assert!(
            mid > 0 && mid < 40,
            "a quarter in, the background is dimmer but still there"
        );
    }

    #[test]
    fn matrix_never_lights_a_cell_above_the_text_it_grew_from() {
        let source = frame(&["    ", "    ", "    ", "ab  ", "    "]);
        for frame in matrix_frames(&source, 24, 5) {
            for index in 0..12 {
                assert!(
                    is_blank(&frame.cells[index]),
                    "cell {index} above the text lit up: {:?}",
                    text(&frame)
                );
            }
            assert!(is_blank(&frame.cells[14]) && is_blank(&frame.cells[15]));
        }
    }

    #[test]
    fn matrix_is_deterministic_for_a_seed_and_empty_for_a_bad_frame() {
        let source = frame(&["abc", "def"]);
        assert_eq!(
            matrix_frames(&source, 18, 11),
            matrix_frames(&source, 18, 11)
        );
        assert_ne!(
            text(&matrix_frames(&source, 18, 11)[4]),
            text(&matrix_frames(&source, 18, 12)[4])
        );

        let mut broken = source.clone();
        broken.cells.pop();
        assert!(matrix_frames(&broken, 18, 1).is_empty());
        assert!(matrix_frames(&source, 0, 1).is_empty());
    }

    #[test]
    fn a_full_screen_of_frames_is_generated_well_within_its_duration() {
        let row = "x".repeat(200);
        let rows = vec![row.as_str(); 60];
        let source = frame(&rows);
        let started = std::time::Instant::now();
        let frames = matrix_frames(&source, 180, 3);
        let elapsed = started.elapsed();
        assert_eq!(frames.len(), 180);
        // 180 frames play in 1.5 s; even an unoptimised build must generate
        // them in a fraction of that or the first frame would visibly lag.
        assert!(
            elapsed < Duration::from_millis(1500),
            "generating 180 frames took {elapsed:?}"
        );
    }

    #[test]
    fn duration_sets_the_frame_count_within_bounds() {
        assert_eq!(frame_count(Duration::from_millis(500)), 60);
        assert_eq!(frame_count(Duration::from_millis(2000)), 240);
        assert_eq!(frame_count(Duration::ZERO), MIN_FRAMES);
        assert_eq!(frame_count(Duration::from_secs(3600)), MAX_FRAMES);
    }
}
