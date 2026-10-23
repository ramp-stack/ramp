use prism::hardware::{
    Audio, Cancelled, Chunks, Data, Format, Frame, ImageFormat, PickResult, PlaybackMode, Tracks,
    Video, Visual,
};
use prism::traits;

use std::io::{Read, Seek, Write};
use std::path::PathBuf;
use std::ops::Bound;
use std::time::Duration;

prism::events::inventory::submit! {prism::Prism::content_backend::<Image, Media>()}

#[derive(Clone)]
pub struct Hardware(pub maverick_os::Hardware);
impl traits::Hardware for Hardware {
    fn open_keyboard(&self) -> prism::hardware::Keyboard {
        prism::hardware::Keyboard::new(Keyboard {_session: self.0.open_keyboard()})
    }
    fn start_camera(&self) -> Result<prism::hardware::Camera, String> {
        let camera = self.0.start_camera()?;
        let media = prism::hardware::Media::from(Media(camera.as_ref().clone()));
        Ok(prism::hardware::Camera::new(media, Camera(camera)))
    }
    fn start_microphone(&self) -> Result<prism::hardware::Microphone, String> {
        let microphone = self.0.start_microphone()?;
        let media = prism::hardware::Media::from(Media(microphone.as_ref().clone()));
        Ok(prism::hardware::Microphone::new(media, Microphone(microphone)))
    }
    fn start_capture(&self) -> Result<prism::hardware::Capture, String> {
        let capture = self.0.start_capture()?;
        let media = prism::hardware::Media::from(Media(capture.as_ref().clone()));
        Ok(prism::hardware::Capture::new(media, Capture(capture)))
    }
    fn share(&self, data: &Data) {self.0.share(&Self::to_maverick(data))}
    fn set_clipboard(&self, data: &Data) {self.0.set_clipboard(&Self::to_maverick(data))}
    fn get_clipboard(&self) -> Option<Data> {self.0.get_clipboard().map(Self::from_maverick)}
    fn set_cache(&self, key: &str, data: &Data) -> Result<(), String> {
        self.0.set_cache(key, &Self::to_maverick(data))
    }
    fn get_cache(&self, key: &str) -> Result<Option<Data>, String> {
        self.0.get_cache(key).map(|data| data.map(Self::from_maverick))
    }
    fn trigger_haptic(&self) {self.0.trigger_haptic()}
}
impl Hardware {
    fn from_maverick(data: maverick_os::Data) -> Data {
        match data {
            maverick_os::Data::Text(text) => Data::Text(text),
            maverick_os::Data::Image(image) => Data::Image(Image::wrap(image)),
            maverick_os::Data::Media(media) => Data::Media(prism::hardware::Media::from(Media(media))),
            maverick_os::Data::File(path) => Data::File(path),
        }
    }
    fn to_maverick(data: &Data) -> maverick_os::Data {
        match data {
            Data::Text(text) => maverick_os::Data::Text(text.clone()),
            Data::Image(image) => maverick_os::Data::Image(Image::native(image)),
            Data::Media(media) => {
                let media = media.downcast_ref::<Media>().expect("Media uses another backend");
                maverick_os::Data::Media(media.0.clone())
            }
            Data::File(path) => maverick_os::Data::File(path.clone()),
        }
    }
}

macro_rules! picker {
    ($native:ty, $item:ty, $convert:expr) => {
        impl traits::Picker<$item> for Hardware {
            type Picked = Picked<$native, $item>;
            fn pick(&self) -> Self::Picked {Picked(self.0.pick(), $convert)}
        }
        impl traits::Picker<Vec<$item>> for Hardware {
            type Picked = Picked<Vec<$native>, Vec<$item>>;
            fn pick(&self) -> Self::Picked {
                Picked(self.0.pick(), |items| items.into_iter().map($convert).collect())
            }
        }
    };
}
picker!(maverick_os::Data, Data, Hardware::from_maverick);
picker!(maverick_os::Image, prism::hardware::Image, Image::wrap);
picker!(maverick_os::Media, prism::hardware::Media, |media| prism::hardware::Media::from(Media(media)));
picker!(maverick_os::Visual, Visual, |visual| match visual {
    maverick_os::Visual::Image(image) => Visual::Image(Image::wrap(image)),
    maverick_os::Visual::Video(video) => Visual::Video(Video(prism::hardware::Media::from(Media(video.0)))),
});
picker!(maverick_os::Video, Video, |video| Video(prism::hardware::Media::from(Media(video.0))));
picker!(maverick_os::Audio, Audio, |audio| Audio(prism::hardware::Media::from(Media(audio.0))));
picker!(PathBuf, PathBuf, std::convert::identity);

#[derive(Clone)]
pub struct Camera(maverick_os::Camera);
impl traits::Capture for Camera {
    fn error(&self) -> Option<String> {self.0.error()}
}

#[derive(Clone)]
pub struct Microphone(maverick_os::Microphone);
impl traits::Capture for Microphone {
    fn error(&self) -> Option<String> {self.0.error()}
}

#[derive(Clone)]
pub struct Capture(maverick_os::Capture);
impl traits::Capture for Capture {
    fn error(&self) -> Option<String> {self.0.error()}
}

pub struct Picked<N: maverick_os::Pickable, T>(maverick_os::Picked<N>, fn(N) -> T);
impl<N: maverick_os::Pickable + 'static, T: Clone + 'static> traits::Picked<T> for Picked<N, T> {
    async fn get(&mut self) -> PickResult<T> {self.0.get().await.map(self.1).map_err(|_| Cancelled)}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image(maverick_os::Image);
impl Image {
    fn wrap(image: maverick_os::Image) -> prism::hardware::Image {
        prism::hardware::Image::from(Self(image))
    }
    fn native(image: &prism::hardware::Image) -> maverick_os::Image {
        image.downcast_ref::<Self>().expect("Image uses another backend").0.clone()
    }
}
impl traits::Image for Image {
    fn read<R: Read>(reader: R) -> Result<Self, String> {maverick_os::Image::read(reader).map(Self)}
    fn from_rgba(dimensions: (u32, u32), pixels: Vec<u8>) -> Self {
        Self(maverick_os::Image::from_rgba(dimensions, pixels))
    }
    fn dimensions(&self) -> (u32, u32) {self.0.dimensions()}
    fn pixels(&self) -> ((u32, u32), &[u8]) {self.0.pixels()}
    fn write<W: Write>(&self, writer: W, format: ImageFormat) -> Result<(), String> {
        let format = match format {
            ImageFormat::Png => maverick_os::ImageFormat::Png,
            ImageFormat::Jpeg => maverick_os::ImageFormat::Jpeg,
            ImageFormat::Webp => maverick_os::ImageFormat::Webp,
            ImageFormat::Gif => maverick_os::ImageFormat::Gif,
            ImageFormat::Bmp => maverick_os::ImageFormat::Bmp,
            ImageFormat::Ico => maverick_os::ImageFormat::Ico,
            ImageFormat::Tiff => maverick_os::ImageFormat::Tiff,
            ImageFormat::Qoi => maverick_os::ImageFormat::Qoi,
        };
        self.0.write(writer, format)
    }
}
impl PartialEq<prism::hardware::Image> for Image {
    fn eq(&self, other: &prism::hardware::Image) -> bool {
        other.downcast_ref::<Self>().is_some_and(|other| self == other)
    }
}

#[derive(Clone, Debug)]
pub struct Media(maverick_os::Media);
impl From<maverick_os::Media> for Media {
    fn from(media: maverick_os::Media) -> Self {Self(media)}
}
impl TryFrom<prism::hardware::Sound> for Media {
    type Error = String;
    fn try_from(sound: prism::hardware::Sound) -> Result<Self, String> {
        maverick_os::Media::try_from(maverick_os::Sound {
            sample_rate: sound.sample_rate, channels: sound.channels, samples: sound.samples,
        }).map(Self)
    }
}
impl TryFrom<(prism::hardware::Image, Duration)> for Media {
    type Error = String;
    fn try_from((image, duration): (prism::hardware::Image, Duration)) -> Result<Self, String> {
        maverick_os::Media::try_from((Image::native(&image), duration)).map(Self)
    }
}
impl Media {
    fn tracks(tracks: Tracks) -> maverick_os::Tracks {
        match tracks {
            Tracks::Audio => maverick_os::Tracks::Audio,
            Tracks::Video => maverick_os::Tracks::Video,
            Tracks::Both => maverick_os::Tracks::Both,
        }
    }
}
impl traits::Media for Media {
    fn new() -> Self {Self(maverick_os::Media::new())}
    fn read<R: Read + Seek>(reader: R) -> Result<Self, String> {
        maverick_os::Media::read(reader).map(Self)
    }
    fn player(&self) -> Result<Box<dyn traits::Player>, String> {
        maverick_os::Player::new(&self.0).map(|player| Box::new(Player(player)) as _)
    }
    fn copy(&self, range: (Bound<Duration>, Bound<Duration>), tracks: Tracks) -> Result<Box<dyn traits::DynMedia>, String> {
        self.0.copy(range, Self::tracks(tracks)).map(|media| Box::new(Self(media)) as _)
    }
    fn replace(&self, range: (Bound<Duration>, Bound<Duration>), replacement: &prism::hardware::Media, tracks: Tracks) -> Result<(), String> {
        let replacement = replacement.downcast_ref::<Self>().expect("Media uses another backend");
        self.0.replace(range, &replacement.0, Self::tracks(tracks))
    }
    fn write(&self, writer: &mut dyn traits::Writer, format: Format) -> Result<(), String> {
        let format = match format {
            Format::Mp4_Mpeg4p2_Aac => maverick_os::Format::Mp4_Mpeg4p2_Aac,
            Format::Mp4_Mpeg4p2 => maverick_os::Format::Mp4_Mpeg4p2,
            Format::Mkv_Ffv1_Flac => maverick_os::Format::Mkv_Ffv1_Flac,
            Format::Mkv_Ffv1_PcmF32le => maverick_os::Format::Mkv_Ffv1_PcmF32le,
            Format::Mkv_Ffv1 => maverick_os::Format::Mkv_Ffv1,
            Format::Mov_ProRes422_PcmS16le => maverick_os::Format::Mov_ProRes422_PcmS16le,
            Format::Mov_ProRes422 => maverick_os::Format::Mov_ProRes422,
            Format::M4a_Aac => maverick_os::Format::M4a_Aac,
            Format::M4a_Alac => maverick_os::Format::M4a_Alac,
            Format::Wav_PcmS16le => maverick_os::Format::Wav_PcmS16le,
            Format::Wav_PcmF32le => maverick_os::Format::Wav_PcmF32le,
            Format::Flac_Flac => maverick_os::Format::Flac_Flac,
        };
        self.0.write(writer, format)
    }
    fn append(&self, other: &prism::hardware::Media) {
        let other = other.downcast_ref::<Self>().expect("Media uses another backend");
        self.0.append(&other.0)
    }
    fn chunks(&self, duration: Duration) -> Result<Chunks, String> {
        let chunks = self.0.chunks(duration)?;
        Ok(Chunks::new(chunks.map(|chunk| prism::hardware::Media::from(Self(chunk)))))
    }
    fn duration(&self) -> Duration {self.0.duration()}
    fn dimensions(&self) -> Option<(u32, u32)> {self.0.dimensions()}
    fn thumbnail(&self) -> Option<prism::hardware::Image> {self.0.thumbnail().map(Image::wrap)}
}

#[derive(Clone, Debug)]
pub struct Player(maverick_os::Player);
impl Player {
    fn from_mode(mode: maverick_os::PlaybackMode) -> PlaybackMode {
        match mode {
            maverick_os::PlaybackMode::Default => PlaybackMode::Default,
            maverick_os::PlaybackMode::Live(delay) => PlaybackMode::Live(delay),
            maverick_os::PlaybackMode::Loop => PlaybackMode::Loop,
        }
    }
    fn from_frame(frame: maverick_os::Frame) -> Frame {
        Frame {
            image: frame.image.map(Image::wrap), position: frame.position, duration: frame.duration,
            mode: Self::from_mode(frame.mode), speed: frame.speed, volume: frame.volume,
            playing: frame.playing, error: frame.error,
        }
    }
}
impl traits::Player for Player {
    fn set_mode(&self, mode: PlaybackMode) {
        self.0.set_mode(match mode {
            PlaybackMode::Default => maverick_os::PlaybackMode::Default,
            PlaybackMode::Live(delay) => maverick_os::PlaybackMode::Live(delay),
            PlaybackMode::Loop => maverick_os::PlaybackMode::Loop,
        });
    }
    fn mode(&self) -> PlaybackMode {Self::from_mode(self.0.mode())}
    fn start(&self) {self.0.start()}
    fn stop(&self) {self.0.stop()}
    fn set_speed(&self, speed: f64) {self.0.set_speed(speed)}
    fn speed(&self) -> f64 {self.0.speed()}
    fn set_volume(&self, volume: f64) {self.0.set_volume(volume)}
    fn volume(&self) -> f64 {self.0.volume()}
    fn seek(&self, position: Duration) {self.0.seek(position)}
    fn position(&self) -> Duration {self.0.position()}
    fn duration(&self) -> Duration {self.0.duration()}
    fn is_playing(&self) -> bool {self.0.is_playing()}
    fn error(&self) -> Option<String> {self.0.error()}
    fn frame(&self) -> Frame {Self::from_frame(self.0.frame())}
    fn next(&mut self) -> traits::BoxFuture<'_, Frame> {
        Box::pin(async move {Self::from_frame(self.0.next().await)})
    }
}

#[derive(Clone)]
struct Keyboard {
    _session: maverick_os::Keyboard,
}
impl traits::Keyboard for Keyboard {}
