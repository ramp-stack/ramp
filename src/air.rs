use air::{
    DefaultResolver, Id, Identity, Location, Name, Output, Request, RequestType, Resolver,
    Response, Secret,
};
use air_server::Client;
use crossfire::{AsyncRx, spsc};
use futures::{StreamExt, stream::FuturesUnordered};
use maverick_os::{Hardware, Notify};
use maverick_os::shared::{Service, Services};
use prism::air::{ContractType, Request as Message, Value};
use prism::service::{ServiceInit, Key};
use prism::traits::{self, ContractUpdate, DynInstance, DynInstances, DynPending, Update};
use serde::{Serialize, Serializer, Deserialize};
use tokio::sync::{Mutex, OnceCell};

use std::collections::{HashMap, VecDeque, hash_map::Entry};
use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;

use crate::notifications::{Action, Push, Subscription};
use crate::shared::{Sharable, Shared, Snapshot};

type InstanceResponse = (
    Option<ContractUpdate>, Option<Request>, Vec<(Pending, Value, bool)>, Vec<Action>,
);
type AirResponse = (Option<Location>, HashMap<RequestType, Request>);

#[derive(Clone)]
pub struct Air {
    state: Shared<AirState>,
    secret: Secret,
    network: Arc<Network>,
    notifications: Option<Box<Instance>>,
    notify: Notify,
}
impl Air {
    pub fn new(secret: Secret, hardware: &Hardware) -> Self {
        let mut air = Self {
            state: Shared::new(secret.clone()),
            secret, network: Arc::new(Network::default()), notifications: None,
            notify: hardware.notifier(),
        };
        Services::start::<AirService>(air.clone());
        air.notifications = Push::start(&air, hardware).map(Box::new);
        air
    }
}
impl Debug for Air {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_tuple("Air").field(&self.secret.name()).finish()
    }
}
impl Air {
    fn notify(&self, notification: Action) {
        Push::notify(self.notifications.as_deref(), &self.notify, notification);
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
pub struct Instance(Shared<InstanceState>, Air);
impl Instance {
    pub fn create<C: air::Contract>(air: Air, init: C::Init) -> Self {
        Instances::new(air, ContractType::new::<C>()).0
            .update(InstancesRequest::Create(Box::new(init))).unwrap()
    }
    fn open(init: InstanceInit) -> Self {
        let air = init.air.clone();
        Self(Shared::restore(init, InstanceState::restore), air)
    }
}
impl DynInstance for Instance {
    fn id(&self) -> Id {self.0.clone().load().instance.id()}
    fn notify(&self, enabled: bool) {
        let notifications = self.0.update(InstanceRequest::Notify(enabled)).3;
        for notification in notifications {self.1.notify(notification);}
    }
    fn send(&mut self, message: Message) -> Box<dyn DynPending> {
        match self.0.update(InstanceRequest::Send(message)).0 {
            Some(Update::Pending(pending)) => pending,
            _ => unreachable!("Sending a message must produce a pending result"),
        }
    }
    fn share(&self, name: Name) {
        let location = self.0.clone().load().instance.location();
        self.1.state.update(AirRequest::Share(self.1.clone(), name, location));
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
        Box::new(self.0.update(InstancesRequest::Create(init)).unwrap())
    }
    fn list(&self) -> HashMap<Id, Box<dyn DynInstance>> {
        self.0.load().0.iter().map(|(id, instance)| {
            (*id, Box::new(instance.clone()) as Box<dyn DynInstance>)
        }).collect()
    }
    fn try_next(&mut self) -> Option<Box<dyn DynInstance>> {
        loop {
            if let Some(instance) = self.0.try_next()? {return Some(Box::new(instance))}
        }
    }
    fn next(&mut self) -> traits::BoxFuture<'_, Box<dyn DynInstance>> {
        Box::pin(async {
            loop {
                if let Some(instance) = self.0.next().await {
                    return Box::new(instance) as Box<dyn DynInstance>
                }
            }
        })
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

enum InstancesRequest {Create(Message), Received(Box<Instance>)}

#[derive(Clone, Debug)]
struct InstancesState(HashMap<Id, Instance>, Air, ContractType);
impl Sharable for InstancesState {
    type Init = InstancesInit;
    type Request = InstancesRequest;
    type Response = Option<Instance>;

    async fn init(init: InstancesInit) -> Self {
        let catalog = init.0.state.clone();
        // Restore local content before exposing the first snapshot; services handle network sync.
        let instances = catalog.load().0.list(&init.1.id).into_iter().filter_map(|location| {
            let instance = Instance::open(InstanceInit {
                air: init.0.clone(), contract: init.1, location,
            });
            if instance.0.load().instance.pending().is_some() {Some((instance.id(), instance))}
            else {None}
        }).collect();
        Self(instances, init.0, init.1)
    }
    async fn handle(&mut self, request: Self::Request) -> Option<Instance> {
        match request {
            InstancesRequest::Create(value) => {
                let location = (self.2.location)(&self.1.secret, &value);
                Some(self.0.entry(Id::hash(&location)).or_insert_with(|| {
                    let init = InstanceInit {air: self.1.clone(), contract: self.2, location};
                    let instance = Instance::open(init.clone());
                    instance.1.state.update(AirRequest::Create(self.1.clone(), self.2, value));
                    Services::start::<InstanceService>(init);
                    instance
                }).clone())
            }
            InstancesRequest::Received(instance) => match self.0.entry(instance.id()) {
                Entry::Vacant(entry) => Some(entry.insert(*instance).clone()),
                Entry::Occupied(_) => None,
            },
        }
    }
}

struct InstancesService {
    catalog: Shared<AirState>,
    init: InstancesInit,
}
impl Service for InstancesService {
    type Init = InstancesInit;
    type Predicate = Location;

    async fn init(init: InstancesInit) -> Self {
        let catalog = init.0.state.clone();
        let locations = catalog.load().0.list(&init.1.id);
        let mut service = Self {catalog, init};
        for location in locations {service.handle(location).await;}
        service
    }
    async fn predicate(&mut self) -> Location {
        loop {
            if let Some(location) = self.catalog.next().await.0
                && location.contract == self.init.1.id {return location}
        }
    }
    async fn handle(&mut self, location: Location) {
        Services::start::<InstanceService>(InstanceInit {
            air: self.init.0.clone(), contract: self.init.1, location,
        });
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
    Start, Request, Notify(bool), Initialize(Box<dyn traits::DynContract>),
    Send(Message), Response(Box<Air>, air::PublicKey, Response),
}

#[derive(Clone, Debug)]
struct InstanceState {
    instance: Box<dyn traits::DynContract>,
    pending: VecDeque<Option<Pending>>,
    notifications: Subscription,
}
impl InstanceState {
    fn restore(init: &InstanceInit, bytes: &[u8]) -> Self {
        // Postcard tuples store their fields consecutively without framing.
        let mut deserializer = postcard::Deserializer::from_bytes(bytes);
        let instance = (init.contract.restore)(
            &mut <dyn erased_serde::Deserializer>::erase(&mut deserializer),
        );
        let pending = usize::deserialize(&mut deserializer).unwrap();
        let enabled = bool::deserialize(&mut deserializer).unwrap();
        let mut notifications = Subscription::new(instance.id());
        notifications.notify(enabled, None);
        Self {instance, pending: (0..pending).map(|_| None).collect(), notifications}
    }
}
impl Serialize for InstanceState {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (&self.instance, self.pending.len(), self.notifications.enabled()).serialize(serializer)
    }
}
impl Sharable for InstanceState {
    type Init = InstanceInit;
    type Request = InstanceRequest;
    type Response = InstanceResponse;

    async fn init(init: InstanceInit) -> Self {
        let instance = (init.contract.receive)(init.air.secret.clone(), init.location);
        let notifications = Subscription::new(instance.id());
        Self {instance, pending: VecDeque::new(), notifications}
    }
    async fn handle(&mut self, request: InstanceRequest) -> InstanceResponse {
        let mut pending = vec![];
        let update = match request {
            InstanceRequest::Start => {
                self.notifications.start();
                return (None, Some(self.instance.start()), vec![], vec![])
            }
            InstanceRequest::Request => return (None, self.instance.request(), vec![], vec![]),
            InstanceRequest::Notify(enabled) => {
                let notifications = self.notifications.notify(enabled, self.instance.subscription());
                return (None, None, vec![], notifications)
            }
            InstanceRequest::Initialize(instance) => {
                if self.instance.pending().is_none() {self.instance = instance;}
                return (None, None, vec![], vec![])
            }
            InstanceRequest::Send(message) => {
                let pending = Pending::new(self.instance.send(message));
                self.pending.push_back(Some(pending.clone()));
                Update::Pending(Box::new(pending) as Box<dyn DynPending>)
            }
            InstanceRequest::Response(air, key, response) => {
                let output = self.instance.response(air.as_ref(), response).await;
                let notifications = self.notifications.response(
                    &output, (key, self.instance.location().server), self.instance.subscription(),
                );
                let (results, update) = match output {
                    Output::Init(results) => (results, Some(Update::Init)),
                    Output::Created(result, results) => {
                        if let Some(Some(handle)) = self.pending.pop_front() {
                            pending.push((handle, result.clone(), true));
                        }
                        (results, Some(Update::Confirmed(result)))
                    }
                    Output::Read(result, results) => (results, Some(Update::Confirmed(result))),
                    Output::Subscribed | Output::Garbage => (vec![], None),
                };
                for (result, handle) in results.into_iter().zip(&self.pending) {
                    if let Some(handle) = handle {pending.push((handle.clone(), result, false));}
                }
                return (update, self.instance.request(), pending, notifications)
            }
        };
        (Some(update), None, pending, vec![])
    }
}

struct InstanceService {
    instance: Shared<InstanceState>,
    instances: Shared<InstancesState>,
    responses: Responses,
    server: Name,
    file: air::PublicKey,
    air: Air,
}
impl Service for InstanceService {
    type Init = InstanceInit;
    type Predicate = InstanceRequest;

    async fn init(init: Self::Init) -> Self {
        let server = init.location.server;
        let air = init.air.clone();
        let instances = Shared::local(InstancesInit(air.clone(), init.contract));
        let instance: Shared<InstanceState> = Shared::restore(init, InstanceState::restore);
        if instance.load().instance.pending().is_some() {
            instances.update(InstancesRequest::Received(Box::new(Instance(
                instance.clone(), air.clone(),
            ))));
        }
        let request = instance.update(InstanceRequest::Start).1.unwrap();
        let file = match &request {Request::Read(key) => *key, Request::Create(signed) => signed.key};
        let responses = air.send(server, request).await;
        Self {instance, instances, responses, server, file, air}
    }
    async fn predicate(&mut self) -> InstanceRequest {
        tokio::select! {
            response = self.responses.next() => {
                InstanceRequest::Response(Box::new(self.air.clone()), self.file, response)
            }
            _ = self.instance.next() => InstanceRequest::Request,
        }
    }
    async fn handle(&mut self, request: InstanceRequest) {
        let (update, request, pending, notifications) = self.instance.update(request);
        for (handle, result, confirmed) in pending {handle.update(result, confirmed);}
        if matches!(update, Some(Update::Init)) {
            self.instances.update(InstancesRequest::Received(Box::new(Instance(
                self.instance.clone(), self.air.clone(),
            ))));
        }
        for notification in notifications {self.air.notify(notification);}

        if let Some(request) = request {
            self.file = match &request {
                Request::Read(key) => *key, Request::Create(signed) => signed.key,
            };
            self.responses = self.air.send(self.server, request).await;
        }
    }
}

enum AirRequest {
    Start, Requests, Response(Air, RequestType, Response),
    Create(Air, ContractType, Message), Share(Air, Name, Location),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct AirState(air::Air);
impl Sharable for AirState {
    type Init = Secret;
    type Request = AirRequest;
    type Response = AirResponse;

    async fn init(secret: Secret) -> Self {
        Self(air::Air::new(secret).expect("Could not create the Air state"))
    }
    async fn handle(&mut self, request: AirRequest) -> AirResponse {
        let location = match request {
            AirRequest::Start => return (None, self.0.start()),
            AirRequest::Requests => return (None, self.0.requests()),
            AirRequest::Response(mut air, kind, response) => {
                let location = self.0.response(&mut air, kind, response).await;
                return (location, self.0.requests())
            }
            AirRequest::Create(air, contract, value) => {
                let instance = (contract.create)(&mut self.0, value);
                let location = instance.location();
                let init = InstanceInit {air, contract, location};
                let state = Shared::<InstanceState>::restore(init, InstanceState::restore);
                state.update(InstanceRequest::Initialize(instance));
                Some(location)
            }
            AirRequest::Share(mut air, name, location) => {
                self.0.share(&mut air, name, location).await;
                None
            }
        };
        (location, HashMap::new())
    }
}

struct AirService(Shared<AirState>, HashMap<RequestType, Responses>, Air);
impl Service for AirService {
    type Init = Air;
    type Predicate = AirRequest;

    async fn init(air: Air) -> Self {
        let catalog = air.state.clone();
        let requests = catalog.update(AirRequest::Start).1;
        let mut streams = HashMap::new();
        for (kind, request) in requests {streams.insert(kind, air.send(Name::orange_me(), request).await);}
        Self(catalog, streams, air)
    }
    async fn predicate(&mut self) -> AirRequest {
        let mut responses: FuturesUnordered<_> = self.1.iter_mut()
            .map(|(kind, stream)| async move {(*kind, stream.next().await)}).collect();

        tokio::select! {
            Some((kind, response)) = responses.next() => {
                AirRequest::Response(self.2.clone(), kind, response)
            }
            _ = self.0.next() => AirRequest::Requests,
        }
    }
    async fn handle(&mut self, request: AirRequest) {
        if let AirRequest::Response(_, kind, response) = &request
            && !matches!(response, Response::Read(_, None)) {self.1.remove(kind);}
        for (kind, request) in self.0.update(request).1 {
            self.1.insert(kind, self.2.send(Name::orange_me(), request).await);
        }
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
