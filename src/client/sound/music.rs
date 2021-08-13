use std::{
    io::{Cursor, Read},
    rc::Rc,
};

use crate::{client::sound::SoundError, common::vfs::Vfs};

use rodio::{Decoder, OutputStreamHandle, Sink, Source};

/// Plays music tracks.
pub struct MusicPlayer {
    vfs: Rc<Vfs>,
    stream: OutputStreamHandle,
    playing: Option<String>,
    sink: Option<Sink>,
}

impl MusicPlayer {
    pub fn new(vfs: Rc<Vfs>, stream: OutputStreamHandle) -> MusicPlayer {
        MusicPlayer {
            vfs,
            stream,
            playing: None,
            sink: None,
        }
    }

    /// Start playing the track with the given name.
    ///
    /// Music tracks are expected to be in the "music/" directory of the virtual
    /// filesystem, so they can be placed either in an actual directory
    /// `"id1/music/"` or packaged in a PAK archive with a path beginning with
    /// `"music/"`.
    ///
    /// If the specified track is already playing, this has no effect.
    pub fn play_named<S>(&mut self, name: S) -> Result<(), SoundError>
    where
        S: AsRef<str>,
    {
        let name = name.as_ref();

        // don't replay the same track
        if let Some(ref playing) = self.playing {
            if playing == name {
                return Ok(());
            }
        }

        // TODO: there's probably a better way to do this extension check
        let mut file = if !name.contains('.') {
            // try all supported formats
            self.vfs
                .open(format!("music/{}.flac", name))
                .or_else(|_| self.vfs.open(format!("music/{}.wav", name)))
                .or_else(|_| self.vfs.open(format!("music/{}.mp3", name)))
                .or_else(|_| self.vfs.open(format!("music/{}.ogg", name)))
                .map_err(|_| SoundError::NoSuchTrack(name.to_owned()))?
        } else {
            self.vfs.open(name)?
        };

        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        let source = Decoder::new(Cursor::new(data))?
            .convert_samples::<f32>()
            .buffered()
            .repeat_infinite();

        // stop the old track before starting the new one so there's no overlap
        self.sink = None;
        // TODO handle PlayError
        let new_sink = Sink::try_new(&self.stream).unwrap();
        new_sink.append(source);
        self.sink = Some(new_sink);

        Ok(())
    }

    /// Start playing the track with the given number.
    ///
    /// Note that the first actual music track is track 2; track 1 on the
    /// original Quake CD-ROM held the game data.
    pub fn play_track(&mut self, track_id: usize) -> Result<(), SoundError> {
        self.play_named(format!("track{:02}", track_id))
    }

    /// Stop the current music track.
    ///
    /// This ceases playback entirely. To pause the track, allowing it to be
    /// resumed later, use `MusicPlayer::pause()`.
    ///
    /// If no music track is currently playing, this has no effect.
    pub fn stop(&mut self) {
        self.sink = None;
        self.playing = None;
    }

    /// Pause the current music track.
    ///
    /// If no music track is currently playing, or if the current track is
    /// already paused, this has no effect.
    pub fn pause(&mut self) {
        if let Some(ref mut sink) = self.sink {
            sink.pause();
        }
    }

    /// Resume playback of the current music track.
    ///
    /// If no music track is currently playing, or if the current track is not
    /// paused, this has no effect.
    pub fn resume(&mut self) {
        if let Some(ref mut sink) = self.sink {
            sink.play();
        }
    }
}
