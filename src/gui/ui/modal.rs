// ============================================================================
// Module:       gui::ui::modal
// Description:  The one modal surface: blurred backdrop, centered card,
//               navigation rail, and the confirmations that layer above it.
//
// Dependencies: eframe::egui; crate::gui::app::{Backdrop, GuiApp}
// ============================================================================

//! The one modal surface: a dimmed, blurred backdrop and a single
//! centered card with a navigation rail down its left side.
//!
//! Everything that used to be its own `egui::Window` — settings,
//! properties, the maintenance tools, the view guide — is a *page* of
//! this one card now. That is what makes cross-links possible: a page
//! can say "the treemap colors live in Appearance" and hand the reader a
//! link that actually takes them there, which six unrelated floating
//! windows could not do.
//!
//! Two things are deliberately *not* pages. A delete confirmation and a
//! destructive-tool confirmation are [`ConfirmKind`]s, and they layer
//! *above* whatever page is open, because "are you sure" has to be able
//! to appear over the Maintenance page that raised it.
//!
//! ## Layering
//!
//! The scrim is an `Order::Middle` area and the card an
//! `Order::Foreground` one, rather than two areas in the same order.
//! Areas within one order are sorted by interaction, and the scrim is
//! deliberately clickable (click-outside dismisses) — so sharing an
//! order would let a click on the scrim raise it above its own card.
//! Separate orders make the stacking a fact rather than a race.
//!
//! ## Why the card waits a frame
//!
//! The blurred backdrop is a screenshot of the window, and a screenshot
//! requested during frame N does not arrive until frame N+1 or N+2. If
//! the card drew immediately it would be in the picture it is being
//! blurred behind. So [`draw_modal`] paints nothing until the snapshot
//! settles — one or two frames, hidden entirely by the open animation
//! that follows.

use crate::gui::app::{Backdrop, GuiApp};
use crate::gui::icons::Icon;
use eframe::egui::{self, Color32, Frame, Margin, RichText, Sense, Stroke, TextStyle, Vec2};

#[cfg(test)]
use super::probes::*;
use super::theme::*;
use super::widgets::*;

/// Which page of the modal is showing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModalPage {
    Locations,
    Appearance,
    Layout,
    Views,
    Cleanups,
    Maintenance,
    Guide,
    About,
}

impl ModalPage {
    pub(crate) const ALL: [Self; 8] = [
        Self::Locations,
        Self::Appearance,
        Self::Layout,
        Self::Views,
        Self::Cleanups,
        Self::Maintenance,
        Self::Guide,
        Self::About,
    ];

    pub(crate) fn label(self) -> String {
        crate::i18n::tr(match self {
            Self::Locations => "page.locations",
            Self::Appearance => "page.appearance",
            Self::Layout => "page.layout",
            Self::Views => "page.views",
            Self::Cleanups => "page.cleanups",
            Self::Maintenance => "page.maintenance",
            Self::Guide => "page.guide",
            Self::About => "page.about",
        })
    }

    pub(crate) fn icon(self) -> Icon {
        match self {
            Self::Locations => Icon::Folder,
            Self::Appearance => Icon::Settings,
            Self::Layout => Icon::LayoutHorizontal,
            Self::Views => Icon::Tree,
            Self::Cleanups => Icon::Export,
            Self::Maintenance => Icon::Tools,
            Self::Guide => Icon::Help,
            Self::About => Icon::App,
        }
    }

    /// The line under the page title. Every page gets one — a heading
    /// with nothing under it was most of what made the old dialogs read
    /// as unfinished.
    pub(crate) fn blurb(self) -> String {
        crate::i18n::tr(match self {
            Self::Locations => "page.locations.blurb",
            Self::Appearance => "page.appearance.blurb",
            Self::Layout => "page.layout.blurb",
            Self::Views => "page.views.blurb",
            Self::Cleanups => "page.cleanups.blurb",
            Self::Maintenance => "page.maintenance.blurb",
            Self::Guide => "page.guide.blurb",
            Self::About => "page.about.blurb",
        })
    }
}

/// A confirmation layered above the page card.
pub(super) enum ConfirmKind {
    Delete,
    WindowsTool(usize),
    /// A user-defined cleanup, already resolved: the card shows the
    /// command itself rather than the template it came from.
    Cleanup,
}

/// Width of the navigation rail. Wide enough for the longest page name
/// plus its icon without wrapping, which is what stops the rail from
/// reflowing as pages are added.
pub(super) const NAV_WIDTH: f32 = 212.0;

/// The card is deliberately generous. It is the app's only modal and it
/// carries every settings, properties, and maintenance surface there is,
/// so the constraint worth optimising is "how much of a page can be read
/// without scrolling", not "how little of the window can it cover".
const CARD_MAX_WIDTH: f32 = 1180.0;
const CARD_MAX_HEIGHT: f32 = 860.0;
const CARD_ROUNDING: u8 = 14;
const CONFIRM_WIDTH: f32 = 470.0;

/// How long the card takes to settle in. Long enough to read as motion,
/// short enough that it never sits between the user and the content.
const OPEN_SECONDS: f32 = 0.13;

/// The one inset every page respects, on both sides. The page header and
/// the scrolled content below it both use it, so the close button, the
/// page title, and the right edge of every box on every page line up in
/// one column.
pub(super) const BODY_PAD: f32 = SPACE_LG;

/// Anything inside a stretching `Frame` has to claim the width it was
/// offered, or the frame shrinks to fit its longest line — which is what
/// left the maintenance rows at a different width each, tracking the
/// length of their own descriptions.
pub(super) fn fill_width(ui: &mut egui::Ui) {
    ui.set_min_width(ui.available_width());
}

pub(super) fn modal_is_open(app: &GuiApp) -> bool {
    app.modal.is_some() || confirm_kind(app).is_some()
}

pub(super) fn confirm_kind(app: &GuiApp) -> Option<ConfirmKind> {
    if app.pending_delete.is_some() {
        return Some(ConfirmKind::Delete);
    }
    if app.tools.pending_cleanup.is_some() {
        return Some(ConfirmKind::Cleanup);
    }
    app.tools.pending.map(ConfirmKind::WindowsTool)
}

/// Paints the whole modal layer, or nothing at all when none is open.
pub(super) fn draw_modal(app: &mut GuiApp, ctx: &egui::Context) {
    if !modal_is_open(app) {
        // Holding a full-window texture for a closed modal is pure waste,
        // and a stale one would be wrong the moment anything scrolls.
        app.backdrop = Backdrop::Idle;
        return;
    }
    if !advance_backdrop(app, ctx) {
        return;
    }

    let opening = ctx.animate_bool_with_time(egui::Id::new("modal_open"), true, OPEN_SECONDS);
    draw_scrim(app, ctx, opening);

    if let Some(page) = app.modal {
        draw_card(app, ctx, page, opening);
    }
    if let Some(confirm) = confirm_kind(app) {
        super::pages::draw_confirm(app, ctx, confirm, opening);
    }
}

/// Drives the screenshot round trip. Returns whether the modal may paint.
fn advance_backdrop(app: &mut GuiApp, ctx: &egui::Context) -> bool {
    match &mut app.backdrop {
        Backdrop::Idle => {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            app.backdrop = Backdrop::Requested { frames_waited: 0 };
            ctx.request_repaint();
            false
        }
        Backdrop::Requested { frames_waited } => {
            // Two frames, then open anyway. A backend that does not
            // implement screenshots (and `egui::Context::default`, which
            // has no backend at all, so every test) would otherwise leave
            // the modal permanently invisible while the app looked hung.
            *frames_waited = frames_waited.saturating_add(1);
            if *frames_waited > 2 {
                app.backdrop = Backdrop::Unavailable;
            }
            ctx.request_repaint();
            false
        }
        Backdrop::Ready(_) | Backdrop::Unavailable => true,
    }
}

/// Turns a captured frame into the texture painted behind the card.
/// Called from `GuiApp::poll_background` when the screenshot arrives.
pub(crate) fn install_backdrop(
    ctx: &egui::Context,
    image: &egui::ColorImage,
    dark: bool,
) -> Backdrop {
    let blurred = blur(image);
    if blurred.pixels.is_empty() {
        return Backdrop::Unavailable;
    }
    let _ = dark;
    Backdrop::Ready(ctx.load_texture(
        "modal_backdrop",
        blurred,
        // Linear filtering does the upscale back to window size, which
        // is a free extra octave of smoothing on top of the box passes.
        egui::TextureOptions::LINEAR,
    ))
}

/// Target width of the downscaled snapshot.
///
/// The blur is done at this size and stretched back up, which is what
/// makes a whole-window gaussian cheap: a few box passes over a
/// several-hundred-pixel-wide image is tens of thousands of operations,
/// not tens of millions, and it happens once per modal open rather than
/// per frame.
///
/// It is also the blur's strength dial, which is not obvious: the
/// downscale factor multiplies the radius below, so at 220 a window
/// 1280px wide was blurred with an effective radius around 20 screen
/// pixels — enough that the app behind the card stopped reading as the
/// app and became a wash of colour. 440 halves the factor and so halves
/// the smear, leaving the window recognisable behind the card while
/// still pushing it clearly behind it.
const BLUR_WIDTH: usize = 440;

/// Box-blur passes. Three approximates a gaussian closely enough that
/// nothing about the result reads as "boxy" once it is stretched back up.
const BLUR_PASSES: usize = 3;

const BLUR_RADIUS: usize = 2;

fn blur(image: &egui::ColorImage) -> egui::ColorImage {
    let [src_w, src_h] = image.size;
    if src_w == 0 || src_h == 0 {
        return egui::ColorImage {
            size: [0, 0],
            source_size: egui::Vec2::ZERO,
            pixels: Vec::new(),
        };
    }
    let factor = src_w.div_ceil(BLUR_WIDTH).max(1);
    let w = src_w.div_ceil(factor).max(1);
    let h = src_h.div_ceil(factor).max(1);

    // Downscale by averaging each source block, rather than by point
    // sampling. Point sampling a window full of one-pixel text produces
    // sparkle that the blur then smears into visible streaks.
    let mut pixels = vec![Color32::TRANSPARENT; w * h];
    for y in 0..h {
        for x in 0..w {
            let (mut r, mut g, mut b, mut n) = (0_u32, 0_u32, 0_u32, 0_u32);
            for sy in (y * factor)..((y + 1) * factor).min(src_h) {
                for sx in (x * factor)..((x + 1) * factor).min(src_w) {
                    let Some(p) = image.pixels.get(sy * src_w + sx) else {
                        continue;
                    };
                    r += p.r() as u32;
                    g += p.g() as u32;
                    b += p.b() as u32;
                    n += 1;
                }
            }
            if n == 0 {
                continue;
            }
            if let Some(slot) = pixels.get_mut(y * w + x) {
                *slot = Color32::from_rgb((r / n) as u8, (g / n) as u8, (b / n) as u8);
            }
        }
    }

    for _ in 0..BLUR_PASSES {
        pixels = box_pass(&pixels, w, h, true);
        pixels = box_pass(&pixels, w, h, false);
    }
    egui::ColorImage {
        size: [w, h],
        source_size: egui::vec2(src_w as f32, src_h as f32),
        pixels,
    }
}

/// One separable box-blur pass, horizontal or vertical.
fn box_pass(src: &[Color32], w: usize, h: usize, horizontal: bool) -> Vec<Color32> {
    let mut out = vec![Color32::TRANSPARENT; src.len()];
    let (outer, inner) = if horizontal { (h, w) } else { (w, h) };
    for a in 0..outer {
        for b in 0..inner {
            let (mut r, mut g, mut bl, mut n) = (0_u32, 0_u32, 0_u32, 0_u32);
            let lo = b.saturating_sub(BLUR_RADIUS);
            let hi = (b + BLUR_RADIUS).min(inner - 1);
            for t in lo..=hi {
                let index = if horizontal { a * w + t } else { t * w + a };
                let Some(p) = src.get(index) else {
                    continue;
                };
                r += p.r() as u32;
                g += p.g() as u32;
                bl += p.b() as u32;
                n += 1;
            }
            if n == 0 {
                continue;
            }
            let index = if horizontal { a * w + b } else { b * w + a };
            if let Some(slot) = out.get_mut(index) {
                *slot = Color32::from_rgb((r / n) as u8, (g / n) as u8, (bl / n) as u8);
            }
        }
    }
    out
}

fn draw_scrim(app: &mut GuiApp, ctx: &egui::Context, opening: f32) {
    let palette = palette();
    let screen = ctx.viewport_rect();
    let response = egui::Area::new(egui::Id::new("modal_scrim"))
        .order(egui::Order::Middle)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let (rect, response) = ui.allocate_exact_size(screen.size(), Sense::click());
            if let Some(texture) = app.backdrop.texture() {
                let mut mesh = egui::Mesh::with_texture(texture.id());
                mesh.add_rect_with_uv(
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE.gamma_multiply(opening),
                );
                ui.painter().add(mesh);
            }
            // The scrim sits *over* the blur. A light theme needs its own
            // tint rather than a weaker black one — dimming a white page
            // toward grey reads as a disabled screenshot, while lifting it
            // toward white reads as depth.
            let veil = if palette.mode.is_dark() {
                Color32::from_black_alpha(150)
            } else {
                Color32::from_rgba_unmultiplied(255, 255, 255, 150)
            };
            ui.painter()
                .rect_filled(rect, 0.0, veil.gamma_multiply(opening));
            #[cfg(test)]
            probe(&TEST_MODAL_SCRIM_RECTS).push(rect);
            response
        })
        .inner;

    // Click-outside dismisses only the topmost layer, the way a stack of
    // dialogs is expected to unwind — dismissing the page underneath a
    // confirmation at the same time would answer a question the user was
    // still being asked.
    if response.clicked() {
        dismiss_top(app);
    }
}

/// Closes the topmost modal layer. Also the `Escape` handler.
pub(super) fn dismiss_top(app: &mut GuiApp) {
    if app.pending_delete.take().is_some() {
        return;
    }
    if app.tools.pending.take().is_some() {
        return;
    }
    app.modal = None;
}

fn draw_card(app: &mut GuiApp, ctx: &egui::Context, page: ModalPage, opening: f32) {
    let palette = palette();
    let screen = ctx.viewport_rect();
    let size = Vec2::new(
        (screen.width() * 0.92).min(CARD_MAX_WIDTH),
        (screen.height() * 0.90).min(CARD_MAX_HEIGHT),
    );
    // Rise into place. egui 0.29 cannot scale a subtree, so the entrance
    // is a translate plus a fade rather than the scale a web modal would
    // use — the read is the same and it costs nothing.
    let lift = (1.0 - opening) * 14.0;
    let pos = screen.center() - size * 0.5 + Vec2::new(0.0, lift);

    egui::Area::new(egui::Id::new("modal_card"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            ui.set_opacity(opening);
            ui.set_min_size(size);
            ui.set_max_size(size);
            card_frame(palette).show(ui, |ui| {
                ui.set_min_size(size);
                ui.set_max_size(size);
                #[cfg(test)]
                probe(&TEST_MODAL_CARD_RECTS).push(ui.max_rect());
                ui.horizontal_top(|ui| {
                    draw_nav(app, ui, page);
                    draw_page_body(app, ui, page, size.y);
                });
            });
        });
}

fn card_frame(palette: Palette) -> Frame {
    Frame::NONE
        .fill(palette.panel)
        .corner_radius(egui::CornerRadius::same(CARD_ROUNDING))
        .stroke(Stroke::new(1.0_f32, palette.border))
        .shadow(egui::epaint::Shadow {
            offset: [0, 16],
            blur: 44,
            spread: 0,
            color: Color32::from_black_alpha(if palette.mode.is_dark() { 160 } else { 60 }),
        })
}

fn draw_nav(app: &mut GuiApp, ui: &mut egui::Ui, page: ModalPage) {
    let palette = palette();
    let height = ui.available_height();
    Frame::NONE
        .fill(palette.app)
        .corner_radius(egui::CornerRadius {
            nw: CARD_ROUNDING - 1,
            sw: CARD_ROUNDING - 1,
            ne: 0,
            se: 0,
        })
        .inner_margin(Margin::symmetric(px(SPACE_MD), px(SPACE_MD)))
        .show(ui, |ui| {
            ui.set_width(NAV_WIDTH);
            ui.set_min_height(height);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    paint_inline_brand(ui, 18.0);
                    ui.label(RichText::new("RustDirStat").strong());
                });
                ui.add_space(SPACE_MD);
                for candidate in ModalPage::ALL {
                    if nav_row(ui, candidate == page, candidate).clicked() {
                        app.modal = Some(candidate);
                    }
                }
            });
        });
}

/// One row of the navigation rail.
///
/// Hand-painted for the same reason `menu_item` is: the icon, the label,
/// and the selected-state bar have to line up down the rail, and a
/// `SelectableLabel` puts each of those wherever its own text happens to
/// land.
fn nav_row(ui: &mut egui::Ui, selected: bool, page: ModalPage) -> egui::Response {
    const ICON: f32 = 16.0;
    const GAP: f32 = 11.0;
    // Not `theme::PAD`. This file glob-imports the theme, so a local
    // `PAD` here would shadow the layout scale's own `PAD` for the length
    // of this function and nowhere else in the file — the same name
    // meaning 10.0 on one line and 12.0 on the next.
    const ICON_INSET: f32 = 10.0;
    const HEIGHT: f32 = 34.0;
    let palette = palette();
    let galley = egui::WidgetText::from(&page.label()).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        TextStyle::Button,
    );
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), HEIGHT), Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if selected {
            palette.accent_muted
        } else {
            hover_fill(ui, &response, Color32::TRANSPARENT, palette.hover)
        };
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(7), fill);
        // The bar is what carries "you are here" at a glance; the fill
        // alone is a subtle enough difference that a fast scan down the
        // rail misses it. It grows out of the middle as the page changes,
        // which ties the rail to the body that just swapped under it.
        let extent = ui.ctx().animate_bool_with_time_and_easing(
            response.id.with("nav_bar"),
            selected,
            HOVER_SECONDS,
            egui::emath::easing::cubic_out,
        );
        if extent > 0.0 {
            let mut bar = rect;
            bar.max.x = bar.min.x + 3.0;
            let inset = 7.0 + (1.0 - extent) * (rect.height() * 0.5 - 7.0).max(0.0);
            ui.painter()
                .rect_filled(bar.shrink2(Vec2::new(0.0, inset)), 2.0, palette.accent);
        }
        let color = if selected {
            palette.on_accent
        } else {
            palette.primary_text
        };
        let icon_rect = egui::Rect::from_center_size(
            egui::pos2(rect.left() + ICON_INSET + ICON * 0.5, rect.center().y),
            Vec2::splat(ICON),
        );
        page.icon().paint(ui.painter(), icon_rect, color);
        ui.painter().galley(
            egui::pos2(
                icon_rect.right() + GAP,
                rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            color,
        );
    }
    #[cfg(test)]
    probe(&TEST_MODAL_NAV_RECTS).push((page, rect));
    response
}

fn draw_page_body(app: &mut GuiApp, ui: &mut egui::Ui, page: ModalPage, card_height: f32) {
    let palette = palette();
    // Measured, not derived from `NAV_WIDTH`. The rail is a frame with
    // its own margins, so it consumes more than the width it was set to —
    // subtracting the bare constant made the body twenty pixels wider
    // than the space actually left, and every box on every page hung that
    // far over the card's right edge.
    let body = Vec2::new(ui.available_width().max(0.0), card_height);
    // The body is given the card's exact remaining size rather than left
    // to work it out. Without this the scroll area below inherits an
    // unbounded height, grows past the bottom of the card, and paints its
    // last rows over the card border and out through the rounded corner.
    ui.allocate_ui_with_layout(body, egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.set_min_size(body);
        ui.set_max_size(body);
        // The scrollbar takes its own space (the app uses solid, always
        // visible bars), so the header has to be inset by the same amount
        // on the right for the close button to sit above the right edge
        // of the content rather than over the bar.
        let bar = ui.spacing().scroll.allocated_width();
        ui.add_space(SPACE_LG);
        ui.horizontal(|ui| {
            ui.add_space(BODY_PAD);
            ui.vertical(|ui| {
                ui.label(RichText::new(page.label()).heading().strong());
                ui.label(RichText::new(page.blurb()).color(palette.secondary_text));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                ui.add_space(BODY_PAD + bar);
                if close_button(ui).clicked() {
                    app.modal = None;
                }
            });
        });
        ui.add_space(SPACE_MD);
        separator(ui);

        // Always scrollable, and always bounded. The windows this
        // replaced sized themselves to their content and ran off the
        // bottom of the screen when the content was taller than the
        // display, which is exactly what the settings dialog did.
        let remaining = ui.available_height().max(0.0);
        egui::ScrollArea::vertical()
            .max_height(remaining)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                Frame::NONE
                    .inner_margin(Margin {
                        left: px(BODY_PAD),
                        right: px(BODY_PAD),
                        top: px(SPACE_MD),
                        bottom: px(BODY_PAD),
                    })
                    .show(ui, |ui| {
                        fill_width(ui);
                        super::pages::draw_page(app, ui, page);
                    });
            });
    });
}

pub(super) fn separator(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, palette().border);
}

fn close_button(ui: &mut egui::Ui) -> egui::Response {
    let response = ui.add(
        egui::Button::new("")
            .frame(false)
            .min_size(Vec2::splat(28.0)),
    );
    let color = if response.hovered() {
        palette().primary_text
    } else {
        palette().secondary_text
    };
    Icon::Close.paint(
        ui.painter(),
        egui::Rect::from_center_size(response.rect.center(), Vec2::splat(13.0)),
        color,
    );
    response.on_hover_text("Close (Esc)")
}

/// The shared shell for a confirmation, drawn above any open page.
pub(super) fn confirm_card(
    ctx: &egui::Context,
    id: &str,
    opening: f32,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let palette = palette();
    let screen = ctx.viewport_rect();
    let width = CONFIRM_WIDTH.min(screen.width() - 40.0).max(240.0);
    let lift = (1.0 - opening) * 14.0;
    egui::Area::new(egui::Id::new(id))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(
            screen.center().x - width * 0.5,
            screen.center().y - 150.0 + lift,
        ))
        .show(ctx, |ui| {
            ui.set_opacity(opening);
            ui.set_max_width(width);
            card_frame(palette)
                .inner_margin(Margin::same(px(SPACE_LG)))
                .show(ui, |ui| {
                    ui.set_width(width - 40.0);
                    add_contents(ui);
                });
        });
}

/// A callout block: a tinted panel with an accent edge, used for the
/// "this cannot be undone" and "needs administrator" notes.
/// How loud a [`callout`] is.
///
/// A callout needs three colours that belong together — a border, a
/// background, and body text — and every call site used to name all
/// three. Nothing stopped one pairing `danger` with `warning_bg`, and
/// nothing would have looked wrong enough to notice.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Tone {
    Danger,
    Warning,
}

impl Tone {
    /// (border, background, text) from the active palette.
    fn colors(self) -> (Color32, Color32, Color32) {
        let palette = palette();
        match self {
            Tone::Danger => (palette.danger, palette.danger_bg, palette.danger_text),
            Tone::Warning => (palette.warning, palette.warning_bg, palette.warning_text),
        }
    }
}

pub(super) fn callout(ui: &mut egui::Ui, tone: Tone, icon: Icon, text: &str) {
    let (tone, fill, text_color) = tone.colors();
    Frame::NONE
        .fill(fill)
        .corner_radius(egui::CornerRadius::same(7))
        .inner_margin(Margin::symmetric(px(SPACE_MD), px(SPACE_SM)))
        .stroke(Stroke::new(1.0_f32, tone))
        .show(ui, |ui| {
            fill_width(ui);
            ui.horizontal(|ui| {
                paint_inline_icon(ui, icon, 15.0, tone);
                ui.add_space(SPACE_XS);
                ui.label(RichText::new(text).color(text_color));
            });
        });
}

/// An inline link that moves the modal to another page — the thing six
/// separate windows could not offer.
pub(super) fn page_link(ui: &mut egui::Ui, page: ModalPage) -> bool {
    let palette = palette();
    let galley = egui::WidgetText::from(&page.label()).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        TextStyle::Body,
    );
    let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::click());
    if ui.is_rect_visible(rect) {
        ui.painter().galley(rect.min, galley, palette.accent);
        if response.hovered() {
            let mut underline = rect;
            underline.min.y = rect.max.y - 1.0;
            ui.painter().rect_filled(underline, 0.0, palette.accent);
        }
    }
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}
