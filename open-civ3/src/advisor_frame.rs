//! The frame Civ3 draws every advisor in: the 1024 x 768 background of the
//! advisor, the spaced title, the advisor's portrait over a speech box, the
//! six advisor tabs down the left edge and the close X. The Domestic and
//! Science Advisors are built in it (`domestic.rs`, `tech_tree.rs`); the
//! art comes from the `advisors` stage of `tools/prep_assets.py`.
//!
//! Positions are pixels of the background art, read off Conquests' own
//! screens.

use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::prelude::*;

use crate::stage::Ui;

/// The six advisors in tab order: `advisor_tab.pcx` holds a row each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Domestic = 0,
    Trade = 1,
    Military = 2,
    Foreign = 3,
    Culture = 4,
    Science = 5,
}

const TABS: [Tab; 6] = [
    Tab::Domestic,
    Tab::Trade,
    Tab::Military,
    Tab::Foreign,
    Tab::Culture,
    Tab::Science,
];

impl Tab {
    /// The advisors the clone has a screen for; the others' tabs are drawn
    /// but do nothing.
    fn built(self) -> bool {
        matches!(self, Tab::Domestic | Tab::Foreign | Tab::Science)
    }
}

/// A tab pressed: switch to that advisor (`switch_tabs`).
#[derive(Component, Clone, Copy, Debug)]
pub struct TabClick(pub Tab);

/// Art that changes with the pointer: `{stem}_0.png` idle, `_1` under the
/// pointer, `_2` pressed (`hover_art`).
#[derive(Component, Clone, Copy)]
pub struct ArtButton(pub &'static str);

pub fn art(assets: &AssetServer, path: &str) -> ImageNode {
    ImageNode::new(assets.load(format!("cache/advisors/{path}.png")))
}

/// A button drawn with three-state art, `w` x `h`, with `action` on it.
#[allow(clippy::too_many_arguments)]
pub fn art_button<A: Component>(
    s: &mut ChildSpawnerCommands,
    ui: &Ui,
    assets: &AssetServer,
    stem: &'static str,
    (x, y, w, h): (f32, f32, f32, f32),
    action: A,
) -> Entity {
    s.spawn((
        Button,
        ArtButton(stem),
        action,
        art(assets, &format!("{stem}_0")),
        ui.st.rect(x, y, w, h),
    ))
    .id()
}

/// Civ3 sets the advisor's name in widely spaced capitals.
fn spaced(title: &str) -> String {
    title
        .to_uppercase()
        .split(' ')
        .map(|w| w.chars().map(String::from).collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("     ")
}

/// What one advisor's frame shows.
pub struct Frame<'a> {
    /// The background, a stem under `cache/advisors`.
    pub background: &'a str,
    pub title: &'a str,
    /// The portrait, a stem under `cache/advisors` (`domestic_0` ...).
    pub portrait: String,
    /// What the advisor says in the box under the portrait.
    pub says: String,
    pub tab: Tab,
}

/// Draw the frame on a stage; `close` goes on the X. Without one the
/// advisor must be answered first: no X, and the tabs do nothing.
pub fn frame<A: Component>(
    s: &mut ChildSpawnerCommands,
    ui: &Ui,
    assets: &AssetServer,
    f: &Frame,
    close: Option<A>,
) {
    ui.picture(s, art(assets, f.background), 0.0, 0.0, 1024.0, 768.0);
    ui.words(
        s,
        0.0,
        20.0,
        1024.0,
        30.0,
        spaced(f.title),
        22.0,
        Color::BLACK,
        true,
    );
    // The head rises above the box, which hides the shoulders.
    ui.picture(s, art(assets, &f.portrait), 861.0, -22.0, 150.0, 150.0);
    ui.picture(s, art(assets, "dialog"), 806.0, 110.0, 207.0, 132.0);
    ui.words(
        s,
        815.0,
        118.0,
        190.0,
        116.0,
        f.says.clone(),
        12.0,
        Color::BLACK,
        false,
    );
    for tab in TABS {
        let row = tab as usize;
        let rect = ui.st.rect(4.0, 242.0 + 62.0 * row as f32, 56.0, 56.0);
        if tab == f.tab {
            s.spawn((art(assets, &format!("tab_{row}_2")), rect));
        } else if tab.built() && close.is_some() {
            s.spawn((
                Button,
                ArtButton(TAB_STEMS[row]),
                TabClick(tab),
                art(assets, &format!("tab_{row}_0")),
                rect,
            ));
        } else {
            s.spawn((art(assets, &format!("tab_{row}_0")), rect));
        }
    }
    if let Some(close) = close {
        close_box(s, ui, assets, close);
    }
}

/// Where Civ3 puts every advisor's close X: the boxed X flush in the
/// bottom-right corner of the 1024 x 768 art (`exitBox-backgroundStates`).
pub const CLOSE_BOX: (f32, f32, f32, f32) = (952.0, 720.0, 72.0, 48.0);

/// The boxed close X in the corner of a 1024 x 768 screen.
pub fn close_box<A: Component>(
    s: &mut ChildSpawnerCommands,
    ui: &Ui,
    assets: &AssetServer,
    close: A,
) -> Entity {
    art_button(s, ui, assets, "exitbox", CLOSE_BOX, close)
}

/// `ArtButton` stems of the tabs (a tab pressed shows its active cell).
const TAB_STEMS: [&str; 6] = ["tab_0", "tab_1", "tab_2", "tab_3", "tab_4", "tab_5"];

/// Swap a button's art as the pointer moves over and presses it.
pub fn hover_art(
    assets: Res<AssetServer>,
    mut buttons: Query<(&Interaction, &ArtButton, &mut ImageNode), Changed<Interaction>>,
) {
    for (i, b, mut img) in &mut buttons {
        let state = match i {
            Interaction::Pressed => 2,
            Interaction::Hovered => 1,
            Interaction::None => 0,
        };
        img.image = assets.load(format!("cache/advisors/{}_{state}.png", b.0));
    }
}

/// A tab pressed: close the advisor up and open the one on the tab.
pub fn switch_tabs(
    tabs: Query<(&Interaction, &TabClick), Changed<Interaction>>,
    mut advisors: ResMut<crate::advisors::Advisors>,
    mut domestic: ResMut<crate::domestic::Domestic>,
) {
    let Some((_, tab)) = tabs.iter().find(|(i, _)| **i == Interaction::Pressed) else {
        return;
    };
    match tab.0 {
        Tab::Domestic => {
            advisors.dismiss();
            domestic.show_now();
        }
        Tab::Science => {
            domestic.hide();
            advisors.show(crate::advisors::Screen::Science);
        }
        Tab::Foreign => {
            domestic.hide();
            advisors.show(crate::advisors::Screen::Foreign);
        }
        Tab::Trade | Tab::Military | Tab::Culture => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_spaced_capitals() {
        assert_eq!(spaced("Science Advisor"), "S C I E N C E     A D V I S O R");
    }
}
