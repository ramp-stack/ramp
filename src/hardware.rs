use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use prism::hardware::{Cancelled, Data, Image, Media, PickResult};
use prism::traits;

#[derive(Clone)]
pub struct Hardware(pub maverick_os::Hardware);
impl traits::Hardware for Hardware {
    type Camera = Camera;
    type Video = Video;

    fn start_camera(&self) -> Self::Camera {Camera(self.0.start_camera())}
    fn video(&self, bytes: Vec<u8>) -> Option<Self::Video> {maverick_os::Video::new(bytes).map(Video::from)}
    fn share(&self, data: &Data) {self.0.share(&Self::to_maverick(data))}
    fn set_clipboard(&self, data: &Data) {self.0.set_clipboard(&Self::to_maverick(data))}
    fn get_clipboard(&self) -> Option<Data> {self.0.get_clipboard().map(Self::from_maverick)}
    fn set_cache(&self, key: &str, data: &Data) {self.0.set_cache(key, &Self::to_maverick(data))}
    fn get_cache(&self, key: &str) -> Option<Data> {self.0.get_cache(key).map(Self::from_maverick)}
    fn trigger_haptic(&self) {self.0.trigger_haptic()}
}
impl Hardware {
    fn from_maverick(data: maverick_os::Data) -> Data {
        match data {
            maverick_os::Data::Text(text) => Data::Text(text),
            maverick_os::Data::Image(image) => Data::Image(image),
            maverick_os::Data::Video(video) => Data::Video(prism::hardware::Video::new(Video::from(video))),
            maverick_os::Data::File(path) => Data::File(path),
        }
    }
    fn to_maverick(data: &Data) -> maverick_os::Data {
        match data {
            Data::Text(text) => maverick_os::Data::Text(text.clone()),
            Data::Image(image) => maverick_os::Data::Image(image.clone()),
            Data::Video(video) => {
                let video = maverick_os::Video::new(video.bytes().as_ref().clone())
                    .expect("A validated video must remain readable");
                maverick_os::Data::Video(video)
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
picker!(maverick_os::Image, Image, std::convert::identity);
picker!(maverick_os::Video, prism::hardware::Video, |video| {
    prism::hardware::Video::new(Video::from(video))
});
picker!(maverick_os::Media, Media, |media| match media {
    maverick_os::Media::Image(image) => Media::Image(image),
    maverick_os::Media::Video(video) => Media::Video(prism::hardware::Video::new(Video::from(video))),
});
picker!(PathBuf, PathBuf, std::convert::identity);

#[derive(Clone)]
pub struct Camera(maverick_os::Camera);
impl traits::Camera for Camera {
    fn frame(&self) -> Image {self.0.frame()}
}

pub struct Picked<N: maverick_os::Pickable, T>(maverick_os::Picked<N>, fn(N) -> T);
impl<N: maverick_os::Pickable + 'static, T: Clone + 'static> traits::Picked<T> for Picked<N, T> {
    async fn get(self) -> PickResult<T> {self.0.get().await.map(self.1).map_err(|_| Cancelled)}
    fn try_get(&mut self) -> Option<PickResult<T>> {
        self.0.try_get().map(|result| result.map(self.1).map_err(|_| Cancelled))
    }
}

#[derive(Clone, Debug)]
pub struct Video(maverick_os::Video);
impl From<maverick_os::Video> for Video {
    fn from(video: maverick_os::Video) -> Self {Self(video)}
}
impl traits::Video for Video {
    fn bytes(&self) -> &Arc<Vec<u8>> {self.0.bytes()}
    fn duration(&self) -> Option<Duration> {self.0.duration()}
    fn dimensions(&self) -> (u32, u32) {self.0.dimensions()}
    fn thumbnail(&self) -> Image {self.0.thumbnail()}
    fn format(&self) -> &str {self.0.format()}
    fn codec(&self) -> &str {self.0.codec()}
    fn player(&self) -> Result<Box<dyn traits::Player>, String> {
        maverick_os::Player::new(&self.0).map(|player| Box::new(Player(player)) as _)
    }
}

#[derive(Clone, Debug)]
pub struct Player(maverick_os::Player);
impl traits::Player for Player {
    fn start(&self) {self.0.start()}
    fn stop(&self) {self.0.stop()}
    fn set_speed(&self, speed: f64) {self.0.set_speed(speed)}
    fn speed(&self) -> f64 {self.0.speed()}
    fn seek(&self, position: Duration) {self.0.seek(position)}
    fn position(&self) -> Duration {self.0.position()}
    fn duration(&self) -> Option<Duration> {self.0.duration()}
    fn is_playing(&self) -> bool {self.0.is_playing()}
    fn frame(&self) -> Image {self.0.frame()}
}
