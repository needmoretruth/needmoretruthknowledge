//! The opening holdings, written once and handed to all three models.
//!
//! The same list becomes a set of unspent outputs, a map of accounts, and a map of objects. Where
//! the models disagree is already visible here: three coins of ten for one address stay three
//! separate things in the UTXO and object models, and collapse into one balance of thirty in the
//! account model.

use crate::crypto::put_len;
use crate::{
    AccountLedger, Address, Amount, Ledger, Model, ObjectLedger, TxId, UtxoLedger, crypto,
};

const DOMAIN_GENESIS: &[u8] = b"nmtk.ledger.genesis.v1";

/// One opening holding.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GenesisCoin {
    /// Who holds it. For a shared coin this is the address a screen calls the pool by.
    pub owner: Address,
    /// How much.
    pub value: Amount,
    /// Whether the object model should make this a shared object rather than an owned one. The
    /// other two models treat it as an ordinary holding, which is the comparison worth seeing.
    pub shared: bool,
}

/// The opening state, before any transfer.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Genesis {
    coins: Vec<GenesisCoin>,
}

impl Genesis {
    /// Nothing held by anybody.
    pub fn new() -> Self {
        Self::default()
    }

    /// Gives an address these coins, in this order. In the account model they add up to one
    /// balance; in the other two they stay separate and can be spent separately.
    pub fn holding(mut self, owner: Address, coins: &[Amount]) -> Self {
        for value in coins {
            self.coins.push(GenesisCoin { owner, value: *value, shared: false });
        }
        self
    }

    /// Marks an address as a shared pool, holding this much.
    ///
    /// Only the object model treats it as shared; the UTXO and account models see an ordinary
    /// holding. Two senders paying into it can run at the same time in those two models and
    /// cannot in the object model, which is the clearest place the three come apart.
    pub fn shared_pool(mut self, owner: Address, balance: Amount) -> Self {
        self.coins.push(GenesisCoin { owner, value: balance, shared: true });
        self
    }

    /// Every opening coin, in the order it was declared.
    pub fn coins(&self) -> &[GenesisCoin] {
        &self.coins
    }

    /// Everything held by everybody. Valid transfers never change it.
    pub fn total_supply(&self) -> Amount {
        self.coins.iter().fold(0u64, |total, coin| total.saturating_add(coin.value))
    }

    /// The id of the one transaction that created every opening holding.
    pub fn txid(&self) -> TxId {
        let mut buffer = Vec::with_capacity(16 + self.coins.len() * 32);
        put_len(&mut buffer, self.coins.len());
        for coin in &self.coins {
            buffer.extend_from_slice(coin.owner.as_bytes());
            buffer.extend_from_slice(&coin.value.to_be_bytes());
            buffer.push(u8::from(coin.shared));
        }
        TxId::from_bytes(crypto::hash(&[DOMAIN_GENESIS, &buffer]))
    }

    /// One model, opened at these holdings.
    pub fn ledger(&self, model: Model) -> Box<dyn Ledger> {
        match model {
            Model::Utxo => Box::new(UtxoLedger::from_genesis(self)),
            Model::Account => Box::new(AccountLedger::from_genesis(self)),
            Model::Object => Box::new(ObjectLedger::from_genesis(self)),
        }
    }
}
