use air::{
    DefaultResolver, Id, Identity, Inbox, Location, Name, Output, Request, RequestType, Resolver,
    Response, Secret,
};
use air_server::Client;
use crossfire::{AsyncRx, spsc};
use futures::{StreamExt, stream::FuturesUnordered};
use maverick_os::{Hardware, Notify, PushDestination};
use maverick_os::shared::{Service, Services};
use prism::air::{ContractType, Instance as PrismInstance, Request as Message, Value};
use prism::service::{ServiceInit, Key};
use prism::traits::{self, ContractUpdate, DynInstance, DynInstances, DynPending, Update};
use serde::{Serialize, Serializer, Deserializer};
use serde::de::{DeserializeSeed, SeqAccess, Visitor, Error as DeError};
use tokio::sync::{Mutex, OnceCell};

use std::collections::{HashMap, VecDeque};
use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;

use crate::notifications::{Notification as PushMessage, Notifications};
use crate::shared::{Sharable, Shared, Snapshot};

#[derive(Clone)]
pub struct Air {
    secret: Secret,
    network: Arc<Network>,
    notifications: Option<PrismInstance<Notifications>>,
    notify: Notify,
}
impl Air {
    pub fn new(secret: Secret, hardware: &Hardware) -> Self {
        let mut air = Self {
            secret, network: Arc::new(Network::default()), notifications: None,
            notify: hardware.notifier(),
        };
        Services::start::<InboxService>(air.clone());
        if let Some(application) = option_env!("RAMP_NOTIFICATION_APPLICATION") {
            let context = prism::air::Air::new(air.clone());
            let instance = context.instances::<Notifications>()
                .create((hardware.device_id().to_owned(), application.parse().unwrap()));
            instance.share(Notifications::server());
            Services::start::<Push>(Push {
                instance: instance.clone(), destination: Box::pin(hardware.push_destination()),
            });
            air.notifications = Some(instance);
        }
        air
    }
}
impl Debug for Air {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_tuple("Air").field(&self.secret.name()).finish()
    }
}
impl Air {
    fn notify(&self, notification: Notification) {
        match (notification, self.notifications.clone()) {
            (Notification::Local, _) => {
                self.notify.notify("New activity", "You have a notification.");
            }
            (Notification::Watch(id, key, server), Some(mut instance)) => {
                if instance.pending().files.get(&id) != Some(&(key, server)) {
                    instance.send(PushMessage::Watch(id, key, server));
                }
            }
            (Notification::Received(id, key, server), Some(mut instance)) => {
                if instance.pending().files.get(&id) == Some(&(key, server)) {
                    instance.send(PushMessage::Received(id, key, server));
                }
            }
            (Notification::Remove(id), Some(mut instance)) => {
                if instance.pending().files.contains_key(&id) {instance.send(PushMessage::Remove(id));}
            }
            (_, None) => {}
        }
    }
    async fn resolve(&self, name: Name, timestamp: Option<u64>) -> Arc<Identity> {
        self.network.resolver.lock().await.resolve(name, timestamp).await
    }
    async fn send(&self, server: Name, request: Request) -> Responses {
        Responses(self.network.client(server).await.send(request).await)
    }
}

impl traits::Air for Air {
    fn me(&self) -> Name {self.secret.name()}
    fn instances(&self, contract: ContractType) -> Box<dyn traits::DynInstances> {
        Box::new(Instances::new(self.clone(), contract))
    }
    fn start(&self, init: ServiceInit) {
        Services::spawn(maverick_os::shared::Key(init.key.0, init.key.1), async move {
            let mut service = init.service.await;
            loop {service.run().await; tokio::task::yield_now().await;}
        });
    }
    fn shutdown(&self, key: Key) {Services::stop(maverick_os::shared::Key(key.0, key.1))}
}
impl traits::Resolver for Air {
    fn resolve(&self, name: Name, timestamp: Option<u64>) -> traits::BoxFuture<'_, Arc<Identity>> {
        Box::pin(Air::resolve(self, name, timestamp))
    }
}
impl Resolver for Air {
    async fn resolve(&mut self, name: Name, timestamp: Option<u64>) -> Arc<Identity> {
        Air::resolve(self, name, timestamp).await
    }
}
impl Hash for Air {
    fn hash<H: Hasher>(&self, hash: &mut H) {self.secret.hash(hash)}
}

pub struct Responses(AsyncRx<spsc::List<Response>>);
impl Responses {
    async fn next(&mut self) -> Response {
        self.0.recv().await.expect("Air closed a response stream")
    }
}

#[derive(Clone, Debug)]
enum Notification {
    Local, Watch(Id, air::PublicKey, Name), Received(Id, air::PublicKey, Name), Remove(Id),
}

#[derive(Clone, Debug)]
struct Instances(Shared<InstancesState>);
impl Instances {
    fn new(air: Air, contract: ContractType) -> Self {
        let init = InstancesInit(air, contract);
        let state = Shared::local(init.clone());
        Services::start::<InstancesService>(init);
        Self(state)
    }
}
impl DynInstances for Instances {
    fn create(&self, init: Message) -> Box<dyn DynInstance> {
        Box::new(self.0.update(InstancesRequest::Create(init)))
    }
    fn list(&self) -> HashMap<Id, Box<dyn DynInstance>> {
        self.0.load().0.iter().map(|(id, instance)| {
            (*id, Box::new(instance.clone()) as Box<dyn DynInstance>)
        }).collect()
    }
    fn try_next(&mut self) -> Option<Box<dyn DynInstance>> {
        self.0.try_next().map(|instance| Box::new(instance) as Box<dyn DynInstance>)
    }
    fn next(&mut self) -> traits::BoxFuture<'_, Box<dyn DynInstance>> {
        Box::pin(async {Box::new(self.0.next().await) as Box<dyn DynInstance>})
    }
}

#[derive(Clone, Debug)]
struct Instance(Shared<InstanceState>, Shared<InboxState>, Air);
impl Instance {
    fn open(init: InstanceInit) -> Self {
        let inbox = Shared::new(init.air.secret.clone());
        let air = init.air.clone();
        Self(Shared::restore(init, InstanceState::restore), inbox, air)
    }
}
impl DynInstance for Instance {
    fn id(&self) -> Id {self.0.clone().load().instance.id()}
    fn notify(&self, enabled: bool) {
        let notifications = self.0.update(InstanceRequest::Notify(enabled)).3;
        for notification in notifications {self.2.notify(notification);}
    }
    fn send(&mut self, message: Message) -> Box<dyn DynPending> {
        match self.0.update(InstanceRequest::Send(message)).0 {
            Some(Update::Pending(pending)) => pending,
            _ => unreachable!("Sending a message must produce a pending result"),
        }
    }
    fn share(&self, name: Name) {
        let location = self.0.clone().load().instance.location();
        self.1.update(InboxRequest::Share(self.2.clone(), name, location));
    }
    fn pending(&self) -> Box<dyn traits::DynRef + '_> {
        Box::new(Snapshot(self.0.load(), |state| state.instance.pending().unwrap()))
    }
    fn confirmed(&self) -> Option<Box<dyn traits::DynRef + '_>> {
        let state = self.0.load();
        state.instance.confirmed().is_some().then(|| {
            Box::new(Snapshot(state, |state: &InstanceState| state.instance.confirmed().unwrap()))
                as Box<dyn traits::DynRef>
        })
    }
    fn try_next(&mut self) -> Option<ContractUpdate> {
        loop {if let Some(update) = self.0.try_next()?.0 {return Some(update)}}
    }
    fn next(&mut self) -> traits::BoxFuture<'_, ContractUpdate> {
        Box::pin(async {loop {if let Some(update) = self.0.next().await.0 {return update}}})
    }
}

#[derive(Clone, Debug)]
struct Pending(Shared<PendingState>);
impl Pending {
    fn new(value: Value) -> Self {Self(Shared::temporary(value))}
    fn update(&self, result: Value, confirmed: bool) {self.0.clone().update((result, confirmed));}
}
impl DynPending for Pending {
    fn pending(&self) -> Box<dyn traits::DynRef + '_> {
        Box::new(Snapshot(self.0.load(), |state| &*state.0))
    }
    fn try_confirmed(&self) -> Option<Value> {
        let state = self.0.load();
        state.1.then(|| state.0.clone())
    }
    fn try_next(&mut self) -> Option<bool> {self.0.try_next()}
    fn next(&mut self) -> traits::BoxFuture<'_, bool> {Box::pin(self.0.next())}
}

type Predicate<'a, P> = traits::BoxFuture<'a, P>;

#[derive(Clone)]
struct PendingState(Value, bool);
impl Debug for PendingState {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {f.debug_tuple("Pending").field(&self.1).finish()}
}
impl Sharable for PendingState {
    type Init = Value;
    type Request = (Value, bool);
    type Response = bool;

    async fn init(value: Value) -> Self {Self(value, false)}
    async fn handle(&mut self, (value, confirmed): Self::Request) -> bool {
        *self = Self(value, confirmed);
        confirmed
    }
}

#[derive(Clone, Debug)]
struct InstancesInit(Air, ContractType);
impl Hash for InstancesInit {
    fn hash<H: Hasher>(&self, hash: &mut H) {self.0.hash(hash); self.1.id.hash(hash);}
}

enum InstancesRequest {Create(Message), Received(Instance)}

#[derive(Clone, Debug)]
struct InstancesState(HashMap<Id, Instance>, Air, ContractType);
impl Sharable for InstancesState {
    type Init = InstancesInit;
    type Request = InstancesRequest;
    type Response = Instance;

    async fn init(init: InstancesInit) -> Self {
        let inbox: Shared<InboxState> = Shared::new(init.0.secret.clone());
        // Restore local content before exposing the first snapshot; services handle network sync.
        let instances = inbox.load().0.list(&init.1.id).into_iter().filter_map(|location| {
            let instance = Instance::open(InstanceInit {
                air: init.0.clone(), contract: init.1, location,
            });
            if instance.0.load().instance.pending().is_some() {Some((instance.id(), instance))}
            else {None}
        }).collect();
        Self(instances, init.0, init.1)
    }
    async fn handle(&mut self, request: Self::Request) -> Instance {
        match request {
            InstancesRequest::Create(value) => {
                let location = (self.2.location)(&self.1.secret, &value);
                self.0.entry(Id::hash(&location)).or_insert_with(|| {
                    let init = InstanceInit {air: self.1.clone(), contract: self.2, location};
                    let instance = Instance::open(init.clone());
                    instance.0.update(InstanceRequest::Initialize(self.1.clone(), self.2, value));
                    Services::start::<InstanceService>(init);
                    instance.1.update(InboxRequest::Share(self.1.clone(), self.1.secret.name(), location));
                    instance
                }).clone()
            }
            InstancesRequest::Received(instance) => self.0.entry(instance.id()).or_insert(instance).clone(),
        }
    }
}

enum InstancesPredicate {Received(Location), Initialized(Id)}

struct InstancesService {
    inbox: Shared<InboxState>,
    instances: Shared<InstancesState>,
    pending: HashMap<Id, Instance>,
    init: InstancesInit,
}
impl Service for InstancesService {
    type Init = InstancesInit;
    type Predicate = InstancesPredicate;

    async fn init(init: InstancesInit) -> Self {
        let inbox: Shared<InboxState> = Shared::new(init.0.secret.clone());
        let instances = Shared::local(init.clone());
        let locations = inbox.load().0.list(&init.1.id);
        let mut service = Self {inbox, instances, pending: HashMap::new(), init};
        for location in locations {service.handle(InstancesPredicate::Received(location)).await;}
        service
    }
    async fn predicate(&mut self) -> InstancesPredicate {
        let mut predicates: FuturesUnordered<Predicate<'_, InstancesPredicate>> = self.pending
            .iter_mut().map(|(id, instance)| Box::pin(async move {
                loop {
                    if instance.0.load().instance.pending().is_some() {
                        return InstancesPredicate::Initialized(*id)
                    }
                    instance.0.next().await;
                }
            }) as Predicate<'_, InstancesPredicate>).collect();
        predicates.push(Box::pin(async {
            loop {
                if let Some(location) = self.inbox.next().await.0 {
                    if location.contract == self.init.1.id {return InstancesPredicate::Received(location)}
                }
            }
        }));
        predicates.next().await.unwrap()
    }
    async fn handle(&mut self, predicate: InstancesPredicate) {
        match predicate {
            InstancesPredicate::Received(location) => {
                let id = Id::hash(&location);
                let init = InstanceInit {air: self.init.0.clone(), contract: self.init.1, location};
                Services::start::<InstanceService>(init.clone());
                if self.instances.load().0.contains_key(&id) || self.pending.contains_key(&id) {return}
                let instance = Instance::open(init);
                if instance.0.load().instance.pending().is_some() {
                    self.instances.update(InstancesRequest::Received(instance));
                } else {self.pending.insert(id, instance);}
            }
            InstancesPredicate::Initialized(id) => {
                self.instances.update(InstancesRequest::Received(self.pending.remove(&id).unwrap()));
            }
        }
    }
}

#[derive(Clone)]
struct InstanceInit {
    air: Air,
    contract: ContractType,
    location: Location,
}
impl Hash for InstanceInit {
    fn hash<H: Hasher>(&self, hash: &mut H) {self.air.hash(hash); self.location.hash(hash);}
}

enum InstanceRequest {
    Start, Request, Notify(bool), Initialize(Air, ContractType, Message),
    Send(Message), Response(Air, air::PublicKey, Response),
}

type InstanceResponse = (
    Option<ContractUpdate>, Option<Request>, Vec<(Pending, Value, bool)>, Vec<Notification>,
);

struct ContractRestore(ContractType);
impl<'de> DeserializeSeed<'de> for ContractRestore {
    type Value = Box<dyn traits::DynContract>;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        let mut erased = <dyn erased_serde::Deserializer>::erase(deserializer);
        Ok((self.0.restore)(&mut erased))
    }
}

struct Restore(ContractType);
impl<'de> DeserializeSeed<'de> for Restore {
    type Value = InstanceState;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<InstanceState, D::Error> {
        deserializer.deserialize_tuple(2, self)
    }
}
impl<'de> Visitor<'de> for Restore {
    type Value = InstanceState;
    fn expecting(&self, f: &mut Formatter<'_>) -> FmtResult {f.write_str("Air state and pending count")}
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<InstanceState, A::Error> {
        let instance = sequence.next_element_seed(ContractRestore(self.0))?
            .ok_or_else(|| A::Error::custom("Missing Air state"))?;
        let pending: usize = sequence.next_element()?
            .ok_or_else(|| A::Error::custom("Missing pending count"))?;
        Ok(InstanceState {
            instance, pending: (0..pending).map(|_| None).collect(), live: false, notify: false,
        })
    }
}

#[derive(Clone, Debug)]
struct InstanceState {
    instance: Box<dyn traits::DynContract>,
    pending: VecDeque<Option<Pending>>,
    live: bool,
    notify: bool,
}
impl InstanceState {
    fn restore(init: &InstanceInit, bytes: &[u8]) -> Self {
        Restore(init.contract).deserialize(&mut postcard::Deserializer::from_bytes(bytes)).unwrap()
    }
}
impl Serialize for InstanceState {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (&self.instance, self.pending.len()).serialize(serializer)
    }
}
impl Sharable for InstanceState {
    // Preserve the persisted namespace from before the AIR modules were consolidated.
    fn name() -> &'static str {"ramp2::air::instances::InstanceState"}

    type Init = InstanceInit;
    type Request = InstanceRequest;
    type Response = InstanceResponse;

    async fn init(init: InstanceInit) -> Self {
        let instance = (init.contract.create)(init.air.secret.clone(), init.location, None);
        Self {instance, pending: VecDeque::new(), live: false, notify: false}
    }
    async fn handle(&mut self, request: InstanceRequest) -> InstanceResponse {
        let mut pending = vec![];
        let mut notifications = vec![];
        let update = match request {
            InstanceRequest::Start => {
                self.live = false;
                return (None, Some(self.instance.start()), vec![], vec![])
            }
            InstanceRequest::Request => return (None, self.instance.request(), vec![], vec![]),
            InstanceRequest::Notify(enabled) => {
                self.notify = enabled;
                if !enabled {notifications.push(Notification::Remove(self.instance.id()));}
                else if self.live && let Some((key, server)) = self.instance.subscription() {
                    notifications.push(Notification::Watch(self.instance.id(), key, server));
                }
                return (None, None, vec![], notifications)
            }
            InstanceRequest::Initialize(air, contract, value) => {
                if self.instance.pending().is_none() {
                    self.instance = (contract.create)(air.secret, self.instance.location(), Some(value));
                }
                return (None, None, vec![], notifications)
            }
            InstanceRequest::Send(message) => {
                let pending = Pending::new(self.instance.send(message));
                self.pending.push_back(Some(pending.clone()));
                Update::Pending(Box::new(pending) as Box<dyn DynPending>)
            }
            InstanceRequest::Response(air, key, response) => {
                let output = self.instance.response(&air, response).await;
                if self.notify && self.live {
                    if matches!(output, Output::Read(_, _)) {notifications.push(Notification::Local);}
                    if matches!(output, Output::Read(_, _) | Output::Created(_, _)) {
                        notifications.push(Notification::Received(
                            self.instance.id(), key, self.instance.location().server,
                        ));
                    }
                }
                let (results, update) = match output {
                    Output::Init(results) => (results, Update::Init),
                    Output::Created(result, results) => {
                        if let Some(Some(handle)) = self.pending.pop_front() {
                            pending.push((handle, result.clone(), true));
                        }
                        (results, Update::Confirmed(result))
                    }
                    Output::Read(result, results) => (results, Update::Confirmed(result)),
                    Output::Subscribed => {
                        self.live = true;
                        let Some((key, server)) = self.instance.subscription() else {
                            return (None, None, vec![], notifications)
                        };
                        if self.notify {
                            notifications.push(Notification::Watch(self.instance.id(), key, server));
                        }
                        else {notifications.push(Notification::Remove(self.instance.id()));}
                        return (None, None, vec![], notifications)
                    }
                    Output::Garbage => return (None, None, vec![], notifications),
                };
                for (result, handle) in results.into_iter().zip(&self.pending) {
                    if let Some(handle) = handle {pending.push((handle.clone(), result, false));}
                }
                update
            }
        };
        (Some(update), None, pending, notifications)
    }
}

enum InstancePredicate {Response(Response), Changed}

struct InstanceService {
    instance: Shared<InstanceState>,
    responses: Responses,
    server: Name,
    file: air::PublicKey,
    air: Air,
}
impl Service for InstanceService {
    type Init = InstanceInit;
    type Predicate = InstancePredicate;

    async fn init(init: Self::Init) -> Self {
        let server = init.location.server;
        let air = init.air.clone();
        let instance: Shared<InstanceState> = Shared::restore(init, InstanceState::restore);
        let request = instance.update(InstanceRequest::Start).1.unwrap();
        let file = match &request {Request::Read(key) => *key, Request::Create(signed) => signed.key};
        let responses = air.send(server, request).await;
        Self {instance, responses, server, file, air}
    }
    async fn predicate(&mut self) -> InstancePredicate {
        tokio::select! {
            response = self.responses.next() => InstancePredicate::Response(response),
            _ = self.instance.next() => InstancePredicate::Changed,
        }
    }
    async fn handle(&mut self, predicate: InstancePredicate) {
        if let InstancePredicate::Response(response) = predicate {
            let request = InstanceRequest::Response(self.air.clone(), self.file, response);
            let (_, _, pending, notifications) = self.instance.update(request);
            for (handle, result, confirmed) in pending {handle.update(result, confirmed);}
            for notification in notifications {self.air.notify(notification);}
        }
        if let Some(request) = self.instance.update(InstanceRequest::Request).1 {
            self.file = match &request {
                Request::Read(key) => *key, Request::Create(signed) => signed.key,
            };
            self.responses = self.air.send(self.server, request).await;
        }
    }
}

enum InboxRequest {Start, Requests, Response(Air, RequestType, Response), Share(Air, Name, Location)}

type InboxResponse = (Option<Location>, HashMap<RequestType, Request>);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct InboxState(Inbox, Name);
impl Sharable for InboxState {
    // Preserve the persisted namespace from before the AIR modules were consolidated.
    fn name() -> &'static str {"ramp2::air::instances::InboxState"}

    type Init = Secret;
    type Request = InboxRequest;
    type Response = InboxResponse;

    async fn init(secret: Secret) -> Self {
        let name = secret.name();
        Self(Inbox::new(secret).expect("Could not create the Air inbox"), name)
    }
    async fn handle(&mut self, request: InboxRequest) -> InboxResponse {
        let location = match request {
            InboxRequest::Start => return (None, self.0.start()),
            InboxRequest::Requests => return (None, self.0.requests()),
            InboxRequest::Response(mut air, kind, response) => {
                self.0.response(&mut air, kind, response).await.map(|(_, location)| location)
            }
            InboxRequest::Share(mut air, name, location) => {
                let sent = self.0.send(&mut air, name, location).await;
                (sent && name == self.1).then_some(location)
            }
        };
        (location, HashMap::new())
    }
}

enum InboxPredicate {Response(RequestType, Response), Changed}

struct InboxService(Shared<InboxState>, HashMap<RequestType, Responses>, Air);
impl Service for InboxService {
    type Init = Air;
    type Predicate = InboxPredicate;

    async fn init(air: Air) -> Self {
        let inbox: Shared<InboxState> = Shared::new(air.secret.clone());
        let requests = inbox.update(InboxRequest::Start).1;
        let mut streams = HashMap::new();
        for (kind, request) in requests {streams.insert(kind, air.send(Name::orange_me(), request).await);}
        Self(inbox, streams, air)
    }
    async fn predicate(&mut self) -> InboxPredicate {
        let mut predicates: FuturesUnordered<Predicate<'_, InboxPredicate>> = self.1.iter_mut()
            .map(|(kind, stream)| Box::pin(async move {
                InboxPredicate::Response(*kind, stream.next().await)
            }) as Predicate<'_, InboxPredicate>).collect();
        predicates.push(Box::pin(async {self.0.next().await; InboxPredicate::Changed}));
        predicates.next().await.unwrap()
    }
    async fn handle(&mut self, predicate: InboxPredicate) {
        if let InboxPredicate::Response(kind, response) = predicate {
            if !matches!(response, Response::Read(_, None)) {self.1.remove(&kind);}
            self.0.update(InboxRequest::Response(self.2.clone(), kind, response));
        }
        for (kind, request) in self.0.update(InboxRequest::Requests).1 {
            self.1.insert(kind, self.2.send(Name::orange_me(), request).await);
        }
    }
}

struct Push {
    instance: PrismInstance<Notifications>,
    destination: traits::BoxFuture<'static, PushDestination>,
}
impl Hash for Push {
    fn hash<H: Hasher>(&self, hasher: &mut H) {self.instance.hash(hasher)}
}
impl Service for Push {
    type Init = Self;
    type Predicate = PushDestination;

    async fn init(init: Self) -> Self {init}
    async fn predicate(&mut self) -> PushDestination {self.destination.as_mut().await}
    async fn handle(&mut self, destination: PushDestination) {
        let push = match &destination {
            PushDestination::Apns(token) => air_notifications::PushDestination::Apns(token.clone()),
            PushDestination::Fcm(token) => air_notifications::PushDestination::Fcm(token.clone()),
        };
        self.instance.send(PushMessage::Destination(push));
        self.destination = Box::pin(async move {destination.current().await});
    }
}

#[derive(Default)]
struct Network {
    resolver: Mutex<DefaultResolver>,
    clients: Mutex<HashMap<Name, Arc<OnceCell<Arc<Client>>>>>,
}
impl Network {
    async fn client(&self, server: Name) -> Arc<Client> {
        let client = self.clients.lock().await.entry(server).or_default().clone();
        client.get_or_init(async || {
            let identity = self.resolver.lock().await.resolve(server, None).await;
            let mut delay = Duration::from_millis(250);
            loop {
                match Client::new((*identity).clone(), "wss").await {
                    Ok(client) => return Arc::new(client),
                    Err(error) => log::warn!("Air connection to {server} failed: {error}"),
                }
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(10));
            }
        }).await.clone()
    }
}
