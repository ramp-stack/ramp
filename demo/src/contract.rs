use ramp::maverick_os::air::{Contract, Substance, Id, Reactants, Reactant, Beaker, Name};

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::convert::Infallible;

use serde::{Serialize, Deserialize};


#[derive(Serialize, Deserialize, Hash)]
pub struct ChatRoom;

impl ChatRoom {
    pub fn new(_name: &str) -> Self { ChatRoom }
}

impl Contract for ChatRoom {
    fn id() -> Id { Id::hash("ChatRoom2.5") }

    fn init(self, signer: &Name, _timestamp: u64) -> Substance {
        Substance::Map(BTreeMap::from([
            ("name".to_string(),     Substance::String("myroom".to_string())),
            ("author".to_string(),   Substance::String(signer.to_string())),
            ("messages".to_string(), Substance::map()),
        ]))
    }

    fn routes() -> BTreeMap<PathBuf, Reactants> {
        BTreeMap::from([
            (PathBuf::from("/name"),     Reactants::new().add::<ChangeName>()),
            (PathBuf::from("/messages"), Reactants::new().add::<SendMessage>()),
        ])
    }
}


#[derive(Serialize, Deserialize, Hash)]
pub struct ChangeName(String);

impl Reactant for ChangeName {
    type Error = Infallible;
    type Contract = ChatRoom;

    fn apply<B: Beaker>(self, _path: &Path, signer: &Name, _timestamp: u64, substance: &mut B) -> Result<(), Self::Error> {
        if substance.query("/author") == Ok(Substance::String(signer.to_string())) {
            let _ = substance.insert("/name", Substance::String(self.0));
        }
        Ok(())
    }
}


#[derive(Serialize, Deserialize, Hash)]
pub struct SendMessage(String);

impl Reactant for SendMessage {
    type Error = Infallible;
    type Contract = ChatRoom;

    fn apply<B: Beaker>(self, _path: &Path, signer: &Name, timestamp: u64, substance: &mut B) -> Result<(), Self::Error> {
        let _ = substance.insert("/messages/-", Substance::Map(BTreeMap::from([
            ("author".to_string(),    Substance::String(signer.to_string())),
            ("timestamp".to_string(), Substance::Integer(timestamp as i64)),
            ("body".to_string(),      Substance::String(self.0)),
        ])));
        Ok(())
    }
}