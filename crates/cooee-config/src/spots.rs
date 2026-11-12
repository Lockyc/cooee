//! Popup spots — the faces window's Talk and Aside, and where the controls and share windows
//! were last left — and sizes: pure geometry in logical points, top-left origin
//! (Tauri's screen coordinate space).

use serde::{Deserialize, Serialize};

use crate::Size;

/// The gap between a popup and the screen edge it is placed against.
pub const EDGE_INSET: f64 = 24.0;
const SPOTS_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    fn overlap(&self, o: &Rect) -> f64 {
        let w = (self.x + self.w).min(o.x + o.w) - self.x.max(o.x);
        let h = (self.y + self.h).min(o.y + o.h) - self.y.max(o.y);
        if w > 0.0 && h > 0.0 {
            w * h
        } else {
            0.0
        }
    }
}

/// The saved spots. `None` means "never saved": the default for that spot applies.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Spots {
    pub talk: Option<Rect>,
    pub aside: Option<Rect>,
    /// Where the controls window was last left — written whenever it moves or resizes.
    #[serde(default)]
    pub controls: Option<Rect>,
    /// Where the share window was last left — written whenever it moves or resizes.
    #[serde(default)]
    pub share: Option<Rect>,
}

/// The controls window's size: a 16:9 preview beside three buttons.
const CONTROLS_SIZE: (f64, f64) = (340.0, 100.0);
/// The share window's first size; after that, whatever you made it.
const SHARE_SIZE: (f64, f64) = (960.0, 600.0);

/// The controls window's default: the bottom-left corner of `screen`, clear of Aside.
pub fn default_controls(screen: Rect) -> Rect {
    let (w, h) = CONTROLS_SIZE;
    Rect {
        x: screen.x + EDGE_INSET,
        y: screen.y + screen.h - h - EDGE_INSET,
        w,
        h,
    }
}

/// Whether a popup's `current` rect is a move or resize by hand, not the echo of cooee placing
/// it at `placed` (the Moved/Resized events `set_position`/`set_size` fire, which round through
/// physical pixels). Placement may have clamped a saved spot onto a smaller screen set; writing
/// that back would lose the spot the user chose.
pub fn moved_by_hand(current: Rect, placed: Option<Rect>) -> bool {
    let Some(p) = placed else {
        return true;
    };
    let near = |a: f64, b: f64| (a - b).abs() < 1.0;
    !(near(current.x, p.x) && near(current.y, p.y) && near(current.w, p.w) && near(current.h, p.h))
}

/// The share window's default: centred on `screen`.
pub fn default_share(screen: Rect) -> Rect {
    let (w, h) = SHARE_SIZE;
    Rect {
        x: screen.x + (screen.w - w) / 2.0,
        y: screen.y + (screen.h - h) / 2.0,
        w,
        h,
    }
}

#[derive(Serialize, Deserialize)]
struct SpotsFile {
    version: u32,
    #[serde(flatten)]
    spots: Spots,
}

impl Spots {
    pub fn to_json(&self) -> String {
        serde_json::to_string(&SpotsFile {
            version: SPOTS_VERSION,
            spots: *self,
        })
        .expect("spots serialize")
    }

    /// An unreadable file, or one from another version, is no saved spots.
    pub fn from_json(s: &str) -> Spots {
        match serde_json::from_str::<SpotsFile>(s) {
            Ok(f) if f.version == SPOTS_VERSION => f.spots,
            _ => Spots::default(),
        }
    }
}

/// Talk, the faces window's primary spot: centred on `screen` and flush with its top (`screen`
/// is the work area, so directly under the menu bar) — directly under the webcam of a laptop or
/// a TV-mounted camera, so you look at the camera while watching faces.
pub fn default_talk(screen: Rect, size: Size) -> Rect {
    let (w, h) = (size.width as f64, size.height as f64);
    Rect {
        x: screen.x + (screen.w - w) / 2.0,
        y: screen.y,
        w,
        h,
    }
}

/// Aside's default: the bottom-right corner of `screen`.
pub fn default_aside(screen: Rect, size: Size) -> Rect {
    let (w, h) = (size.width as f64, size.height as f64);
    Rect {
        x: screen.x + screen.w - w - EDGE_INSET,
        y: screen.y + screen.h - h - EDGE_INSET,
        w,
        h,
    }
}

/// The next size wider than `current`, wrapping to the smallest; the top-centre stays put.
pub fn cycle_size(current: Rect, sizes: &[Size]) -> Rect {
    let next = sizes
        .iter()
        .find(|s| s.width as f64 > current.w)
        .or_else(|| sizes.first())
        .copied()
        .unwrap_or(Size {
            width: current.w as u32,
            height: current.h as u32,
        });
    let (w, h) = (next.width as f64, next.height as f64);
    Rect {
        x: current.x + current.w / 2.0 - w / 2.0,
        y: current.y,
        w,
        h,
    }
}

/// Fit `r` wholly onto the screen it overlaps most (the first screen if it overlaps none),
/// shrinking it if it is bigger than that screen.
pub fn clamp_to_screens(r: Rect, screens: &[Rect]) -> Rect {
    let Some(first) = screens.first() else {
        return r;
    };
    let s = screens
        .iter()
        .fold((first, r.overlap(first)), |best, s| {
            let o = r.overlap(s);
            if o > best.1 {
                (s, o)
            } else {
                best
            }
        })
        .0;
    let w = r.w.min(s.w);
    let h = r.h.min(s.h);
    Rect {
        x: r.x.clamp(s.x, s.x + s.w - w),
        y: r.y.clamp(s.y, s.y + s.h - h),
        w,
        h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Size;

    const TV: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 3840.0,
        h: 2160.0,
    };
    const SIZES: [Size; 3] = [
        Size {
            width: 360,
            height: 225,
        },
        Size {
            width: 560,
            height: 350,
        },
        Size {
            width: 800,
            height: 500,
        },
    ];

    #[test]
    fn default_talk_is_top_centre_flush_under_the_menu_bar() {
        let work_area = Rect { y: 25.0, ..TV };
        let r = default_talk(
            work_area,
            Size {
                width: 560,
                height: 350,
            },
        );
        assert_eq!(
            r,
            Rect {
                x: 1640.0,
                y: 25.0,
                w: 560.0,
                h: 350.0
            }
        );
    }

    #[test]
    fn default_aside_is_bottom_right_inset() {
        let r = default_aside(
            TV,
            Size {
                width: 360,
                height: 225,
            },
        );
        assert_eq!(
            r,
            Rect {
                x: 3840.0 - 360.0 - 24.0,
                y: 2160.0 - 225.0 - 24.0,
                w: 360.0,
                h: 225.0
            }
        );
    }

    #[test]
    fn cycle_size_steps_and_wraps_keeping_top_centre() {
        let small = Rect {
            x: 1740.0,
            y: 24.0,
            w: 360.0,
            h: 225.0,
        };
        let mid = cycle_size(small, &SIZES);
        assert_eq!(
            mid,
            Rect {
                x: 1640.0,
                y: 24.0,
                w: 560.0,
                h: 350.0
            }
        );
        let big = cycle_size(mid, &SIZES);
        assert_eq!(big.w, 800.0);
        assert_eq!(big.x + big.w / 2.0, 1920.0);
        let wrapped = cycle_size(big, &SIZES);
        assert_eq!(wrapped.w, 360.0);
        assert_eq!(wrapped.y, 24.0);
    }

    #[test]
    fn cycle_size_from_a_hand_resized_width_goes_to_the_next_larger() {
        let odd = Rect {
            x: 0.0,
            y: 0.0,
            w: 400.0,
            h: 300.0,
        };
        assert_eq!(cycle_size(odd, &SIZES).w, 560.0);
    }

    #[test]
    fn clamp_moves_offscreen_rect_onto_screen() {
        let laptop = Rect {
            x: -1512.0,
            y: 0.0,
            w: 1512.0,
            h: 982.0,
        };
        // Saved on a screen that has since been unplugged: overlaps nothing.
        let gone = Rect {
            x: 5000.0,
            y: 100.0,
            w: 560.0,
            h: 350.0,
        };
        let r = clamp_to_screens(gone, &[TV, laptop]);
        assert_eq!(
            r,
            Rect {
                x: 3840.0 - 560.0,
                y: 100.0,
                w: 560.0,
                h: 350.0
            }
        );
    }

    #[test]
    fn clamp_pulls_a_straddling_rect_onto_the_screen_it_mostly_covers() {
        let laptop = Rect {
            x: -1512.0,
            y: 0.0,
            w: 1512.0,
            h: 982.0,
        };
        let straddle = Rect {
            x: -100.0,
            y: 10.0,
            w: 560.0,
            h: 350.0,
        };
        assert_eq!(clamp_to_screens(straddle, &[laptop, TV]).x, 0.0);
    }

    #[test]
    fn clamp_keeps_onscreen_rect() {
        let r = Rect {
            x: 100.0,
            y: 100.0,
            w: 560.0,
            h: 350.0,
        };
        assert_eq!(clamp_to_screens(r, &[TV]), r);
    }

    #[test]
    fn clamp_shrinks_a_rect_bigger_than_the_screen() {
        let small = Rect {
            x: 0.0,
            y: 0.0,
            w: 300.0,
            h: 200.0,
        };
        let r = clamp_to_screens(
            Rect {
                x: 0.0,
                y: 0.0,
                w: 800.0,
                h: 500.0,
            },
            &[small],
        );
        assert_eq!(r, small);
    }

    #[test]
    fn spots_round_trip_json() {
        let s = Spots {
            talk: Some(Rect {
                x: 1.0,
                y: 2.0,
                w: 3.0,
                h: 4.0,
            }),
            aside: None,
            ..Spots::default()
        };
        let json = s.to_json();
        assert!(json.contains("\"version\":1"), "{json}");
        assert_eq!(Spots::from_json(&json), s);
    }

    const LAPTOP: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 1440.0,
        h: 900.0,
    };

    #[test]
    fn placing_a_popup_is_not_a_move_to_remember() {
        let placed = Rect {
            x: 24.0,
            y: 776.0,
            w: 340.0,
            h: 100.0,
        };
        // The Moved/Resized events cooee's own placement fires (rounded through physical pixels).
        assert!(!moved_by_hand(
            Rect {
                x: 24.4,
                y: 775.6,
                ..placed
            },
            Some(placed)
        ));
        assert!(moved_by_hand(Rect { x: 300.0, ..placed }, Some(placed)));
        assert!(moved_by_hand(placed, None), "never placed: a real move");
    }

    #[test]
    fn controls_default_is_bottom_left_inset() {
        assert_eq!(
            default_controls(LAPTOP),
            Rect {
                x: 24.0,
                y: 900.0 - 24.0 - 100.0,
                w: 340.0,
                h: 100.0
            }
        );
    }

    #[test]
    fn share_default_is_centred() {
        assert_eq!(
            default_share(LAPTOP),
            Rect {
                x: 240.0,
                y: 150.0,
                w: 960.0,
                h: 600.0
            }
        );
    }

    #[test]
    fn a_v1_file_without_popup_spots_still_loads() {
        let s = Spots::from_json(r#"{"version":1,"talk":{"x":1,"y":2,"w":3,"h":4},"aside":null}"#);
        assert_eq!(s.talk.map(|r| r.w), Some(3.0));
        assert_eq!(s.controls, None);
        assert_eq!(s.share, None);
    }

    #[test]
    fn unreadable_spots_json_is_empty() {
        assert_eq!(Spots::from_json("not json"), Spots::default());
        assert_eq!(Spots::from_json("{\"version\":99}"), Spots::default());
    }
}
