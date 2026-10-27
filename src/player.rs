use maverick_os::{Playback, Speaker};
use maverick_os::shared::{Key, Services};
use media::PlaybackMode;
use tokio::sync::{mpsc, oneshot};

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::shared::{Sharable, Shared};

/// Clones share one player, with an independent frame subscription for each handle.
#[derive(Clone, Debug)]
pub struct Player {
    frames: Shared<Frame>,
    session: Arc<Session>,
    requests: mpsc::UnboundedSender<(Request, oneshot::Sender<()>)>,
}
impl Player {
    pub fn new(mut player: media::Player) -> Result<Self, String> {
        let speaker = if player.has_audio() {Some(Speaker::new()?)} else {None};
        player.synchronize_audio();
        let frame = player.frame().clone();
        let playing = frame.playing;
        let frame = Frame {frame, has_video: player.has_video(), mode: player.mode(), error: None};
        let frames = Shared::temporary(frame);
        let id = uuid::Uuid::new_v4().as_u128();
        let (requests, incoming) = mpsc::unbounded_channel();
        let service = PlayerService {
            id, revision: player.audio_revision(), player, state: frames.clone(),
            requests: incoming, speaker, error: None, progress: Instant::now(),
        };
        Playback::update(id, playing);
        Services::spawn(Key::new::<Player>(&id), service.run());
        let session = Arc::new(Session {id, state: frames.clone()});
        Ok(Self {frames, session, requests})
    }
    pub fn set_mode(&self, mode: PlaybackMode) {self.request(Request::Mode(mode));}
    pub fn mode(&self) -> PlaybackMode {self.session.state.load().mode}
    pub fn has_video(&self) -> bool {self.session.state.load().has_video}
    pub fn start(&self) {self.request(Request::Start);}
    pub fn stop(&self) {self.request(Request::Stop);}
    pub fn seek(&self, position: Duration) {self.request(Request::Seek(position));}
    pub fn set_speed(&self, speed: f64) {
        assert!(speed.is_finite() && speed > 0.0);
        self.request(Request::Speed(speed));
    }
    pub fn speed(&self) -> f64 {self.session.state.load().frame.speed}
    pub fn set_volume(&self, volume: f64) {
        assert!(volume.is_finite() && volume >= 0.0);
        self.request(Request::Volume(volume));
    }
    pub fn volume(&self) -> f64 {self.session.state.load().frame.volume}
    pub fn position(&self) -> Duration {self.session.state.load().frame.position}
    pub fn duration(&self) -> Option<Duration> {self.session.state.load().frame.duration}
    pub fn is_playing(&self) -> bool {self.session.state.load().frame.playing}
    pub fn error(&self) -> Option<String> {self.session.state.load().error.clone()}
    pub fn frame(&self) -> media::Frame {self.frames.load().frame.clone()}
    pub async fn next(&mut self) -> media::Frame {self.frames.next().await.frame}

    fn request(&self, request: Request) {
        let (reply, response) = oneshot::channel();
        self.requests.send((request, reply)).expect("Player service stopped");
        let wait = || futures::executor::block_on(response).expect("Player command was not handled");
        let threaded = tokio::runtime::Handle::try_current().is_ok_and(|runtime| {
            runtime.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread
        });
        if threaded {tokio::task::block_in_place(wait)} else {wait()}
    }
}
impl prism::traits::Player for Player {
    fn has_video(&self) -> bool {self.has_video()}
    fn set_mode(&self, mode: PlaybackMode) {self.set_mode(mode)}
    fn mode(&self) -> PlaybackMode {self.mode()}
    fn start(&self) {self.start()}
    fn stop(&self) {self.stop()}
    fn set_speed(&self, speed: f64) {self.set_speed(speed)}
    fn speed(&self) -> f64 {self.speed()}
    fn set_volume(&self, volume: f64) {self.set_volume(volume)}
    fn volume(&self) -> f64 {self.volume()}
    fn seek(&self, position: Duration) {self.seek(position)}
    fn position(&self) -> Duration {self.position()}
    fn duration(&self) -> Option<Duration> {self.duration()}
    fn is_playing(&self) -> bool {self.is_playing()}
    fn error(&self) -> Option<String> {self.error()}
    fn frame(&self) -> media::Frame {self.frame()}
    fn next(&mut self) -> prism::traits::BoxFuture<'_, media::Frame> {Box::pin(self.next())}
}

enum Request {Start, Stop, Speed(f64), Volume(f64), Seek(Duration), Mode(PlaybackMode)}

#[derive(Clone, Debug)]
struct Frame {
    frame: media::Frame,
    has_video: bool,
    mode: PlaybackMode,
    error: Option<String>,
}
impl Sharable for Frame {
    type Init = Self;
    type Request = Self;
    type Response = Self;

    fn capacity() -> usize {8}
    async fn init(frame: Self) -> Self {frame}
    async fn handle(&mut self, frame: Self) -> Self {*self = frame; self.clone()}
}

#[derive(Debug)]
struct Session {
    id: u128,
    state: Shared<Frame>,
}
impl Drop for Session {
    fn drop(&mut self) {Services::stop(Key::new::<Player>(&self.id));}
}

struct PlayerService {
    id: u128,
    player: media::Player,
    state: Shared<Frame>,
    requests: mpsc::UnboundedReceiver<(Request, oneshot::Sender<()>)>,
    speaker: Option<Speaker>,
    revision: u64,
    error: Option<String>,
    progress: Instant,
}
impl PlayerService {
    async fn run(mut self) {
        loop {
            let command = tokio::select! {
                command = self.requests.recv() => {
                    let Some(command) = command else {return};
                    Some(command)
                },
                _ = tokio::time::sleep(Duration::from_millis(5)) => None,
            };
            tokio::task::block_in_place(|| {
                if let Some((request, reply)) = command {
                    self.apply(request);
                    self.update(true);
                    let _ = reply.send(());
                } else {self.update(false);}
            });
        }
    }
    fn apply(&mut self, request: Request) {
        match request {
            Request::Start => {self.error = None; self.player.start();}
            Request::Stop => self.player.stop(),
            Request::Speed(speed) => self.player.set_speed(speed),
            Request::Volume(volume) => self.player.set_volume(volume),
            Request::Seek(position) => self.player.seek(position),
            Request::Mode(mode) => self.player.set_mode(mode),
        }
    }
    fn update(&mut self, force: bool) {
        let previous = self.state.load();
        self.player.frame();
        if !self.player.has_audio() {self.speaker = None;}
        else if self.speaker.is_none() && self.error.is_none() {
            match Speaker::new() {
                Ok(speaker) => {
                    self.speaker = Some(speaker);
                    self.player.synchronize_audio();
                }
                Err(error) => {self.error = Some(error); self.player.stop();}
            }
        }
        if let Some(speaker) = &mut self.speaker {
            let output = (|| {
                let playing = self.player.frame().playing;
                let (mut duration, underrun) = speaker.request()?;
                if self.revision != self.player.audio_revision() || (playing && underrun) {
                    speaker.clear();
                    self.player.synchronize_audio();
                    duration = speaker.request()?.0;
                }
                if playing && !duration.is_zero() {
                    let (rate, channels) = speaker.format();
                    loop {
                        let revision = self.player.audio_revision();
                        let audio = self.player.audio(duration, rate, channels)?;
                        if revision == self.player.audio_revision() {
                            speaker.write(audio.sound);
                            break
                        }
                        speaker.clear();
                        self.player.synchronize_audio();
                        duration = speaker.request()?.0;
                    }
                }
                self.revision = self.player.audio_revision();
                speaker.set_playing(self.player.frame().playing)
            })();
            if let Err(error) = output {
                self.error = Some(error);
                self.player.stop();
                self.speaker = None;
            }
        }
        let frame = self.player.frame().clone();
        let mode = self.player.mode();
        let has_video = self.player.has_video();
        let changed = match (&frame.video, &previous.frame.video) {
            (Some(frame), Some(previous)) => !Arc::ptr_eq(frame, previous),
            (None, None) => false,
            _ => true,
        } || frame.duration != previous.frame.duration || mode != previous.mode
            || frame.speed != previous.frame.speed || frame.volume != previous.frame.volume
            || frame.playing != previous.frame.playing || self.error != previous.error
            || has_video != previous.has_video;
        let progress = frame.position != previous.frame.position
            && (self.progress.elapsed() >= Duration::from_millis(100) || !frame.playing);
        drop(previous);
        Playback::update(self.id, frame.playing);
        if force || changed || progress {
            self.state.update(Frame {frame, has_video, mode, error: self.error.clone()});
            self.progress = Instant::now();
        }
    }
}
impl Drop for PlayerService {
    fn drop(&mut self) {Playback::remove(self.id);}
}
