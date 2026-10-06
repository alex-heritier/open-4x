//! Audio: menu/splash music, the Asian peace loop, UI and unit sounds.
//! All clips are preconverted by tools/prep_assets.py; the game loads
//! OGG music plus WAV effects.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::units::UnitType;

/// Run sounds, `(Art/Units folder, wav in it)`.
const RUN_SOUNDS: &[(&str, &str)] = &[
    ("settler", "SetRunFoot1.wav"),
    ("worker", "WorkRunFoot1.wav"),
    ("warrior", "WarriorRunFoot1.wav"),
    ("scout", "ScoutRunFoot1.wav"),
    ("archer", "ArchRunFoot1.wav"),
    ("spearman", "SpearmanRunFoot1.wav"),
    ("horseman", "HorsemanRunHooves.wav"),
];

/// Fortify sounds, by `Art/Units` folder.
const FORTIFY_SOUNDS: &[(&str, &str)] = &[("warrior", "WarriorFortify.wav")];

/// A wav in the cached folder of the unit art `art`; nothing plays when the
/// ruleset has no such unit.
fn unit_sound(assets: &AssetServer, art: &str, file: &str) -> Handle<AudioSource> {
    match crate::ruleset::get().art.unit_key(art) {
        Some(key) => assets.load(format!("{}/{key}/{file}", crate::assets::CACHE_URL)),
        None => Handle::default(),
    }
}

/// The sound table key of a unit: its art folder, lowercase.
pub fn sound_key(t: UnitType) -> String {
    t.row().art.to_ascii_lowercase()
}

#[derive(Resource)]
pub struct GameAudio {
    pub menu: Handle<AudioSource>,
    pub peace: Handle<AudioSource>,
    pub ui: HashMap<&'static str, Handle<AudioSource>>,
    /// Movement sound by `Art/Units` folder (lowercase).
    pub run: HashMap<String, Handle<AudioSource>>,
    pub build: Handle<AudioSource>,
    /// Fortify sound by `Art/Units` folder (lowercase).
    pub fortify: HashMap<String, Handle<AudioSource>>,
    pub work_road: Handle<AudioSource>,
    pub work_irrigate: Handle<AudioSource>,
    pub work_mine: Handle<AudioSource>,
    pub work_clear: Handle<AudioSource>,
    pub music: Option<Entity>,
}

impl GameAudio {
    fn load(assets: &AssetServer) -> Self {
        let ui_names = [
            "Select",
            "Button OK",
            "City View",
            "EnterTurn",
            "WhatToBuild",
            "Hut",
            "Barbarian Raid",
        ];
        let mut ui = HashMap::new();
        for n in ui_names {
            ui.insert(n, assets.load(format!("cache/audio/ui/{n}.wav")));
        }
        // Unit sounds are keyed by the unit's `Art/Units` folder (lowercase),
        // the Civ3 file they come from.
        let mut run = HashMap::new();
        for (art, file) in RUN_SOUNDS {
            run.insert(art.to_string(), unit_sound(assets, art, file));
        }
        let mut fortify = HashMap::new();
        for (art, file) in FORTIFY_SOUNDS {
            fortify.insert(art.to_string(), unit_sound(assets, art, file));
        }
        Self {
            menu: assets.load("cache/audio/music/menu.ogg"),
            peace: assets.load("cache/audio/music/as_early_peace.ogg"),
            ui,
            run,
            build: unit_sound(assets, "settler", "SettlerBuild.wav"),
            fortify,
            work_road: unit_sound(assets, "worker", "WorkRoadShovelIn.wav"),
            work_irrigate: unit_sound(assets, "worker", "WorkIrrigateHoe1.wav"),
            work_mine: unit_sound(assets, "worker", "WorkMinePickAxe.wav"),
            work_clear: unit_sound(assets, "worker", "WorkForestAxe.wav"),
            music: None,
        }
    }
}

/// Play a one-shot effect.
pub fn sfx(commands: &mut Commands, audio: &GameAudio, name: &str) {
    if let Some(h) = audio.ui.get(name) {
        commands.spawn(AudioPlayer(h.clone()));
    }
}

/// Start the looping peace music, stopping whatever music plays.
pub fn start_peace_music(commands: &mut Commands, audio: &mut GameAudio) {
    if let Some(e) = audio.music.take() {
        commands.entity(e).despawn();
    }
    let e = commands
        .spawn((AudioPlayer(audio.peace.clone()), PlaybackSettings::LOOP))
        .id();
    audio.music = Some(e);
}

pub fn setup_audio(mut commands: Commands, assets: Res<AssetServer>) {
    let mut audio = GameAudio::load(&assets);
    let e = commands.spawn(AudioPlayer(audio.menu.clone())).id();
    audio.music = Some(e);
    commands.insert_resource(audio);
}

impl GameAudio {
    pub fn work_sfx(&self, a: crate::improvements::WorkAction) -> Handle<AudioSource> {
        use crate::improvements::WorkAction::*;
        match a {
            Road => self.work_road.clone(),
            Irrigate => self.work_irrigate.clone(),
            Mine => self.work_mine.clone(),
            Clear => self.work_clear.clone(),
            Fortress | Barricade | Outpost => self.work_mine.clone(),
        }
    }
}
