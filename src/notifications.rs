pub use air_notifications::*;
use maverick_os::{Hardware, Notify, PushToken};
use maverick_os::shared::{Service, Services};
use prism::traits::DynInstance;

use std::hash::{Hash, Hasher};

use crate::air::{Air, Instance};

pub struct Push {
    instance: Instance,
    destination: maverick_os::PushDestination,
}
impl Push {
    pub fn start(air: &Air, hardware: &Hardware) -> Option<Instance> {
        let application = option_env!("RAMP_NOTIFICATION_APPLICATION")?;
        let instance = Instance::create::<Notifications>(
            air.clone(), (hardware.device_id().to_owned(), application.parse().unwrap()),
        );
        instance.share(Notifications::server());
        Services::start::<Self>(Self {
            instance: instance.clone(), destination: hardware.push_destination(),
        });
        Some(instance)
    }
    pub fn notify(instance: Option<&Instance>, notify: &Notify, action: Action) {
        match (action, instance) {
            (Action::Local(header, body), _) => notify.notify(header, body),
            (Action::Remote(message), Some(instance)) => {
                if instance.pending().value().downcast_ref::<Notifications>().unwrap().changes(&message) {
                    instance.clone().send(Box::new(message));
                }
            }
            (_, None) => {}
        }
    }
}
impl Hash for Push {
    fn hash<H: Hasher>(&self, hasher: &mut H) {self.instance.id().hash(hasher)}
}
impl Service for Push {
    type Init = Self;
    type Predicate = PushToken;

    async fn init(init: Self) -> Self {init}
    async fn predicate(&mut self) -> PushToken {self.destination.next().await}
    async fn handle(&mut self, token: PushToken) {
        let destination = match token {
            PushToken::Apns(token) => PushDestination::Apns(token),
            PushToken::Fcm(token) => PushDestination::Fcm(token),
        };
        self.instance.send(Box::new(Notification::Destination(destination)));
    }
}
