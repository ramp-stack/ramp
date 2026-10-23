use maverick_os::shared as native;
use serde::{Serialize, Serializer, Deserialize, Deserializer, de::DeserializeOwned};

use std::any::TypeId;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};

pub struct Snapshot<'a, S>(pub native::Ref<'a, S>, pub fn(&S) -> &(dyn std::any::Any + Send + Sync));
impl<S> Debug for Snapshot<'_, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(std::any::type_name::<S>())
    }
}
impl<S> prism::traits::DynRef for Snapshot<'_, S> {
    fn value(&self) -> &(dyn std::any::Any + Send + Sync) {(self.1)(&self.0)}
}

pub trait Sharable: Clone + Debug + Send + Sync + 'static {
    type Init: Send + 'static;
    type Request: Send + 'static;
    type Response: Clone + Send + Sync + 'static;

    fn name() -> &'static str {std::any::type_name::<Self>()}
    fn init(init: Self::Init) -> impl Future<Output = Self> + Send;
    fn handle(&mut self, request: Self::Request) -> impl Future<Output = Self::Response> + Send;
}

#[derive(Clone, Debug)]
pub struct Shared<S: Sharable>(native::Shared<State<S>>);
impl<S: Sharable<Init: Hash> + Serialize + DeserializeOwned> Shared<S> {
    pub fn new(init: S::Init) -> Self {
        Self::restore(init, |_, bytes| postcard::from_bytes(bytes).expect("Invalid cached state"))
    }
}
impl<S: Sharable<Init: Hash> + Serialize> Shared<S> {
    pub fn restore(init: S::Init, restore: fn(&S::Init, &[u8]) -> S) -> Self {
        let init = Init::<S> {
            key: native::Key::new::<S>(&init),
            value: init, serialize: Some(|value| value), restore: Some(restore),
        };
        Self(native::Shared::new(init))
    }
}
impl<S: Sharable<Init: Hash>> Shared<S> {
    pub fn local(value: S::Init) -> Self {
        let init = Init::<S> {
            key: native::Key::new::<S>(&value), value, serialize: None, restore: None,
        };
        Self(native::Shared::new(init))
    }
}
impl<S: Sharable> Shared<S> {
    pub fn temporary(value: S::Init) -> Self {
        let init = Init::<S> {key: native::Key(TypeId::of::<S>(), 0), value, serialize: None, restore: None};
        Self(native::Shared::temporary(init))
    }
    pub fn update(&self, request: S::Request) -> S::Response {self.0.update(request)}
    pub fn load(&self) -> native::Ref<'_, S> {self.0.load().map(|state| &state.0)}
    pub fn try_next(&mut self) -> Option<S::Response> {self.0.try_next()}
    pub async fn next(&mut self) -> S::Response {self.0.next().await}
}

struct Init<S: Sharable> {
    key: native::Key,
    value: S::Init,
    serialize: Option<fn(&S) -> &dyn erased_serde::Serialize>,
    restore: Option<fn(&S::Init, &[u8]) -> S>,
}
impl<S: Sharable> Hash for Init<S> {
    fn hash<H: Hasher>(&self, hash: &mut H) {self.key.hash(hash)}
}

#[derive(Clone, Debug)]
struct State<S: Sharable>(S, Option<fn(&S) -> &dyn erased_serde::Serialize>);
impl<S: Sharable> Serialize for State<S> {
    fn serialize<T: Serializer>(&self, serializer: T) -> Result<T::Ok, T::Error> {
        let serialize = self.1.expect("Temporary state cannot be cached");
        erased_serde::serialize(serialize(&self.0), serializer)
    }
}
impl<'de, S: Sharable> Deserialize<'de> for State<S> {
    fn deserialize<D: Deserializer<'de>>(_deserializer: D) -> Result<Self, D::Error> {
        Err(serde::de::Error::custom("Shared state requires its initialization codec"))
    }
}
impl<S: Sharable> native::Sharable for State<S> {
    type Init = Init<S>;
    type Request = S::Request;
    type Response = S::Response;

    fn key(init: &Init<S>) -> native::Key {init.key}
    fn cache_key(init: &Init<S>) -> Option<String> {
        init.restore.map(|_| format!("{}:{:016x}", S::name(), init.key.1))
    }
    fn restore(init: &Init<S>, bytes: &[u8]) -> Self {
        Self(init.restore.unwrap()(&init.value, bytes), init.serialize)
    }
    async fn init(init: Init<S>) -> Self {Self(S::init(init.value).await, init.serialize)}
    async fn handle(&mut self, request: S::Request) -> S::Response {self.0.handle(request).await}
}
