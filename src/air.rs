use air::{Contract, DefaultResolver, Id, Inbox, Location, Name, Output, Request, RequestType};
use air::{Resolver, Response, Secret};
use air_server::Client;
use arc_swap::ArcSwap;
use crossfire::{AsyncRx, spsc};
use futures::{StreamExt, stream::FuturesUnordered};
use maverick_os::shared::{Ref, Service, Services, Sharable, Shared};
use prism::traits::{self, Update};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{HashMap, VecDeque};
use std::fmt::Debug;
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::pin::Pin;
use std::sync::{Arc, LazyLock, Mutex as ResponseMutex};
use std::time::Duration;
use tokio::sync::{Mutex, OnceCell, broadcast};

#[derive(Clone, Debug)]
pub struct Air {secret: Secret}
impl Air {
    pub fn new(secret: Secret) -> Self {
        Services::start::<InboxService>(secret.clone());
        Self {secret}
    }
}
impl traits::Air for Air {
    type Instance<C: Contract> = Instance<C>;
    type Instances<C: Contract> = Instances<C>;

    fn instances<C: Contract>(&self) -> Instances<C> {
        let instances = Shared::new(self.secret.clone());
        Services::start::<InstancesService<C>>(self.secret.clone());
        Instances(instances)
    }
    fn me(&self) -> Name {self.secret.name()}
}

#[derive(Clone, Debug)]
pub struct Instances<C: Contract>(Shared<InstancesState<C>>);
impl<C: Contract> traits::Instances<C> for Instances<C> {
    type Instance = Instance<C>;

    fn create(&self, init: C::Init) -> Instance<C> {self.0.update(InstancesRequest::Create(init))}
    fn list(&self) -> HashMap<Id, Instance<C>> {self.0.load().0.clone()}
    fn try_next(&mut self) -> Option<Instance<C>> {self.0.try_next()}
    async fn next(&mut self) -> Instance<C> {self.0.next().await}
}

#[derive(Clone, Debug)]
pub struct Instance<C: Contract>(Shared<InstanceState<C>>, Shared<InboxState>);
impl<C: Contract> traits::Instance<C> for Instance<C> {
    type Pending = Pending<C::Result>;
    type Ref<'a> = Ref<'a, C>;

    fn send(&mut self, message: C::Message) -> Self::Pending {
        match self.0.update(InstanceRequest::Send(message)).0 {
            Some(Update::Pending(pending)) => pending,
            _ => unreachable!("Sending a message must produce its pending result"),
        }
    }
    fn share(&self, name: Name) {
        let location = *self.0.clone().load().0.location();
        self.1.update(InboxRequest::Share(name, location));
    }
    fn pending(&self) -> Ref<'_, C> {self.0.load().map(|state| state.0.pending().unwrap())}
    fn confirmed(&self) -> Option<Ref<'_, C>> {
        let state = self.0.load();
        state.0.confirmed().is_some().then(|| state.map(|state| state.0.confirmed().unwrap()))
    }
    fn try_next(&mut self) -> Option<Update<Self::Pending, C::Result>> {
        loop {if let Some(update) = self.0.try_next()?.0 {return Some(update)}}
    }
    async fn next(&mut self) -> Update<Self::Pending, C::Result> {
        loop {if let Some(update) = self.0.next().await.0 {return update}}
    }
}

#[derive(Debug)]
pub struct Pending<R: Clone + Debug + Send + Sync + 'static> {
    value: Arc<ArcSwap<(R, bool)>>,
    sender: broadcast::Sender<bool>,
    responses: ResponseMutex<broadcast::Receiver<bool>>,
}
impl<R: Clone + Debug + Send + Sync + 'static> Pending<R> {
    fn update(&self, result: R, confirmed: bool) {
        self.value.store(Arc::new((result, confirmed)));
        let _ = self.sender.send(confirmed);
    }
}
impl<R: Clone + Debug + Send + Sync + 'static> Clone for Pending<R> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(), sender: self.sender.clone(),
            responses: ResponseMutex::new(self.sender.subscribe()),
        }
    }
}
impl<R: Clone + Debug + Send + Sync + 'static> traits::Pending<R> for Pending<R> {
    type Ref<'a> = Ref<'a, R>;

    fn pending(&self) -> Ref<'_, R> {
        *self.responses.lock().unwrap() = self.sender.subscribe();
        Ref::new(self.value.as_ref().load()).map(|state| &state.0)
    }
    fn try_confirmed(&self) -> Option<R> {
        *self.responses.lock().unwrap() = self.sender.subscribe();
        let state = self.value.as_ref().load();
        state.1.then(|| state.0.clone())
    }
    async fn confirmed(mut self) -> R {
        loop {
            if let Some(result) = self.try_confirmed() {return result}
            self.next().await;
        }
    }
    fn try_next(&mut self) -> Option<bool> {
        match self.responses.get_mut().unwrap().try_recv() {
            Ok(confirmed) => Some(confirmed),
            Err(broadcast::error::TryRecvError::Empty) => None,
            Err(error) => panic!("{error}"),
        }
    }
    async fn next(&mut self) -> bool {self.responses.get_mut().unwrap().recv().await.unwrap()}
}


type InstanceResponse<R> = (
    Option<Update<Pending<R>, R>>, Option<Request>, Vec<(Pending<R>, R, bool)>,
);
type InboxResponse = (Option<Location>, HashMap<RequestType, Request>);
type Predicate<'a, P> = Pin<Box<dyn Future<Output = P> + Send + 'a>>;

static NETWORK: LazyLock<Network> = LazyLock::new(Network::default);

enum InstancesRequest<C: Contract> {Create(C::Init), Received(Instance<C>)}
enum InstancesPredicate {Received(Location), Initialized(Id)}
enum InstanceRequest<C: Contract> {
    Start, Request, Initialize(Secret, C::Init), Send(C::Message), Response(Response),
}
enum InstancePredicate {Response(Response), Changed}
enum InboxRequest {Start, Requests, Response(RequestType, Response), Share(Name, Location)}
enum InboxPredicate {Response(RequestType, Response), Changed}

#[derive(Clone)]
struct InstanceInit<I> {secret: Secret, location: Location, init: Option<I>}
impl<I> Hash for InstanceInit<I> {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        self.secret.hash(hasher);
        self.location.hash(hasher);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct InboxState(Inbox, Name);
impl Sharable for InboxState {
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
            InboxRequest::Response(kind, response) => {
                self.0.response(&mut *NETWORK.resolver.lock().await, kind, response).await.map(|(_, l)| l)
            }
            InboxRequest::Share(name, location) => {
                let sent = self.0.send(&mut *NETWORK.resolver.lock().await, name, location).await;
                (sent && name == self.1).then_some(location)
            }
        };
        (location, HashMap::new())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "C: Contract")]
struct InstancesState<C: Contract>(#[serde(skip)] HashMap<Id, Instance<C>>, Secret);
impl<C: Contract> Sharable for InstancesState<C> {
    type Init = Secret;
    type Request = InstancesRequest<C>;
    type Response = Instance<C>;

    async fn init(secret: Secret) -> Self {Self(HashMap::new(), secret)}
    async fn handle(&mut self, request: Self::Request) -> Instance<C> {
        match request {
            InstancesRequest::Create(init) => {
                let location = air::Instance::<C>::generate_location(&self.1, &init)
                    .expect("Could not create the Air instance location");
                self.0.entry(Id::hash(&location)).or_insert_with(|| {
                    let data = InstanceInit {secret: self.1.clone(), location, init: Some(init.clone())};
                    let instance = Instance(Shared::new(data.clone()), Shared::new(self.1.clone()));
                    instance.0.update(InstanceRequest::Initialize(self.1.clone(), init));
                    Services::start::<InstanceService<C>>(data);
                    instance.1.update(InboxRequest::Share(self.1.name(), location));
                    instance
                }).clone()
            }
            InstancesRequest::Received(instance) => {
                let id = instance.0.load().0.id();
                self.0.entry(id).or_insert(instance).clone()
            }
        }
    }
}

#[derive(Clone, Debug)]
struct InstanceState<C: Contract>(air::Instance<C>, VecDeque<Option<Pending<C::Result>>>);
impl<C: Contract> Serialize for InstanceState<C> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (&self.0, self.1.len()).serialize(serializer)
    }
}
impl<'de, C: Contract> Deserialize<'de> for InstanceState<C> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (instance, pending): (air::Instance<C>, usize) = Deserialize::deserialize(deserializer)?;
        Ok(Self(instance, (0..pending).map(|_| None).collect()))
    }
}
impl<C: Contract> Sharable for InstanceState<C> {
    type Init = InstanceInit<C::Init>;
    type Request = InstanceRequest<C>;
    type Response = InstanceResponse<C::Result>;

    async fn init(init: Self::Init) -> Self {
        let instance = match init.init {
            Some(value) => air::Instance::new(init.secret, value),
            None => air::Instance::receive(init.secret, init.location),
        }.expect("Could not open the Air instance");
        Self(instance, VecDeque::new())
    }
    async fn handle(&mut self, request: Self::Request) -> Self::Response {
        let mut notifications = vec![];
        let update = match request {
            InstanceRequest::Start => return (None, Some(self.0.start()), vec![]),
            InstanceRequest::Request => return (None, self.0.request(), vec![]),
            InstanceRequest::Initialize(secret, init) => {
                if self.0.pending().is_none() {
                    self.0 = air::Instance::new(secret, init).expect("Could not initialize the Air instance");
                }
                return (None, None, vec![])
            }
            InstanceRequest::Send(message) => {
                let result = self.0.send(message);
                let (sender, responses) = broadcast::channel(1024);
                let pending = Pending {
                    value: Arc::new(ArcSwap::from_pointee((result, false))), sender,
                    responses: ResponseMutex::new(responses),
                };
                self.1.push_back(Some(pending.clone()));
                Update::Pending(pending)
            }
            InstanceRequest::Response(response) => {
                let (results, update) = match self.0
                    .response(&mut *NETWORK.resolver.lock().await, response).await {
                    Output::Init(results) => (results, Update::Init),
                    Output::Created(result, results) => {
                        if let Some(Some(pending)) = self.1.pop_front() {
                            notifications.push((pending, result.clone(), true));
                        }
                        (results, Update::Confirmed(result))
                    }
                    Output::Read(result, results) => (results, Update::Confirmed(result)),
                    Output::Subscribed | Output::Garbage => return (None, None, vec![]),
                };
                for (result, pending) in results.into_iter().zip(&self.1) {
                    if let Some(pending) = pending {
                        notifications.push((pending.clone(), result, false));
                    }
                }
                update
            }
        };
        (Some(update), None, notifications)
    }
}

struct InstancesService<C: Contract> {
    inbox: Shared<InboxState>,
    instances: Shared<InstancesState<C>>,
    pending: HashMap<Id, Shared<InstanceState<C>>>,
    secret: Secret,
}
impl<C: Contract> Service for InstancesService<C> {
    type Init = Secret;
    type Predicate = InstancesPredicate;

    async fn init(secret: Secret) -> Self {
        let inbox: Shared<InboxState> = Shared::new(secret.clone());
        let instances: Shared<InstancesState<C>> = Shared::new(secret.clone());
        let locations = inbox.load().0.list(&C::id());
        let mut service = Self {inbox, instances, pending: HashMap::new(), secret};
        for location in locations {service.handle(InstancesPredicate::Received(location)).await;}
        service
    }
    async fn predicate(&mut self) -> InstancesPredicate {
        let mut predicates: FuturesUnordered<Predicate<'_, InstancesPredicate>> = self.pending
            .iter_mut().map(|(id, instance)| Box::pin(async move {
                loop {
                    if instance.load().0.pending().is_some() {
                        return InstancesPredicate::Initialized(*id)
                    }
                    instance.next().await;
                }
            }) as Predicate<'_, InstancesPredicate>).collect();
        predicates.push(Box::pin(async {
            loop {
                if let Some(location) = self.inbox.next().await.0 {
                    if location.contract == C::id() {return InstancesPredicate::Received(location)}
                }
            }
        }));
        predicates.next().await.unwrap()
    }
    async fn handle(&mut self, predicate: InstancesPredicate) {
        match predicate {
            InstancesPredicate::Received(location) => {
                let id = Id::hash(&location);
                if self.instances.load().0.contains_key(&id) || self.pending.contains_key(&id) {
                    return
                }
                let init = InstanceInit {secret: self.secret.clone(), location, init: None};
                let instance: Shared<InstanceState<C>> = Shared::new(init.clone());
                Services::start::<InstanceService<C>>(init);
                if instance.load().0.pending().is_some() {
                    let instance = Instance(instance, self.inbox.clone());
                    self.instances.update(InstancesRequest::Received(instance));
                } else {self.pending.insert(id, instance);}
            }
            InstancesPredicate::Initialized(id) => {
                let instance = Instance(self.pending.remove(&id).unwrap(), self.inbox.clone());
                self.instances.update(InstancesRequest::Received(instance));
            }
        }
    }
}

struct InstanceService<C: Contract>(Shared<InstanceState<C>>, AsyncRx<spsc::List<Response>>, Arc<Client>);
impl<C: Contract> Service for InstanceService<C> {
    type Init = InstanceInit<C::Init>;
    type Predicate = InstancePredicate;

    async fn init(init: Self::Init) -> Self {
        let server = init.location.server;
        let client = NETWORK.client(server).await;
        let instance: Shared<InstanceState<C>> = Shared::new(init);
        let request = instance.update(InstanceRequest::Start).1.unwrap();
        Self(instance, client.send(request).await, client)
    }
    async fn predicate(&mut self) -> InstancePredicate {
        tokio::select! {
            response = self.1.recv() => {
                InstancePredicate::Response(response.expect("Air closed a response stream"))
            }
            _ = self.0.next() => InstancePredicate::Changed,
        }
    }
    async fn handle(&mut self, predicate: InstancePredicate) {
        if let InstancePredicate::Response(response) = predicate {
            let (_, _, notifications) = self.0.update(InstanceRequest::Response(response));
            for (pending, result, confirmed) in notifications {pending.update(result, confirmed);}
        }
        if let Some(request) = self.0.update(InstanceRequest::Request).1 {
            self.1 = self.2.send(request).await;
        }
    }
}

struct InboxService(Shared<InboxState>, HashMap<RequestType, AsyncRx<spsc::List<Response>>>, Arc<Client>);
impl Service for InboxService {
    type Init = Secret;
    type Predicate = InboxPredicate;

    async fn init(secret: Secret) -> Self {
        let client = NETWORK.client(Name::orange_me()).await;
        let inbox: Shared<InboxState> = Shared::new(secret);
        let requests = inbox.update(InboxRequest::Start).1;
        let mut streams = HashMap::new();
        for (kind, request) in requests {streams.insert(kind, client.send(request).await);}
        Self(inbox, streams, client)
    }
    async fn predicate(&mut self) -> InboxPredicate {
        let mut predicates: FuturesUnordered<Predicate<'_, InboxPredicate>> = self.1.iter_mut()
            .map(|(kind, stream)| Box::pin(async move {
                let response = stream.recv().await.expect("Air closed an inbox response stream");
                InboxPredicate::Response(*kind, response)
            }) as Predicate<'_, InboxPredicate>).collect();
        predicates.push(Box::pin(async {self.0.next().await; InboxPredicate::Changed}));
        predicates.next().await.unwrap()
    }
    async fn handle(&mut self, predicate: InboxPredicate) {
        if let InboxPredicate::Response(kind, response) = predicate {
            if !matches!(response, Response::Read(_, None)) {self.1.remove(&kind);}
            self.0.update(InboxRequest::Response(kind, response));
        }
        for (kind, request) in self.0.update(InboxRequest::Requests).1 {
            self.1.insert(kind, self.2.send(request).await);
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
