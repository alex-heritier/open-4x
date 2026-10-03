//! Audio: menu/splash music, the Asian peace loop, UI and unit sounds.
//! All clips are preconverted by tools/prep_assets.py; the game loads
//! OGG music plus WAV effects.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::units::UnitType;

#[derive(Resource)]
pub struct GameAudio {
    pub menu: Handle<AudioSource>,
    pub peace: Handle<AudioSource>,
    pub ui: HashMap<&'static str, Handle<AudioSource>>,
    pub run: HashMap<UnitType, Handle<AudioSource>>,
    pub build: Handle<AudioSource>,
    pub fortify: Handle<AudioSource>,
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
            ui.insert(n, assets.load(format!("gen/audio/ui/{n}.wav")));
        }
        let mut run = HashMap::new();
        run.insert(
            UnitType::Settler,
            assets.load("gen/audio/units/Settler/SetRunFoot1.wav"),
        );
        run.insert(
            UnitType::Worker,
            assets.load("gen/audio/units/Worker/WorkRunFoot1.wav"),
        );
        run.insert(
            UnitType::Warrior,
            assets.load("gen/audio/units/warrior/WarriorRunFoot1.wav"),
        );
        run.insert(
            UnitType::Scout,
            assets.load("gen/audio/units/Scout/ScoutRunFoot1.wav"),
        );
        run.insert(
            UnitType::Archer,
            assets.load("gen/audio/units/Archer/ArchRunFoot1.wav"),
        );
        run.insert(
            UnitType::Spearman,
            assets.load("gen/audio/units/Spearman/SpearmanRunFoot1.wav"),
        );
        run.insert(
            UnitType::Horseman,
            assets.load("gen/audio/units/Horseman/HorsemanRunHooves.wav"),
        );
        Self {
            menu: assets.load("gen/audio/music/menu.ogg"),
            peace: assets.load("gen/audio/music/as_early_peace.ogg"),
            ui,
            run,
            build: assets.load("gen/audio/units/Settler/SettlerBuild.wav"),
            fortify: assets.load("gen/audio/units/warrior/WarriorFortify.wav"),
            work_road: assets.load("gen/audio/units/Worker/WorkRoadShovelIn.wav"),
            work_irrigate: assets.load("gen/audio/units/Worker/WorkIrrigateHoe1.wav"),
            work_mine: assets.load("gen/audio/units/Worker/WorkMinePickAxe.wav"),
            work_clear: assets.load("gen/audio/units/Worker/WorkForestAxe.wav"),
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
        .spawn((
            AudioPlayer(audio.peace.clone()),
            PlaybackSettings::LOOP,
        ))
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
        }
    }
}
