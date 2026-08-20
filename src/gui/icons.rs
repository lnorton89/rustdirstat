//! Small DPI-independent outline icon set for the native GUI.
//!
//! These are painted as egui vector primitives instead of Unicode/emoji,
//! so they render consistently on every machine without depending on an
//! icon font or the platform's emoji fallback.

use eframe::egui::{self, Color32, Painter, Pos2, Rect, Shape, Stroke};

/// Rasterized companion to [`Icon::App`] for the native window and taskbar.
/// Keeping it generated here means the shell icon and in-app mark cannot drift.
pub(super) fn app_icon() -> egui::IconData {
    const SIZE: usize = 64;
    let mut rgba = vec![0_u8; SIZE * SIZE * 4];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let rounded_corner = (x < 6 && y < 6 && (6 - x).pow(2) + (6 - y).pow(2) > 36)
                || (x > 57 && y < 6 && (x - 57).pow(2) + (6 - y).pow(2) > 36)
                || (x < 6 && y > 57 && (6 - x).pow(2) + (y - 57).pow(2) > 36)
                || (x > 57 && y > 57 && (x - 57).pow(2) + (y - 57).pow(2) > 36);
            if rounded_corner {
                continue;
            }
            let color = if !(5..59).contains(&x) || !(5..59).contains(&y) {
                [115, 178, 255, 255]
            } else if x < 31 && y < 34 {
                [55, 129, 229, 255]
            } else if x >= 33 && y < 24 {
                [71, 194, 137, 255]
            } else if x >= 33 && y >= 26 {
                [239, 168, 67, 255]
            } else {
                [168, 92, 216, 255]
            };
            let offset = (y * SIZE + x) * 4;
            rgba[offset..offset + 4].copy_from_slice(&color);
        }
    }
    egui::IconData {
        rgba,
        width: SIZE as u32,
        height: SIZE as u32,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Icon {
    App,
    Folder,
    FolderOpen,
    File,
    Refresh,
    ExternalLink,
    Copy,
    Trash,
    Info,
    ZoomIn,
    ZoomOut,
    Home,
    LayoutHorizontal,
    LayoutVertical,
    Settings,
    Tools,
    ChevronRight,
    ChevronUp,
    ChevronDown,
    Search,
    Duplicate,
    Tree,
    Largest,
    Extensions,
    Export,
    Help,
    /// Checkmark for menu items that toggle a setting on and off.
    Check,
    /// Filled dot for menu items that pick one of a set of choices.
    Dot,
    /// Dismiss control on the modal card.
    Close,
    /// Marks a callout whose consequences the reader has to take in
    /// before acting — a destructive operation, an elevation
    /// requirement.
    Warning,
    /// A theme swatch, for the appearance picker.
    Palette,
    // One per file category, so a row can be recognised by shape before
    // its label is read — the same reason a phone's storage breakdown
    // uses them.
    Archive,
    Image,
    Video,
    Audio,
    Program,
    Source,
}

impl Icon {
    /// The glyph standing for a whole file category.
    ///
    /// Documents, extension-less files and the catch-all share the plain
    /// document mark: inventing distinct shapes for "everything else"
    /// would imply a distinction the categories do not actually make.
    pub(super) fn for_category(category: crate::color::Category) -> Self {
        use crate::color::Category;
        match category {
            Category::Archives => Self::Archive,
            Category::Images => Self::Image,
            Category::Video => Self::Video,
            Category::Audio => Self::Audio,
            Category::Programs => Self::Program,
            Category::Source => Self::Source,
            Category::Documents | Category::NoExtension | Category::Other => Self::File,
        }
    }
}

impl Icon {
    #[cfg(test)]
    pub(super) const ALL: [Self; 37] = [
        Self::App,
        Self::Folder,
        Self::FolderOpen,
        Self::File,
        Self::Refresh,
        Self::ExternalLink,
        Self::Copy,
        Self::Trash,
        Self::Info,
        Self::ZoomIn,
        Self::ZoomOut,
        Self::Home,
        Self::LayoutHorizontal,
        Self::LayoutVertical,
        Self::Settings,
        Self::Tools,
        Self::ChevronRight,
        Self::ChevronUp,
        Self::ChevronDown,
        Self::Search,
        Self::Duplicate,
        Self::Tree,
        Self::Largest,
        Self::Extensions,
        Self::Export,
        Self::Help,
        Self::Check,
        Self::Dot,
        Self::Close,
        Self::Warning,
        Self::Palette,
        Self::Archive,
        Self::Image,
        Self::Video,
        Self::Audio,
        Self::Program,
        Self::Source,
    ];

    pub(super) fn paint(self, painter: &Painter, rect: Rect, color: Color32) {
        self.paint_turned(painter, rect, color, 0.0);
    }

    /// Paints the icon turned `turns` clockwise about the centre of
    /// `rect`, where `1.0` is a full turn.
    ///
    /// The rotation is applied inside the point mapping every icon's
    /// geometry already goes through, so a polyline icon turns exactly.
    /// The icons built from `rect_stroke` or `circle_filled` take
    /// axis-aligned inputs that a point mapping cannot turn, and would
    /// come out skewed — which is fine, because the only caller that
    /// passes a non-zero angle is the tree's expand chevron, and a
    /// chevron is one polyline. At `turns == 0.0` the rotation is the
    /// exact identity, so every other icon is unaffected.
    pub(super) fn paint_turned(self, painter: &Painter, rect: Rect, color: Color32, turns: f32) {
        let s = Stroke::new((rect.width() / 16.0 * 1.45).max(1.0), color);
        let rotation = egui::emath::Rot2::from_angle(turns * std::f32::consts::TAU);
        let center = rect.center();
        let p = |x: f32, y: f32| -> Pos2 {
            let point = egui::pos2(
                rect.left() + rect.width() * x / 16.0,
                rect.top() + rect.height() * y / 16.0,
            );
            center + rotation * (point - center)
        };
        let line = |points: &[(f32, f32)]| {
            painter.add(Shape::line(
                points.iter().map(|&(x, y)| p(x, y)).collect(),
                s,
            ));
        };
        let box_outline = |x1: f32, y1: f32, x2: f32, y2: f32, radius: f32| {
            painter.rect_stroke(Rect::from_min_max(p(x1, y1), p(x2, y2)), radius, s);
        };

        match self {
            Self::App => {
                box_outline(1.5, 2.0, 14.5, 14.0, 2.0);
                painter.rect_filled(Rect::from_min_max(p(3.0, 4.0), p(7.3, 8.0)), 1.0, color);
                painter.rect_filled(Rect::from_min_max(p(8.4, 4.0), p(13.0, 6.1)), 1.0, color);
                painter.rect_filled(Rect::from_min_max(p(8.4, 7.2), p(13.0, 12.0)), 1.0, color);
                painter.rect_filled(Rect::from_min_max(p(3.0, 9.1), p(7.3, 12.0)), 1.0, color);
            }
            Self::Folder | Self::FolderOpen => {
                line(&[
                    (1.5, 5.0),
                    (1.5, 13.5),
                    (14.5, 13.5),
                    (14.5, 5.0),
                    (8.0, 5.0),
                    (6.5, 3.0),
                    (1.5, 3.0),
                    (1.5, 5.0),
                ]);
                if self == Self::FolderOpen {
                    line(&[(2.0, 7.0), (13.8, 7.0), (12.4, 13.5), (1.5, 13.5)]);
                }
            }
            Self::File => {
                line(&[
                    (3.0, 1.5),
                    (9.5, 1.5),
                    (13.0, 5.0),
                    (13.0, 14.5),
                    (3.0, 14.5),
                    (3.0, 1.5),
                ]);
                line(&[(9.5, 1.8), (9.5, 5.0), (12.7, 5.0)]);
                line(&[(5.0, 8.0), (11.0, 8.0)]);
                line(&[(5.0, 11.0), (10.0, 11.0)]);
            }
            Self::Refresh => {
                arc(p(8.0, 8.0), rect.width() * 0.34, -2.6, 0.4, painter, s);
                arc(p(8.0, 8.0), rect.width() * 0.34, 0.55, 3.55, painter, s);
                line(&[(12.4, 2.8), (12.6, 6.2), (9.3, 5.3)]);
                line(&[(3.6, 13.2), (3.4, 9.8), (6.7, 10.7)]);
            }
            Self::ExternalLink => {
                box_outline(2.0, 5.0, 11.0, 14.0, 1.2);
                line(&[(7.0, 9.0), (14.0, 2.0)]);
                line(&[(9.5, 2.0), (14.0, 2.0), (14.0, 6.5)]);
            }
            Self::Copy => {
                box_outline(5.0, 4.5, 14.0, 13.5, 1.2);
                box_outline(2.0, 1.5, 11.0, 10.5, 1.2);
            }
            Self::Trash => {
                line(&[(2.5, 4.0), (13.5, 4.0)]);
                line(&[(6.0, 2.0), (10.0, 2.0), (11.0, 4.0)]);
                line(&[(4.0, 4.0), (4.8, 14.0), (11.2, 14.0), (12.0, 4.0)]);
                line(&[(7.0, 6.5), (7.0, 11.5)]);
                line(&[(9.5, 6.5), (9.5, 11.5)]);
            }
            Self::Info | Self::Help => {
                painter.circle_stroke(p(8.0, 8.0), rect.width() * 0.40, s);
                if self == Self::Info {
                    painter.circle_filled(p(8.0, 4.6), rect.width() * 0.07, color);
                    line(&[(8.0, 7.0), (8.0, 11.5)]);
                } else {
                    arc(p(8.0, 6.7), rect.width() * 0.17, -2.7, 1.2, painter, s);
                    line(&[(8.5, 8.5), (8.0, 10.0)]);
                    painter.circle_filled(p(7.8, 12.2), rect.width() * 0.06, color);
                }
            }
            Self::ZoomIn | Self::ZoomOut | Self::Search => {
                painter.circle_stroke(p(6.8, 6.8), rect.width() * 0.30, s);
                line(&[(10.2, 10.2), (14.0, 14.0)]);
                if self != Self::Search {
                    line(&[(4.5, 6.8), (9.1, 6.8)]);
                    if self == Self::ZoomIn {
                        line(&[(6.8, 4.5), (6.8, 9.1)]);
                    }
                }
            }
            Self::Home => {
                line(&[(1.5, 7.0), (8.0, 1.8), (14.5, 7.0)]);
                line(&[(3.3, 6.0), (3.3, 14.0), (12.7, 14.0), (12.7, 6.0)]);
                box_outline(6.3, 9.0, 9.7, 14.0, 0.5);
            }
            Self::LayoutHorizontal => {
                box_outline(1.5, 2.0, 14.5, 14.0, 1.5);
                line(&[(1.8, 8.0), (14.2, 8.0)]);
            }
            Self::LayoutVertical => {
                box_outline(1.5, 2.0, 14.5, 14.0, 1.5);
                line(&[(8.0, 2.3), (8.0, 13.7)]);
            }
            Self::Settings => {
                painter.circle_stroke(p(8.0, 8.0), rect.width() * 0.22, s);
                painter.circle_stroke(p(8.0, 8.0), rect.width() * 0.40, s);
                for i in 0..8 {
                    let a = i as f32 * std::f32::consts::TAU / 8.0;
                    let (sin, cos) = a.sin_cos();
                    painter.line_segment(
                        [
                            p(8.0 + cos * 6.0, 8.0 + sin * 6.0),
                            p(8.0 + cos * 7.3, 8.0 + sin * 7.3),
                        ],
                        s,
                    );
                }
            }
            Self::Tools => {
                line(&[(3.0, 13.5), (10.0, 6.5)]);
                painter.circle_stroke(p(3.0, 13.0), rect.width() * 0.12, s);
                arc(p(11.0, 5.0), rect.width() * 0.27, -0.5, 3.7, painter, s);
                line(&[(12.8, 1.7), (10.4, 4.1), (12.0, 5.7), (14.4, 3.3)]);
            }
            Self::ChevronRight => line(&[(5.5, 3.0), (10.5, 8.0), (5.5, 13.0)]),
            Self::ChevronUp => line(&[(3.0, 10.5), (8.0, 5.5), (13.0, 10.5)]),
            Self::ChevronDown => line(&[(3.0, 5.5), (8.0, 10.5), (13.0, 5.5)]),
            Self::Duplicate => {
                box_outline(1.5, 4.5, 11.5, 14.0, 1.0);
                box_outline(4.5, 1.5, 14.5, 11.0, 1.0);
                line(&[(6.5, 5.0), (12.0, 5.0)]);
                line(&[(6.5, 7.5), (11.0, 7.5)]);
            }
            Self::Tree => {
                line(&[(3.0, 3.0), (3.0, 13.0), (6.0, 13.0)]);
                line(&[(3.0, 6.0), (6.0, 6.0)]);
                box_outline(6.0, 4.0, 14.0, 8.0, 0.8);
                box_outline(6.0, 11.0, 14.0, 15.0, 0.8);
                painter.circle_filled(p(3.0, 2.5), rect.width() * 0.10, color);
            }
            Self::Largest => {
                line(&[(2.0, 14.0), (14.0, 14.0)]);
                painter.rect_filled(Rect::from_min_max(p(3.0, 9.0), p(5.5, 14.0)), 0.8, color);
                painter.rect_filled(Rect::from_min_max(p(7.0, 6.0), p(9.5, 14.0)), 0.8, color);
                painter.rect_filled(Rect::from_min_max(p(11.0, 2.5), p(13.5, 14.0)), 0.8, color);
            }
            Self::Extensions => {
                painter.circle_filled(p(4.0, 4.0), rect.width() * 0.13, color);
                painter.circle_filled(p(12.0, 4.0), rect.width() * 0.13, color);
                painter.circle_filled(p(4.0, 12.0), rect.width() * 0.13, color);
                painter.circle_filled(p(12.0, 12.0), rect.width() * 0.13, color);
                line(&[(5.8, 4.0), (10.2, 4.0)]);
                line(&[(4.0, 5.8), (4.0, 10.2)]);
                line(&[(12.0, 5.8), (12.0, 10.2)]);
                line(&[(5.8, 12.0), (10.2, 12.0)]);
            }
            Self::Export => {
                box_outline(2.0, 8.5, 14.0, 14.0, 1.0);
                line(&[(8.0, 1.5), (8.0, 10.0)]);
                line(&[(4.8, 7.0), (8.0, 10.2), (11.2, 7.0)]);
            }
            Self::Check => {
                line(&[(3.0, 8.4), (6.6, 12.0), (13.0, 4.4)]);
            }
            Self::Dot => {
                painter.circle_filled(p(8.0, 8.0), rect.width() * 0.19, color);
            }
            Self::Close => {
                line(&[(4.0, 4.0), (12.0, 12.0)]);
                line(&[(12.0, 4.0), (4.0, 12.0)]);
            }
            Self::Warning => {
                line(&[(8.0, 2.2), (14.6, 13.4), (1.4, 13.4), (8.0, 2.2)]);
                line(&[(8.0, 6.2), (8.0, 9.8)]);
                painter.circle_filled(p(8.0, 11.6), rect.width() * 0.06, color);
            }
            Self::Palette => {
                painter.circle_stroke(p(8.0, 8.0), rect.width() * 0.40, s);
                painter.circle_filled(p(5.6, 6.4), rect.width() * 0.07, color);
                painter.circle_filled(p(9.0, 5.2), rect.width() * 0.07, color);
                painter.circle_filled(p(11.2, 8.4), rect.width() * 0.07, color);
            }
            Self::Archive => {
                box_outline(2.0, 2.5, 14.0, 6.0, 1.0);
                box_outline(3.0, 6.0, 13.0, 13.5, 1.0);
                line(&[(6.8, 9.2), (9.2, 9.2)]);
            }
            Self::Image => {
                box_outline(2.0, 3.0, 14.0, 13.0, 1.5);
                painter.circle_filled(p(5.8, 6.6), rect.width() * 0.08, color);
                line(&[
                    (2.6, 12.2),
                    (6.6, 8.4),
                    (9.4, 11.0),
                    (11.4, 9.2),
                    (13.4, 11.4),
                ]);
            }
            Self::Video => {
                box_outline(1.8, 3.6, 11.0, 12.4, 1.5);
                line(&[(11.0, 7.2), (14.4, 4.8), (14.4, 11.2), (11.0, 8.8)]);
            }
            Self::Audio => {
                line(&[(6.2, 12.0), (6.2, 3.2), (13.0, 2.0), (13.0, 10.6)]);
                painter.circle_stroke(p(4.5, 12.2), rect.width() * 0.11, s);
                painter.circle_stroke(p(11.3, 10.8), rect.width() * 0.11, s);
            }
            Self::Program => {
                box_outline(1.8, 2.8, 14.2, 13.2, 1.5);
                line(&[(1.8, 6.2), (14.2, 6.2)]);
                painter.circle_filled(p(4.0, 4.5), rect.width() * 0.055, color);
                painter.circle_filled(p(6.1, 4.5), rect.width() * 0.055, color);
            }
            Self::Source => {
                line(&[(5.6, 4.4), (2.0, 8.0), (5.6, 11.6)]);
                line(&[(10.4, 4.4), (14.0, 8.0), (10.4, 11.6)]);
            }
        }
    }
}

fn arc(center: Pos2, radius: f32, start: f32, end: f32, painter: &Painter, stroke: Stroke) {
    let steps = 16;
    let points = (0..=steps)
        .map(|i| {
            let t = start + (end - start) * i as f32 / steps as f32;
            center + egui::vec2(t.cos() * radius, t.sin() * radius)
        })
        .collect();
    painter.add(Shape::line(points, stroke));
}

#[cfg(test)]
mod tests {
    use super::{app_icon, Icon};
    use eframe::egui;

    #[test]
    fn icon_catalog_has_no_missing_variants() {
        // `ALL` is the list the rendering test sweeps, so a variant listed
        // twice silently shrinks that coverage by one. The length is
        // pinned by the array type; what needs asserting is that every
        // slot holds a different icon.
        for (i, icon) in Icon::ALL.iter().enumerate() {
            assert!(
                !Icon::ALL[..i].contains(icon),
                "{icon:?} appears twice in Icon::ALL"
            );
        }
    }

    #[test]
    fn native_icon_is_valid_rgba() {
        let icon = app_icon();
        assert_eq!((icon.width, icon.height), (64, 64));
        assert_eq!(icon.rgba.len(), 64 * 64 * 4);
        assert!(icon.rgba.chunks_exact(4).any(|pixel| pixel[3] != 0));
    }

    #[test]
    fn every_icon_renders_vector_shapes() {
        for icon in Icon::ALL {
            let context = egui::Context::default();
            let output = context.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    icon.paint(
                        ui.painter(),
                        egui::Rect::from_min_size(egui::pos2(4.0, 4.0), egui::Vec2::splat(16.0)),
                        egui::Color32::WHITE,
                    );
                });
            });
            assert!(!output.shapes.is_empty(), "{icon:?} painted no shapes");
        }
    }
}
