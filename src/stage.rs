//! A 1024 x 768 stage for the screens built on Civ3's full-window art (the
//! diplomacy screens, the wonder splash, the Wonders window).
//!
//! The art is 1024 x 768 and every control sits at a fixed pixel in it, so a
//! screen is written in those coordinates and the stage scales them to the
//! window: `min(width / 1024, height / 768)`, never above 1 (the art is not
//! enlarged; a bigger window leaves a margin around it).
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

pub const WIDTH: f32 = 1024.0;
pub const HEIGHT: f32 = 768.0;
/// Smallest scale worth drawing.
const SMALLEST: f32 = 0.4;

/// The scale of the stage for a window of this size.
pub fn scale_for(width: f32, height: f32) -> f32 {
    (width / WIDTH).min(height / HEIGHT).clamp(SMALLEST, 1.0)
}

/// The scale for the primary window (1 without one, as in tests).
pub fn scale_of(windows: &Query<&Window, With<PrimaryWindow>>) -> f32 {
    windows.single().map_or(1.0, |w| scale_for(w.width(), w.height()))
}

/// One scale, applied to every length of a screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stage(pub f32);

impl Stage {
    /// A length of the art, in logical pixels of the window.
    pub fn px(self, v: f32) -> Val {
        Val::Px((v * self.0).round())
    }

    pub fn font(self, size: f32) -> f32 {
        (size * self.0).round().max(8.0)
    }

    /// A box of the art, placed with its top left corner at `(x, y)`.
    pub fn rect(self, x: f32, y: f32, w: f32, h: f32) -> Node {
        Node {
            position_type: PositionType::Absolute,
            left: self.px(x),
            top: self.px(y),
            width: self.px(w),
            height: self.px(h),
            ..default()
        }
    }
}

/// Text on parchment.
pub const INK: Color = Color::BLACK;
/// Button labels, and the wash of a button.
pub const LABEL: Color = Color::srgb(0.05, 0.3, 0.65);
pub const WASH: Color = Color::srgba(0.65, 0.60, 0.40, 0.22);
pub const WASH_ON: Color = Color::srgba(0.25, 0.55, 0.3, 0.45);
/// A word of warning (a refusal, a note).
pub const WARN: Color = Color::srgb(0.55, 0.1, 0.05);

/// What a screen's builders draw with: the font and the scale.
pub struct Ui<'a> {
    pub font: &'a Handle<Font>,
    pub st: Stage,
}

impl Ui<'_> {
    /// A picture of the art at `(x, y)` of the stage, `w` x `h` big.
    pub fn picture(&self, parent: &mut ChildSpawnerCommands, image: ImageNode, x: f32, y: f32, w: f32, h: f32) -> Entity {
        parent.spawn((self.st.rect(x, y, w, h), image)).id()
    }

    /// Words in a box of the art; `center` centers every line.
    #[allow(clippy::too_many_arguments)]
    pub fn words(&self, parent: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32, h: f32, s: impl Into<String>, size: f32, color: Color, center: bool) {
        parent.spawn((
            self.st.rect(x, y, w, h),
            Text::new(s),
            TextFont { font: self.font.clone(), font_size: self.st.font(size), ..default() },
            TextColor(color),
            TextLayout::new_with_justify(if center { Justify::Center } else { Justify::Left }),
        ));
    }

    /// A button of the art with `action` on it; `on` washes it green.
    #[allow(clippy::too_many_arguments)]
    pub fn button<A: Component>(&self, parent: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32, h: f32, label: impl Into<String>, size: f32, action: A, on: bool) {
        parent
            .spawn((
                Button,
                action,
                Node {
                    border: UiRect::all(Val::Px(1.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..self.st.rect(x, y, w, h)
                },
                BackgroundColor(if on { WASH_ON } else { WASH }),
                BorderColor::all(Color::srgba(0.2, 0.3, 0.12, 0.7)),
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new(label),
                    TextFont { font: self.font.clone(), font_size: self.st.font(size), ..default() },
                    TextColor(LABEL),
                    TextLayout::new_with_justify(Justify::Center),
                ));
            });
    }
}

/// A full-window root that dims the game and centers the stage inside it.
/// Returns the stage node's id; the screen's children go under it.
pub fn spawn<M: Component>(commands: &mut Commands, marker: M, stage: Stage, dim: f32, z: i32) -> Entity {
    let inner = commands
        .spawn(Node { width: stage.px(WIDTH), height: stage.px(HEIGHT), ..default() })
        .id();
    commands
        .spawn((
            marker,
            GlobalZIndex(z),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, dim)),
        ))
        .add_child(inner);
    inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stage_fits_the_window_and_never_grows() {
        assert_eq!(scale_for(1024.0, 768.0), 1.0);
        assert_eq!(scale_for(1280.0, 800.0), 1.0, "the default window shows the art whole");
        assert_eq!(scale_for(2560.0, 1600.0), 1.0, "the art is not enlarged");
        assert_eq!(scale_for(512.0, 768.0), 0.5, "the narrow side decides");
        assert_eq!(scale_for(1024.0, 384.0), 0.5);
        assert_eq!(scale_for(100.0, 100.0), SMALLEST);
    }

    #[test]
    fn lengths_scale_and_round_to_whole_pixels() {
        let s = Stage(0.5);
        assert_eq!(s.px(411.0), Val::Px(206.0));
        assert_eq!(s.font(16.0), 8.0);
        assert_eq!(Stage(1.0).font(3.0), 8.0, "text stays readable");
    }
}
