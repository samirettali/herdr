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
        DetachEffectConfig::BlackHole => {
            black_hole_frames(&frame, frame_count(duration), seed_from_clock())
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
    /// Rows fallen so far. Advanced frame by frame with a jitter, so the
    /// streams stutter and surge instead of sliding at one speed.
    fallen: f32,
}

/// Rows behind the head over which the code fades from white to green.
const WHITE_LEAD: f32 = 3.5;

/// Mix of two packed RGB colours, `t` towards the second.
fn mix(a: u32, b: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let channel = |shift: u32| {
        let from = ((a >> shift) & 0xFF) as f32;
        let to = ((b >> shift) & 0xFF) as f32;
        (from + (to - from) * t) as u8
    };
    packed_rgb(channel(16), channel(8), channel(0))
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
                fallen: 0.0,
            });
        }
    }

    (0..frames)
        .map(|tick| {
            // Whatever the jitter left on screen, the effect ends black.
            if tick + 1 == frames {
                return grid.frame(frame, vec![blank_cell(); frame.cells.len()]);
            }
            // A few glyphs flip every frame: the code is alive, not static.
            for _ in 0..(glyphs.len() / 30).max(1) {
                let index = rng.below(glyphs.len());
                glyphs[index] = glyph(&mut rng);
            }
            let progress = tick as f32 / (frames - 1).max(1) as f32;
            // Per cell: the strongest tail covering it, the head that sits
            // on it, and whether any stream has passed and wiped the text.
            let mut intensity = vec![0.0f32; frame.cells.len()];
            // How close behind a head each cell is, for the white lead.
            let mut lead = vec![f32::MAX; frame.cells.len()];
            let mut head: Vec<Option<(String, bool)>> = vec![None; frame.cells.len()];
            let mut wiped = vec![false; frame.cells.len()];
            // The last stretch has every stream surge, so nothing is still
            // on screen when the effect ends.
            let closing = progress > 0.85;
            for stream in &mut streams {
                if tick < stream.start {
                    continue;
                }
                if tick > stream.start {
                    // Uneven motion: a stream stalls now and then and runs
                    // fast at other times, around its own pace.
                    let jitter = if !closing && rng.below(12) == 0 {
                        0.0
                    } else if closing {
                        1.0 + rng.unit() * 0.8
                    } else {
                        0.6 + rng.unit() * 0.9
                    };
                    stream.fallen += stream.rows_per_frame * jitter;
                }
                let head_row = stream.origin as f32 + stream.fallen;
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
                        if distance < lead[index] {
                            lead[index] = distance;
                        }
                        if y == head_cell {
                            // The character itself leads for its first
                            // rows, then the head is code like the rest.
                            let symbol = if stream.fallen < HOLD_ROWS {
                                stream.symbol.clone()
                            } else {
                                glyphs[index].clone()
                            };
                            // Heads flicker: most frames near white, now and
                            // then one dips towards green for a frame.
                            let flash = rng.below(8) == 0;
                            head[index] = Some((symbol, flash));
                        }
                    }
                }
            }
            let cells = frame
                .cells
                .iter()
                .enumerate()
                .map(|(index, original)| {
                    if let Some((symbol, flash)) = &head[index] {
                        let white = 205 + rng.below(51) as u8;
                        let fg = if *flash {
                            packed_rgb(120, 255, 140)
                        } else {
                            packed_rgb(white, 255, white)
                        };
                        CellData {
                            symbol: symbol.clone(),
                            fg,
                            bg: 0,
                            modifier: BOLD,
                            skip: false,
                            hyperlink: None,
                        }
                    } else if intensity[index] > 0.0 {
                        // The first rows behind the head are still nearly
                        // white and fade into the green of the tail.
                        let green = rain_green(intensity[index]);
                        let fg = if lead[index] < WHITE_LEAD {
                            mix(packed_rgb(225, 255, 225), green, lead[index] / WHITE_LEAD)
                        } else {
                            green
                        };
                        CellData {
                            symbol: glyphs[index].clone(),
                            fg,
                            bg: 0,
                            modifier: if lead[index] < 1.5 { BOLD } else { 0 },
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

/// A cell is about twice as tall as it is wide. The black hole works in
/// square units, with rows scaled by this, so orbits are round on screen.
const ROW_ASPECT: f32 = 2.0;
/// Fraction of the effect by which the last particle has crossed the
/// horizon; what is left is the horizon swallowing the screen.
const INFALL_END: f32 = 0.88;
/// Turns a particle at the edge of the screen makes over the whole infall.
/// Kepler's law then makes the inner ones spin far faster.
const OUTER_TURNS: f32 = 0.35;
/// Largest angle a particle may sweep in one frame: past this an orbit
/// aliases into jitter instead of reading as spin.
const MAX_SWEEP: f32 = std::f32::consts::FRAC_PI_3;
/// Previous positions drawn behind a particle, dimmer, as motion blur.
const BLUR_STEPS: usize = 2;
/// Cosine of the disk's inclination to the line of sight: the disk is seen
/// nearly edge-on, squashed to this fraction of its height.
const DISK_FORESHORTENING: f32 = 0.28;
/// Sine of the same inclination, for how much of the orbital speed points
/// at the viewer and Doppler-beams the light.
const DISK_EDGE_ON: f32 = 0.96;
/// The disk's tilt on screen, so it sits askew rather than level.
const DISK_TILT: f32 = -0.32;
/// Fraction of the effect over which gravitational lensing reaches full
/// strength, so the first frame is still the screen as it was.
const LENSING_RAMP: f32 = 0.3;
/// Turns the whole scene makes around the centre over the effect, in the
/// direction of the disk's spin, on top of the orbits within the disk.
const SCENE_TURNS: f32 = 0.3;

/// A character falling into the hole: where it sits in square units, its
/// orbit, and what it looked like before gravity got hold of it.
struct Particle {
    symbol: String,
    fg: u32,
    radius0: f32,
    radius: f32,
    angle: f32,
    /// Frame at which the orbit starts decaying; until then it holds still.
    start: usize,
    /// Direction of rotation, the same for the whole disk bar a few strays.
    spin: f32,
    /// How steeply the radius collapses: near 1 is even, higher lingers
    /// outside and plunges at the end.
    plunge: f32,
    trail: [Option<(f32, f32)>; BLUR_STEPS],
}

/// Colour of a particle at `radius`, in units of its starting radius and of
/// the horizon: the original colour far out, heating to white-yellow and
/// orange through the accretion disk, and redshifting into deep red just
/// above the horizon.
fn accretion_color(original: u32, radius: f32, radius0: f32, horizon: f32) -> u32 {
    let above_horizon = ((radius - horizon) / horizon.max(0.1)).clamp(0.0, 1.0);
    if above_horizon < 1.0 {
        let t = above_horizon;
        return packed_rgb(
            (120.0 + 135.0 * t) as u8,
            (10.0 + 120.0 * t) as u8,
            (10.0 + 30.0 * t) as u8,
        );
    }
    let heat = (1.0 - radius / radius0.max(1.0)).clamp(0.0, 1.0);
    if heat < 0.35 {
        original
    } else if heat < 0.7 {
        packed_rgb(255, 235, 190)
    } else {
        packed_rgb(255, 170, 60)
    }
}

/// Doppler beaming: light from the side of the disk coming towards the
/// viewer is brighter and bluer, the receding side dimmer and redder.
/// `beam` is the line-of-sight fraction of the orbital speed, in `[-1, 1]`.
/// Palette colours cannot be scaled and are left alone.
fn beamed(color: u32, beam: f32) -> u32 {
    if color >> 24 != 0x02 {
        return color;
    }
    let channel = |shift: u32| (color >> shift) & 0xFF;
    let (r, g, b) = (channel(16) as f32, channel(8) as f32, channel(0) as f32);
    let gain = 1.0 + 0.55 * beam;
    let (r, g, b) = if beam < 0.0 {
        (
            r * gain,
            g * gain * (1.0 + 0.4 * beam),
            b * gain * (1.0 + 0.6 * beam),
        )
    } else {
        (r * gain, g * gain, b * gain * (1.0 + 0.5 * beam))
    };
    packed_rgb(r.min(255.0) as u8, g.min(255.0) as u8, b.min(255.0) as u8)
}

/// Where a point of the disk at polar coordinates `(radius, angle)` lands on
/// screen, in square units around the centre: foreshortened by the
/// inclination, tilted, and on the far side bent up over the shadow by
/// gravitational lensing at strength `warp`. The near side has `sin(angle)`
/// positive and hangs below the shadow.
fn project(radius: f32, angle: f32, warp: f32, horizon: f32) -> (f32, f32) {
    let x = radius * angle.cos();
    let mut y = radius * angle.sin() * DISK_FORESHORTENING;
    if angle.sin() < 0.0 {
        // Light from behind the hole reaches us over its top: the closer to
        // the hole, the higher it is lifted, which draws the arch.
        let lift = warp * horizon * 2.2 * (horizon / radius.max(horizon)).powf(0.5);
        y -= lift;
    }
    (x, y)
}

/// A projected point turned by `tilt` around the centre of the screen: the
/// disk's resting slant plus however far the whole scene has spun.
fn tilted(x: f32, y: f32, tilt: f32) -> (f32, f32) {
    (
        x * tilt.cos() - y * tilt.sin(),
        x * tilt.sin() + y * tilt.cos(),
    )
}

/// The inverse of `project` and `tilted` without lensing: the disk point
/// that sits under a screen position at the start, so every character
/// begins in place.
fn unproject(x: f32, y: f32, tilt: f32) -> (f32, f32) {
    let flat_x = x * tilt.cos() + y * tilt.sin();
    let flat_y = -x * tilt.sin() + y * tilt.cos();
    let disk_y = flat_y / DISK_FORESHORTENING;
    (
        (flat_x * flat_x + disk_y * disk_y).sqrt(),
        disk_y.atan2(flat_x),
    )
}

/// The frames of the black hole effect. Every character becomes a particle
/// on a decaying orbit in an accretion disk seen almost edge-on and askew:
/// its radius shrinks on a power curve while its angle advances at the
/// Keplerian rate for that radius, so the disk spins up as it falls in. The
/// far half of the disk is lensed up over the shadow into an arch, with a
/// faint second image under it; the approaching side is Doppler-beamed
/// brighter and the receding side dimmer and redder. Particles heat through
/// the disk, redshift just outside the horizon and vanish across it; the
/// shadow grows as it feeds, ringed by a hot photon ring, and painted
/// backgrounds fade to black on their own. The last frame is black.
pub(super) fn black_hole_frames(frame: &FrameData, frames: usize, seed: u64) -> Vec<FrameData> {
    let Some(grid) = Grid::of(frame, frames) else {
        return Vec::new();
    };
    let mut rng = Lcg(seed);
    let center = (
        grid.width as f32 / 2.0,
        grid.height as f32 * ROW_ASPECT / 2.0,
    );
    let reach = center.0.max(center.1).max(1.0);
    let infall_frames = ((frames as f32 * INFALL_END) as usize).max(2);
    // Kepler: angular speed goes with radius^-3/2. Fix it at the edge of
    // the screen so that an outer particle makes OUTER_TURNS over the
    // infall, and the rest follows.
    let outer_rate = OUTER_TURNS * std::f32::consts::TAU / infall_frames as f32;
    let gravity = outer_rate * outer_rate * reach * reach * reach;
    let disk_spin = if rng.below(2) == 0 { 1.0 } else { -1.0 };
    let mut particles = frame
        .cells
        .iter()
        .enumerate()
        .filter(|(_, cell)| !is_blank(cell))
        .map(|(index, cell)| {
            let x = (index % grid.width) as f32 + 0.5 - center.0;
            let y = ((index / grid.width) as f32 + 0.5) * ROW_ASPECT - center.1;
            let (radius, angle) = unproject(x, y, DISK_TILT);
            let radius = radius.max(0.5);
            Particle {
                symbol: cell.symbol.clone(),
                fg: cell.fg,
                radius0: radius,
                radius,
                angle,
                start: rng.below((infall_frames / 3).max(1)),
                spin: if rng.below(12) == 0 {
                    -disk_spin
                } else {
                    disk_spin
                },
                plunge: 1.3 + rng.unit() * 0.9,
                trail: [None; BLUR_STEPS],
            }
        })
        .collect::<Vec<_>>();

    (0..frames)
        .map(|tick| {
            let progress = tick as f32 / (frames - 1).max(1) as f32;
            let horizon = 1.0 + 3.0 * progress;
            let warp = (progress / LENSING_RAMP).min(1.0);
            // The scene spins up from rest, so the first frames sit still
            // while the orbits are still slow too.
            let tilt =
                DISK_TILT + disk_spin * SCENE_TURNS * std::f32::consts::TAU * progress.powf(1.5);
            let mut cells = frame
                .cells
                .iter()
                .map(|original| {
                    if is_blank(original) {
                        let mut cell = original.clone();
                        cell.bg = fade_background(cell.bg, progress);
                        cell
                    } else {
                        // Its character is a particle now; only the surface
                        // it sat on stays, and fades.
                        let mut cell = blank_cell();
                        cell.bg = fade_background(original.bg, progress);
                        cell
                    }
                })
                .collect::<Vec<_>>();
            let cell_at = |x: f32, y: f32| -> Option<usize> {
                let column = (x + center.0).floor();
                let row = ((y + center.1) / ROW_ASPECT).floor();
                (column >= 0.0
                    && row >= 0.0
                    && column < grid.width as f32
                    && row < grid.height as f32)
                    .then(|| row as usize * grid.width + column as usize)
            };
            // The photon ring: a faint hot circle just outside the horizon,
            // drawn under the particles once the hole has opened.
            if progress > 0.05 && tick + 1 < frames {
                for row in 0..grid.height {
                    for column in 0..grid.width {
                        let x = column as f32 + 0.5 - center.0;
                        let y = (row as f32 + 0.5) * ROW_ASPECT - center.1;
                        let distance = (x * x + y * y).sqrt();
                        if distance >= horizon && distance < horizon + 1.2 {
                            let cell = &mut cells[row * grid.width + column];
                            cell.symbol = "·".to_string();
                            cell.fg = packed_rgb(255, 200, 120);
                            cell.modifier = BOLD;
                        }
                    }
                }
            }
            for particle in &mut particles {
                if tick >= particle.start && particle.radius > horizon {
                    let elapsed = (tick - particle.start) as f32;
                    let span = (infall_frames.saturating_sub(particle.start)).max(1) as f32;
                    let u = (elapsed / span).min(1.0);
                    particle.radius = particle.radius0 * (1.0 - u).powf(particle.plunge);
                    let sweep = (gravity / particle.radius.max(0.5).powi(3)).sqrt();
                    particle.angle += particle.spin * sweep.min(MAX_SWEEP);
                }
                if particle.radius <= horizon || tick + 1 == frames {
                    particle.radius = 0.0;
                    continue;
                }
                let (flat_x, flat_y) = project(particle.radius, particle.angle, warp, horizon);
                let (x, y) = tilted(flat_x, flat_y, tilt);
                // Orbital speed towards the viewer: the near side moves
                // sideways in the direction of spin, so cos(angle) says
                // which way this point is going.
                let beam = particle.spin * particle.angle.cos() * DISK_EDGE_ON;
                // Motion blur: the last positions, dimmer, drawn first so
                // the particle itself ends up on top.
                for (step, previous) in particle.trail.iter().enumerate() {
                    if let Some((px, py)) = previous {
                        if let Some(index) = cell_at(*px, *py) {
                            let cell = &mut cells[index];
                            cell.symbol = particle.symbol.clone();
                            cell.fg = packed_rgb(
                                (90 / (step + 1)) as u8,
                                (40 / (step + 1)) as u8,
                                (10 / (step + 1)) as u8,
                            );
                            cell.modifier = 0;
                        }
                    }
                }
                let color = beamed(
                    accretion_color(particle.fg, particle.radius, particle.radius0, horizon),
                    beam,
                );
                // The far side's second image: light bent under the shadow
                // as well as over it, faint, close in under the hole.
                if particle.angle.sin() < 0.0 && warp > 0.0 && particle.radius < horizon * 4.0 {
                    let under = horizon * 1.3 + (particle.radius - horizon) * 0.25;
                    let (mx, my) = tilted(flat_x, under, tilt);
                    if let Some(index) = cell_at(mx, my) {
                        let cell = &mut cells[index];
                        if cell.symbol == " " || cell.symbol == "·" {
                            cell.symbol = particle.symbol.clone();
                            cell.fg = beamed(packed_rgb(110, 50, 15), beam);
                            cell.modifier = 0;
                        }
                    }
                }
                if let Some(index) = cell_at(x, y) {
                    let cell = &mut cells[index];
                    cell.symbol = particle.symbol.clone();
                    cell.fg = color;
                    cell.modifier = if particle.radius < particle.radius0 * 0.6 {
                        BOLD
                    } else {
                        0
                    };
                }
                if tick >= particle.start {
                    particle.trail.rotate_right(1);
                    particle.trail[0] = Some((x, y));
                }
            }
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
    fn black_hole_pulls_every_character_in_and_ends_black() {
        let source = frame(&[
            "abcdefghijklmnop",
            "                ",
            "q              r",
            "                ",
            "stuvwxyzABCDEFGH",
        ]);
        let frames = black_hole_frames(&source, 60, 7);
        assert_eq!(frames.len(), 60);
        assert_eq!(
            text(&frames[0]),
            text(&source),
            "the first frame is the screen as it was"
        );
        let last = text(&frames[59]);
        assert!(last.trim().is_empty(), "last frame: {last:?}");
        // Characters converge: their mean distance from the centre falls.
        let mean_distance = |frame: &FrameData| {
            let (cx, cy) = (8.0f32, 2.5f32 * ROW_ASPECT);
            let (mut sum, mut count) = (0.0f32, 0usize);
            for (index, cell) in frame.cells.iter().enumerate() {
                if is_blank(cell) || cell.symbol == "·" {
                    continue;
                }
                let x = (index % 16) as f32 + 0.5 - cx;
                let y = ((index / 16) as f32 + 0.5) * ROW_ASPECT - cy;
                sum += (x * x + y * y).sqrt();
                count += 1;
            }
            sum / count.max(1) as f32
        };
        assert!(mean_distance(&frames[0]) > mean_distance(&frames[30]));
        assert!(mean_distance(&frames[30]) > mean_distance(&frames[45]));
    }

    #[test]
    fn black_hole_fades_backgrounds_and_keeps_the_grid() {
        let mut source = frame(&["ab  ", "    ", "    "]);
        source.cells[8].bg = packed_rgb(60, 60, 60);
        let frames = black_hole_frames(&source, 40, 3);
        for frame in &frames {
            assert_eq!(frame.cells.len(), source.cells.len());
            assert!(frame.cursor.is_none());
        }
        assert_eq!(frames[39].cells[8].bg, 0);
        assert_eq!(
            black_hole_frames(&source, 40, 3),
            black_hole_frames(&source, 40, 3)
        );
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
