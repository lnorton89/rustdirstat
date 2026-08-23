// ============================================================================
// Module:       gui::flags
// Description:  The small flags drawn beside each entry in the language
//               picker, as vector geometry rather than emoji or bitmaps.
//
// Dependencies: eframe::egui; crate::i18n (the tags a flag is looked up by)
// ============================================================================

//! The flags drawn beside each entry in the language picker.
//!
//! Painted, not typed. The obvious way to put a flag in a label is the
//! regional-indicator emoji — `🇩🇪` — and it does not work here: the
//! window bundles egui's own faces, which carry no regional indicators,
//! so every one of those would render as a pair of empty boxes. The
//! obvious second way is a sprite sheet, which is a binary asset nobody
//! can review in a diff. So the flags are geometry, the way
//! [`crate::gui::icons`] and [`crate::brand`] are, and the table below is
//! the whole of their definition.
//!
//! The colours here are literals, and they are the second and last
//! exception to the rule that every colour in drawing code comes from
//! `palette()` — the same exception, for the same reason, as the brand
//! mark. A national flag re-tinted to match the active theme is not that
//! flag. Everything *around* a flag — the border it is drawn with, the
//! row it sits in — is palette work, and stays that way.
//!
//! What a flag stands for here is a *catalogue*, not a country, and the
//! two do not line up: `es` is spoken in twenty countries and `en` in
//! rather more, and the picker offers whichever the translation was
//! written for. The flag is a landmark that lets the eye find a row
//! before it reads one — the name beside it is what actually names the
//! language, which is why no row is ever a flag alone.

use eframe::egui::{self, Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2};

/// A colour in a flag, as straight RGB.
type Rgb = [u8; 3];

/// One weighted stripe of a banded flag: how wide relative to its
/// siblings, and what colour.
///
/// Weighted rather than equal because Spain's are 1:2:1, and a table that
/// could only express equal thirds would need a whole variant of its own
/// to say so.
type Band = (f32, Rgb);

/// A flag, as the family of construction it belongs to.
///
/// Families rather than one variant per country: nine of the twelve
/// shipped flags are stripes, a Nordic cross, or a wedge, and describing
/// them as data means adding a language is usually a one-line change
/// here. The three that are genuinely their own drawing — the Union
/// Flag, Brazil, and the star and crescent — say so.
#[derive(Clone, Copy)]
pub(super) enum Flag {
    /// Stripes running from the top down.
    Horizontal(&'static [Band]),
    /// Stripes running from the hoist across.
    Vertical(&'static [Band]),
    /// A Nordic cross: an off-centre cross on a plain field.
    Nordic { field: Rgb, cross: Rgb },
    /// Two stripes with a triangle driven in from the hoist.
    Wedge { top: Rgb, bottom: Rgb, wedge: Rgb },
    /// A disc on a lozenge on a plain field.
    Lozenge { field: Rgb, lozenge: Rgb, disc: Rgb },
    /// A star and crescent on a plain field.
    StarAndCrescent { field: Rgb, emblem: Rgb },
    /// Overlaid crosses and saltires — the Union Flag, which is not a
    /// family of anything.
    Union,
}

/// The flag for a language tag, or `None` for a tag with no flag in the
/// table.
///
/// An exact match first, so `pt-BR` can fly Brazil's flag while a plain
/// `pt` would fly Portugal's; the base language after that, so a
/// catalogue someone drops in as `de-AT` is still recognisably German.
/// A tag matching neither is not an error — the picker draws a globe in
/// the same space and the row reads the same — which is what keeps a
/// user-supplied catalogue a first-class entry rather than a broken one.
pub(super) fn flag(tag: &str) -> Option<Flag> {
    if let Some((_, found)) = FLAGS.iter().find(|(known, _)| *known == tag) {
        return Some(*found);
    }
    let base = tag.split(['-', '_']).next()?;
    FLAGS
        .iter()
        .find(|(known, _)| *known == base)
        .map(|(_, found)| *found)
}

/// Every flag the picker can draw, by the tag that selects it.
///
/// Ordered by tag. `every_shipped_language_has_a_flag` fails the build
/// when a translation is added to `i18n::SHIPPED` and forgotten here.
///
/// Deliberately not exhaustive over the languages `i18n` can name. Four
/// of them — Greek, Croatian, Slovak, Slovenian — are either too fine
/// to draw at this size or, with the charge that distinguishes them left
/// out, identical to a flag already in this table: an arms-less Slovak
/// flag is Russia's, an arms-less Croatian one is the Netherlands'. A
/// landmark that points at the wrong row is worse than no landmark, so
/// those fall through to the globe.
const FLAGS: &[(&str, Flag)] = &[
    (
        "cs",
        Flag::Wedge {
            top: [255, 255, 255],
            bottom: [215, 20, 26],
            wedge: [17, 69, 126],
        },
    ),
    (
        "da",
        Flag::Nordic {
            field: [198, 12, 48],
            cross: [255, 255, 255],
        },
    ),
    (
        "de",
        Flag::Horizontal(&[
            (1.0, [20, 20, 20]),
            (1.0, [221, 0, 0]),
            (1.0, [255, 206, 0]),
        ]),
    ),
    ("en", Flag::Union),
    (
        "es",
        Flag::Horizontal(&[
            (1.0, [170, 21, 27]),
            (2.0, [241, 191, 0]),
            (1.0, [170, 21, 27]),
        ]),
    ),
    (
        "et",
        Flag::Horizontal(&[
            (1.0, [0, 114, 206]),
            (1.0, [16, 16, 16]),
            (1.0, [255, 255, 255]),
        ]),
    ),
    (
        "fi",
        Flag::Nordic {
            field: [255, 255, 255],
            cross: [0, 53, 128],
        },
    ),
    (
        "fr",
        Flag::Vertical(&[
            (1.0, [0, 35, 149]),
            (1.0, [255, 255, 255]),
            (1.0, [237, 41, 57]),
        ]),
    ),
    (
        "hu",
        Flag::Horizontal(&[
            (1.0, [206, 41, 57]),
            (1.0, [255, 255, 255]),
            (1.0, [71, 112, 80]),
        ]),
    ),
    (
        "is",
        Flag::Nordic {
            field: [0, 56, 151],
            cross: [255, 255, 255],
        },
    ),
    (
        "it",
        Flag::Vertical(&[
            (1.0, [0, 140, 69]),
            (1.0, [244, 245, 240]),
            (1.0, [205, 33, 42]),
        ]),
    ),
    (
        "lt",
        Flag::Horizontal(&[
            (1.0, [253, 185, 19]),
            (1.0, [0, 106, 68]),
            (1.0, [193, 39, 45]),
        ]),
    ),
    (
        "lv",
        Flag::Horizontal(&[
            (2.0, [158, 48, 57]),
            (1.0, [255, 255, 255]),
            (2.0, [158, 48, 57]),
        ]),
    ),
    (
        "nb",
        Flag::Nordic {
            field: [186, 12, 47],
            cross: [255, 255, 255],
        },
    ),
    (
        "nl",
        Flag::Horizontal(&[
            (1.0, [174, 28, 40]),
            (1.0, [255, 255, 255]),
            (1.0, [33, 70, 139]),
        ]),
    ),
    (
        "pl",
        Flag::Horizontal(&[(1.0, [255, 255, 255]), (1.0, [220, 20, 60])]),
    ),
    (
        "pt",
        Flag::Vertical(&[(2.0, [0, 102, 71]), (3.0, [255, 0, 0])]),
    ),
    (
        "pt-BR",
        Flag::Lozenge {
            field: [0, 155, 58],
            lozenge: [254, 223, 0],
            disc: [0, 39, 118],
        },
    ),
    (
        "ro",
        Flag::Vertical(&[
            (1.0, [0, 43, 127]),
            (1.0, [252, 209, 22]),
            (1.0, [206, 17, 38]),
        ]),
    ),
    (
        "ru",
        Flag::Horizontal(&[
            (1.0, [255, 255, 255]),
            (1.0, [0, 57, 166]),
            (1.0, [213, 43, 30]),
        ]),
    ),
    (
        "sr",
        Flag::Horizontal(&[
            (1.0, [198, 54, 60]),
            (1.0, [12, 68, 144]),
            (1.0, [255, 255, 255]),
        ]),
    ),
    (
        "sv",
        Flag::Nordic {
            field: [0, 106, 167],
            cross: [254, 204, 0],
        },
    ),
    (
        "tr",
        Flag::StarAndCrescent {
            field: [227, 10, 23],
            emblem: [255, 255, 255],
        },
    ),
    (
        "uk",
        Flag::Horizontal(&[(1.0, [0, 87, 183]), (1.0, [255, 215, 0])]),
    ),
];

/// The width a flag is drawn at, for the height it is given.
///
/// Real flags are mostly 3:2 and the Union Flag is 2:1; one ratio for all
/// of them is what keeps a column of them a column. 3:2 is the one that
/// leaves every emblem here legible at fifteen pixels tall.
pub(super) const ASPECT: f32 = 1.5;

impl Flag {
    /// Paints the flag to fill `rect`, with `border` ruled around it.
    ///
    /// The border is a palette colour supplied by the caller, and it is
    /// not decoration: half the flags here are white along one edge, and
    /// against a light theme's card an unruled Polish flag is a red
    /// stripe floating in space.
    pub(super) fn paint(self, painter: &Painter, rect: Rect, border: Color32) {
        // Every emblem below is placed by proportion and several of them
        // run to the edge, so a clipped painter is what keeps a stroke
        // that rounds outwards off the row beside it.
        let painter = painter.with_clip_rect(rect);
        match self {
            Self::Horizontal(bands) => stripes(&painter, rect, bands, false),
            Self::Vertical(bands) => stripes(&painter, rect, bands, true),
            Self::Nordic { field, cross } => nordic(&painter, rect, field, cross),
            Self::Wedge { top, bottom, wedge } => {
                stripes(&painter, rect, &[(1.0, top), (1.0, bottom)], false);
                // Reaches the midpoint of the flag's length, which is what
                // makes it a wedge rather than a corner.
                painter.add(Shape::convex_polygon(
                    vec![rect.left_top(), rect.center(), rect.left_bottom()],
                    rgb(wedge),
                    Stroke::NONE,
                ));
            }
            Self::Lozenge {
                field,
                lozenge,
                disc,
            } => {
                painter.rect_filled(rect, 0.0, rgb(field));
                let center = rect.center();
                let (dx, dy) = (rect.width() * 0.42, rect.height() * 0.42);
                painter.add(Shape::convex_polygon(
                    vec![
                        egui::pos2(center.x, center.y - dy),
                        egui::pos2(center.x + dx, center.y),
                        egui::pos2(center.x, center.y + dy),
                        egui::pos2(center.x - dx, center.y),
                    ],
                    rgb(lozenge),
                    Stroke::NONE,
                ));
                painter.circle_filled(center, rect.height() * 0.26, rgb(disc));
            }
            Self::StarAndCrescent { field, emblem } => {
                painter.rect_filled(rect, 0.0, rgb(field));
                // The crescent is a disc with a second disc cut out of it,
                // and the cut is made by painting the field colour back
                // over — sound only because the field is flat, which for
                // every star-and-crescent flag it is.
                let moon = egui::pos2(rect.left() + rect.width() * 0.33, rect.center().y);
                painter.circle_filled(moon, rect.height() * 0.25, rgb(emblem));
                painter.circle_filled(
                    moon + Vec2::new(rect.width() * 0.05, 0.0),
                    rect.height() * 0.20,
                    rgb(field),
                );
                star(
                    &painter,
                    egui::pos2(rect.left() + rect.width() * 0.53, rect.center().y),
                    rect.height() * 0.16,
                    rgb(emblem),
                );
            }
            Self::Union => union_flag(&painter, rect),
        }
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0_f32, border),
            egui::StrokeKind::Inside,
        );
    }
}

fn rgb(color: Rgb) -> Color32 {
    Color32::from_rgb(color[0], color[1], color[2])
}

/// Bands across `rect`, weighted, running down or across.
fn stripes(painter: &Painter, rect: Rect, bands: &[Band], vertical: bool) {
    let total: f32 = bands.iter().map(|(weight, _)| weight).sum();
    if total <= 0.0 {
        return;
    }
    let extent = if vertical {
        rect.width()
    } else {
        rect.height()
    };
    let mut offset = 0.0_f32;
    for (weight, color) in bands {
        let span = extent * weight / total;
        // Each band is drawn from its start to the *next* band's start
        // rather than to its own computed end: a rounded edge between two
        // of them would otherwise leave a hairline of whatever is behind.
        let band = if vertical {
            Rect::from_min_max(
                egui::pos2(rect.left() + offset, rect.top()),
                egui::pos2(rect.left() + offset + span.ceil(), rect.bottom()),
            )
        } else {
            Rect::from_min_max(
                egui::pos2(rect.left(), rect.top() + offset),
                egui::pos2(rect.right(), rect.top() + offset + span.ceil()),
            )
        };
        painter.rect_filled(band.intersect(rect), 0.0, rgb(*color));
        offset += span;
    }
}

/// A Nordic cross: arms a fifth of the height, the upright shifted
/// towards the hoist.
fn nordic(painter: &Painter, rect: Rect, field: Rgb, cross: Rgb) {
    painter.rect_filled(rect, 0.0, rgb(field));
    let arm = (rect.height() * 0.2).max(1.0);
    let upright = rect.left() + rect.width() * 0.375;
    painter.rect_filled(
        Rect::from_min_max(
            egui::pos2(upright - arm * 0.5, rect.top()),
            egui::pos2(upright + arm * 0.5, rect.bottom()),
        ),
        0.0,
        rgb(cross),
    );
    painter.rect_filled(
        Rect::from_min_max(
            egui::pos2(rect.left(), rect.center().y - arm * 0.5),
            egui::pos2(rect.right(), rect.center().y + arm * 0.5),
        ),
        0.0,
        rgb(cross),
    );
}

/// A five-pointed star, `radius` to its points.
///
/// Built as a convex core with five triangles on it rather than as one
/// ten-vertex outline, because that outline is concave and egui fills
/// convex polygons — the concave version comes out as a pentagon with
/// the points folded inside it. Below a couple of pixels the whole thing
/// is a dot anyway, and it is drawn as one: five sub-pixel triangles
/// tessellate to a smudge.
fn star(painter: &Painter, center: Pos2, radius: f32, color: Color32) {
    if radius < 2.0 {
        painter.circle_filled(center, radius, color);
        return;
    }
    let point = |index: usize, distance: f32| -> Pos2 {
        // Index 0 is straight up, so the star sits on two feet.
        let angle = std::f32::consts::TAU * index as f32 / 5.0 - std::f32::consts::FRAC_PI_2;
        center + Vec2::angled(angle) * distance
    };
    let inner = radius * 0.42;
    let core: Vec<Pos2> = (0..5)
        .map(|index| {
            let angle =
                std::f32::consts::TAU * (index as f32 + 0.5) / 5.0 - std::f32::consts::FRAC_PI_2;
            center + Vec2::angled(angle) * inner
        })
        .collect();
    painter.add(Shape::convex_polygon(core.clone(), color, Stroke::NONE));
    for index in 0..5 {
        let Some(left) = core.get((index + 4) % 5) else {
            continue;
        };
        let Some(right) = core.get(index) else {
            continue;
        };
        painter.add(Shape::convex_polygon(
            vec![point(index, radius), *right, *left],
            color,
            Stroke::NONE,
        ));
    }
}

/// The Union Flag: two saltires and two crosses, counterchanged.
///
/// The offset that counterchanges the red saltire is left out, and at
/// fifteen pixels tall it would land inside a pixel — what carries the
/// flag at this size is the order the six pieces are laid down in.
fn union_flag(painter: &Painter, rect: Rect) {
    const BLUE: Rgb = [1, 33, 105];
    const RED: Rgb = [200, 16, 46];
    const WHITE: Rgb = [255, 255, 255];

    painter.rect_filled(rect, 0.0, rgb(BLUE));
    let diagonals = |width: f32, color: Rgb| {
        let stroke = Stroke::new(width.max(1.0), rgb(color));
        painter.line_segment([rect.left_top(), rect.right_bottom()], stroke);
        painter.line_segment([rect.left_bottom(), rect.right_top()], stroke);
    };
    diagonals(rect.height() * 0.28, WHITE);
    diagonals(rect.height() * 0.12, RED);

    let cross = |width: f32, color: Rgb| {
        let arm = (rect.height() * width).max(1.0);
        painter.rect_filled(
            Rect::from_center_size(rect.center(), Vec2::new(arm, rect.height())),
            0.0,
            rgb(color),
        );
        painter.rect_filled(
            Rect::from_center_size(rect.center(), Vec2::new(rect.width(), arm)),
            0.0,
            rgb(color),
        );
    };
    cross(0.36, WHITE);
    cross(0.20, RED);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shipped_language_has_a_flag() -> anyhow::Result<()> {
        for tag in crate::i18n::shipped_tags() {
            assert!(
                flag(tag).is_some(),
                "{tag} ships as a translation but has no flag in FLAGS"
            );
        }
        Ok(())
    }

    #[test]
    fn a_regional_tag_falls_back_to_its_base_language() -> anyhow::Result<()> {
        // Brazil is its own entry; Austria is not, and has to come out
        // German rather than blank.
        assert!(
            matches!(flag("pt-BR"), Some(Flag::Lozenge { .. })),
            "pt-BR should be Brazil's flag, not Portugal's"
        );
        assert!(
            matches!(flag("de-AT"), Some(Flag::Horizontal(_))),
            "a regional German tag should still fly a flag"
        );
        assert!(flag("qqq").is_none(), "an unknown tag has no flag");
        Ok(())
    }

    #[test]
    fn the_table_is_ordered_and_holds_each_tag_once() -> anyhow::Result<()> {
        for pair in FLAGS.windows(2) {
            let [(before, _), (after, _)] = pair else {
                continue;
            };
            assert!(
                before < after,
                "FLAGS is out of order (or repeats): {before} before {after}"
            );
        }
        Ok(())
    }

    #[test]
    fn every_flag_paints_its_own_colours() -> anyhow::Result<()> {
        // A flag that paints nothing, or paints only the field it starts
        // from, is a solid rectangle in the picker — which looks like a
        // deliberate design rather than a bug, so nothing else would
        // catch it.
        for (tag, flag) in FLAGS {
            let context = egui::Context::default();
            let mut output = context.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    flag.paint(
                        ui.painter(),
                        Rect::from_min_size(egui::pos2(4.0, 4.0), Vec2::new(30.0, 20.0)),
                        Color32::BLACK,
                    );
                });
            });
            output.textures_delta.clear();
            assert!(
                output.shapes.len() > 2,
                "{tag} painted {} shapes; a flag is at least a field, an \
                 emblem and its border",
                output.shapes.len()
            );
        }
        Ok(())
    }
}
