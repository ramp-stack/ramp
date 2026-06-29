use maverick_os::air::{self, Contract, Reactants, Reactant, Instance, Name, Service, Services, Listner, Metadata, Secret};
use maverick_os::air::names::Id;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::str::FromStr;

use bitcoin::{Network, Txid};

use bdk_wallet::{KeychainKind, ChangeSet, Update as BDKUpdate, LoadParams};
use bdk_wallet::descriptor::template::Bip86;
use bdk_wallet::bitcoin::bip32::Xpriv;
use bdk_wallet::{PersistedWallet, WalletPersister};
use bdk_wallet::chain::{Merge, ChainPosition};
use bdk_esplora::esplora_client::Builder;
use bdk_esplora::EsploraExt;

pub const LOCK: u64 = 2_000_000_000_000;//2000 seconds

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Address(bitcoin::Address);
impl std::ops::Deref for Address {
    type Target = bitcoin::Address;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<bitcoin::Address> for Address {fn from(addr: bitcoin::Address) -> Self {Address(addr)}}
impl From<Address> for bitcoin::Address {fn from(addr: Address) -> Self {addr.0}}
impl Serialize for Address {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}
impl<'de> Deserialize<'de> for Address {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(Address(bitcoin::Address::from_str(&s).map_err(serde::de::Error::custom)?
            .require_network(Network::Bitcoin).map_err(serde::de::Error::custom)?
        ))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Error{
    NotYourWallet(Name),
    NoAddresses,
    InsufficentFunds(u64, u64),
    ServiceRunning(u64)
}
impl std::error::Error for Error {}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {match self {
        Error::NotYourWallet(name) => write!(f, "This is not your wallet it belongs to {}", name),
        Error::NoAddresses => write!(f, "There are no available addresses to receive to, Ensure the WalletService is running"),
        Error::InsufficentFunds(have, want) => write!(f, "You have {have} but are trying to send {want}"),
        Error::ServiceRunning(wait) => write!(f, "Service is already running check again in {wait}"),
    }}
}

///All btc amounts are in nano bitcoins
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Transaction{
    pub amount: u64,
    pub received: bool,
    pub address: Address,
    pub priority: Priority,
    pub canceled: bool,
    pub txid: Option<Txid>,//None until transaction is created by the service
    pub fee: Option<u64>,
    pub timestamp: Option<u64>,//None until confirmed in the blockchain
    pub btc_price_usd: Option<f64>,
}
impl Transaction {
    pub fn new(address: Address, amount: u64, priority: Priority) -> Self {Transaction{
        amount,
        received: false,
        address,
        priority,
        canceled: false,
        txid: None,
        fee: None,
        timestamp: None,
        btc_price_usd: None,
    }}
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub enum Priority {
    #[default]
    Normal,
    Priority
}
impl Priority {
    pub fn blind_estimate(&self) -> u64 {match self {
        Self::Normal => 4200,
        Self::Priority => 8400,
    }}
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Lock(u64, Id);
impl Lock {
    pub fn lock(&mut self, id: Id, now: u64) -> Result<u64, Error> {
        if self.1 == id || self.0+LOCK < now {
            self.0 = now;
            self.1 = id;
            Ok(self.0)
        } else {
            Err(Error::ServiceRunning(self.0+LOCK - now))
        }
    }

    pub fn is_locked(id: Id, now: u64) -> bool {
        self.1 == id && self.0+LOCK > now
    }
}
 

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Wallet{
    lock: Lock,
    author: Name,
    balance: u64,
    btc_price_usd: f64,
    addresses: [Option<Address>; 10],
    transactions: BTreeMap<u64, Transaction>,
    internal: ChangeSet 
}

impl Wallet {
    fn balance(&self) -> u64 {self.balance}
    fn btc_to_usd(&self, nb: u64) -> f64 {self.btc_price_usd * (nb as f64 /1_000_000_000.0)}
    fn transactions(&self) -> Vec<&Transaction> {
        let mut v = self.transactions.iter().collect::<Vec<_>>();
        v.sort_by(|a, b| match (a.1.timestamp, b.1.timestamp) {
            (None, None) => a.0.cmp(b.0),
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(_), None) => std::cmp::Ordering::Less,
            (Some(x), Some(y)) => x.cmp(&y),
        });
        v.into_iter().map(|a| a.1).collect()
    }
}

impl Contract for Wallet {
    type Init = SecretKey;
    fn id() -> Id {Id::hash("Wallet")}

    fn init(init: Self::Init, metadata: Metadata) -> Self {
        let mut internal = MemoryPersister::default();
        let key = Xpriv::new_master(Network::Bitcoin, Id::hash(&init).as_ref()).unwrap(); 
        let _ = bdk_wallet::Wallet::create(Bip86(key, KeychainKind::External), Bip86(key, KeychainKind::Internal))
            .network(Network::Bitcoin).create_wallet(&mut internal).unwrap();
        Wallet {
            lock: Lock::default(),
            author: metadata.signer,
            balance: 0,
            btc_price_usd: 0.0,
            addresses: [const {None}; 10],
            transactions: BTreeMap::new(),
            internal: internal.0
        }
    }

    fn reactants() -> Reactants<Self> {
        Reactants::default().add::<GetAddress>().add::<Send>().add::<Update>().add::<StoreAddress>()
    }
}

pub trait WalletUtils {
    fn get_address(&mut self, ctx: &air::Context) -> PendingResult<Address, Error>;
    fn send(&mut self, address: Address, amount: u64, fee: Priority) -> PendingResult<u64, Error>;
}

impl WalletUtils for Instance<Wallet> {
    fn get_address(&mut self, ctx: &air::Context) -> PendingResult<Address, Error> {self.try_apply(TakeAddress)}
    fn send(&mut self, address: Address, amount: u64, fee: Priority) -> PendingResult<u64, Error> {
        self.try_apply(Send(address, amount, fee))
    }
}

///You can use the address immediately or hang on to the Id and listen for when this reactant confirms
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GetAddress;
impl Reactant<Wallet> for GetAddress {
    fn id() -> Id {Id::hash("GetAddress")}
    type Result = Result<Address, Error>;
    fn apply(self, wallet: &mut Wallet, metadata: Metadata) -> Self::Result {
        if metadata.signer != wallet.author {Err(Error::NotYourWallet(wallet.author))?}
        wallet.addresses.iter_mut().find_map(|a| a.take()).ok_or(Error::NoAddresses)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Send(Address, u64, Priority);
impl Reactant<Wallet> for Send {
    fn id() -> Id {Id::hash("Send")}
    type Result = Result<u64, Error>;
    fn apply(self, wallet: &mut Wallet, metadata: Metadata) -> Self::Result {
        if metadata.signer != wallet.author {Err(Error::NotYourWallet(wallet.author))?}
        let estimate = self.1+self.2.blind_estimate();
        if wallet.balance < estimate {Err(Error::InsufficentFunds(wallet.balance, estimate))?}
        wallet.balance -= estimate;
        wallet.transactions.insert(metadata.timestamp, Transaction::new(self.0, self.1, self.2));
        Ok(metadata.timestamp)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Update {
    btc_price_usd: f64,
    balance: u64,
    addresses: Vec<Address>,
    transactions: BTreeMap<u64, Transaction>,
    internal: Option<ChangeSet>
}
impl Reactant<Wallet> for Update {
    fn id() -> Id {Id::hash("Update")}
    type Result = Result<Option<ChangeSet>, Error>;
    fn apply(self, wallet: &mut Wallet, metadata: Metadata) -> Self::Result {
        //ensure lock and wallet ownership
        //update price and balance
        //store addresses
        //update transactions
        //merge internal change set
        //return internal change set if the lock was for a new service 
        todo!()
    }
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
struct MemoryPersister(ChangeSet);
impl WalletPersister for MemoryPersister {
    type Error = Error;
    fn initialize(persister: &mut Self) -> Result<ChangeSet, Self::Error> {Ok(persister.0.clone())}
    fn persist(persister: &mut Self, changeset: &ChangeSet) -> Result<(), Self::Error> {persister.0.merge(changeset.clone()); Ok(())}
}

pub struct WalletService{
    wallet: Instance<Wallet>,
    internal: PersistedWallet<MemoryPersister>,
}
impl WalletService {
  //async fn obtain_lock(instance: &mut Instance<Wallet>, service_id: Id) -> Option<Wallet> {
  //    if wallet.pending().lock.is_locked(service_id, now()) {None} else { loop {
  //        let wait = match wallet.try_apply(Update::lock(service_id)) {
  //            Ok(result) => match result.await {
  //                Ok(new_change_set) => break new_service.map(|change_set|
  //                    Wallet::load().load_wallet_no_persist(changeset).unwrap().unwrap()
  //                ),
  //                Err(Error::ServiceRunning(wait)) => wait,
  //                e => panic!("Error: {e:?}")
  //            },
  //            Err(Error::ServiceRunning(wait)) => wait,
  //            e => panic!("Error: {e:?}")
  //        };
  //        tokio::time::sleep(Duration::from_nanos(wait)).await;
  //    }}
  //}
}
impl Service for WalletService {
    fn id() -> Id {Id::hash(&"WALLETSERVICE")}
    async fn new(ctx: &mut air::Context, secret: Secret) -> Self {
        let wallet_key = secret.harden();
        let mut wallet = ctx.create::<Wallet>(wallet_key);
        let internal = Wallet::load().load_wallet_no_persist(wallet.pending().internal.clone()).unwrap().unwrap()
        WalletService{wallet, internal}
    }
  
    async fn run(&mut self, ctx: &mut air::Context) {
        //Check if I need to update the btc_price
        //
        //Sync bdk with latest state
        //
        //If there are any outgoing transactions that I havent processed yet create and
        //broadcast them
        //
        //Wait for next update to respond to I can listen to pending since I have the lock
        //Or send a Update::lock if lock-margin time has passed
        if let (room, Some(index)) = self.0.listen::<SendMessage>(ctx).await {
            let message = room.confirmed().unwrap().messages.get(index).unwrap().clone();
            if message.author == ctx.me() && !message.body.contains("ChatBot") {
                room.apply(SendMessage(format!("ChatBot Replying to \"{:.10}...\": I totally agree", message.body)));
            }
        }
    }
    async fn shutdown(self, ctx: &mut air::Context) {
        for mut room in ctx.list::<Room>() {
            room.apply(SendMessage("ChatBot Shutting Down".to_string())).wait_confirmed().await;
        }
        println!("CHATBOT SHUTDOWN");
    }
}
