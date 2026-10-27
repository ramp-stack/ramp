use futures::future::LocalBoxFuture;
use media::{Audio, AudioFrame, Image, Media, MediaFrame, Video, VideoFrame};
use prism::hardware::{Cancelled, Data, PickResult, Visual};
use prism::traits;

use std::path::PathBuf;

use crate::Player;

#[derive(Clone)]
pub struct Hardware(pub maverick_os::Hardware);
impl traits::Hardware for Hardware {
    fn keyboard(&self) -> prism::hardware::Keyboard {
        prism::hardware::Keyboard::new(Keyboard(self.0.keyboard()))
    }
    fn player(&self, player: media::Player) -> Result<prism::hardware::Player, String> {
        Player::new(player).map(prism::hardware::Player::new)
    }
    fn start_camera(&self) -> Result<prism::hardware::Camera, String> {
        self.0.start_camera().map(|camera| prism::hardware::Camera::new(Camera(camera)))
    }
    fn start_microphone(&self) -> Result<prism::hardware::Microphone, String> {
        self.0.start_microphone()
            .map(|microphone| prism::hardware::Microphone::new(Microphone(microphone)))
    }
    fn start_capture(&self) -> Result<prism::hardware::Capture, String> {
        self.0.start_capture().map(|capture| prism::hardware::Capture::new(Capture(capture)))
    }
    fn share(&self, data: &Data) {self.0.share(&Self::to_maverick(data))}
    fn set_clipboard(&self, data: &Data) {self.0.set_clipboard(&Self::to_maverick(data))}
    fn get_clipboard(&self) -> Option<Data> {self.0.get_clipboard().map(Self::from_maverick)}
    fn set_cache(&self, key: &str, data: &Data) -> Result<(), String> {
        self.0.set_cache(key, &Self::to_maverick(data))
    }
    fn get_cache(&self, key: &str) -> Option<Data> {self.0.get_cache(key).map(Self::from_maverick)}
    fn trigger_haptic(&self) {self.0.trigger_haptic()}
}
impl Hardware {
    fn from_maverick(data: maverick_os::Data) -> Data {
        match data {
            maverick_os::Data::Text(text) => Data::Text(text),
            maverick_os::Data::Image(image) => Data::Image(image),
            maverick_os::Data::Media(media) => Data::Media(media),
            maverick_os::Data::File(path) => Data::File(path),
        }
    }
    fn to_maverick(data: &Data) -> maverick_os::Data {
        match data {
            Data::Text(text) => maverick_os::Data::Text(text.clone()),
            Data::Image(image) => maverick_os::Data::Image(image.clone()),
            Data::Media(media) => maverick_os::Data::Media(media.clone()),
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
picker!(Image, Image, std::convert::identity);
picker!(Media, Media, std::convert::identity);
picker!(maverick_os::Visual, Visual, |visual| match visual {
    maverick_os::Visual::Image(image) => Visual::Image(image),
    maverick_os::Visual::Video(video) => Visual::Video(video),
});
picker!(Video, Video, std::convert::identity);
picker!(Audio, Audio, std::convert::identity);
picker!(PathBuf, PathBuf, std::convert::identity);

#[derive(Clone)]
pub struct Camera(maverick_os::Camera);
impl traits::Capture<VideoFrame> for Camera {
    fn next(&mut self) -> LocalBoxFuture<'_, Option<VideoFrame>> {Box::pin(self.0.next())}
    fn error(&self) -> Option<String> {self.0.error()}
}

#[derive(Clone)]
pub struct Microphone(maverick_os::Microphone);
impl traits::Capture<AudioFrame> for Microphone {
    fn next(&mut self) -> LocalBoxFuture<'_, Option<AudioFrame>> {Box::pin(self.0.next())}
    fn error(&self) -> Option<String> {self.0.error()}
}

#[derive(Clone)]
pub struct Capture(maverick_os::Capture);
impl traits::Capture<MediaFrame> for Capture {
    fn next(&mut self) -> LocalBoxFuture<'_, Option<MediaFrame>> {Box::pin(self.0.next())}
    fn error(&self) -> Option<String> {self.0.error()}
}

pub struct Picked<N: maverick_os::Pickable, T>(maverick_os::Picked<N>, fn(N) -> T);
impl<N: maverick_os::Pickable + 'static, T: Clone + 'static> traits::Picked<T> for Picked<N, T> {
    async fn get(&mut self) -> PickResult<T> {self.0.get().await.map(self.1).map_err(|_| Cancelled)}
}

#[derive(Clone)]
struct Keyboard(maverick_os::Keyboard);
impl traits::Keyboard for Keyboard {
    fn show(&self, dismiss_padding: f32) {self.0.show(dismiss_padding);}
    fn hide(&self) {self.0.hide();}
    fn visible(&self) -> bool {self.0.visible()}
}
