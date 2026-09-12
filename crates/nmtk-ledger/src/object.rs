//! The object model, as Sui records ownership.
//!
//! State is a map from object id to an owner, a version and a value. A transfer does not consume
//! and recreate the way a UTXO does, and it does not adjust a running balance the way an account
//! does: it hands an object to a new owner and bumps its version. The transaction pins the
//! version it was built against, so the second copy of a spend names a version the object has
//! already moved past and dies at [`CheckStep::Freshness`].
//!
//! Versions are also the parallelism story. A transaction over objects only its sender owns names
//! every version it depends on, so a validator can check it against those entries alone and run
//! it beside any transaction that names different ones. A **shared** object breaks that: it
//! belongs to nobody, several senders can aim at it at once, and its version cannot be pinned in
//! advance — the order has to be agreed first. That is why [`TransferTarget::Shared`] carries an
//! id and no version, and why every transaction touching one queues behind the others.

use std::collections::{BTreeMap, BTreeSet};

use crate::crypto::{Signature, put_len};
use crate::genesis::Genesis;
use crate::{
    Address, Amount, ApplyOutcome, ApplyResult, CheckStep, CoinChoice, EntryKey, EntryKind,
    Holding, Ledger, Model, ModelFacts, OBJECT_ENTRY_BYTES, ObjectId, Rejection, RejectionKind,
    TouchedEntries, Transaction, TransferRequest, TxId, crypto,
};

const DOMAIN_TX: &[u8] = b"nmtk.ledger.object.tx.v1";
const DOMAIN_GENESIS_OBJECT: &[u8] = b"nmtk.ledger.object.genesis.v1";
const DOMAIN_NEW_OBJECT: &[u8] = b"nmtk.ledger.object.created.v1";

/// How many times an object has changed. Every change bumps it by one.
pub type Version = u64;

/// Who an object belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ownership {
    /// One address may spend it, and a transaction over it can be checked on its own.
    Owned(Address),
    /// Anybody may aim a transaction at it, so the order of those transactions has to be agreed
    /// before any of them runs.
    Shared,
}

/// What the ledger keeps for one object.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ObjectEntry {
    /// What it is worth.
    pub value: Amount,
    /// How many times it has changed.
    pub version: Version,
    /// Who it belongs to.
    pub ownership: Ownership,
}

/// An object named at the version the sender saw.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ObjectRef {
    /// Which object.
    pub id: ObjectId,
    /// The version it was at when the transaction was built.
    pub version: Version,
}

/// Where the value of a transfer goes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TransferTarget {
    /// To one owner. Only the sender's own objects are touched, so the transfer can run beside
    /// any transfer over different objects.
    Owner(Address),
    /// Into a shared object. The id is named but the version is not: a shared object is sequenced
    /// by agreement rather than pinned by the sender, and that is what costs the parallelism.
    Shared(ObjectId),
}

/// A Sui-shaped transaction: spend these objects at these versions, send this much there.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ObjectTx {
    /// The objects being spent, each at the version the sender saw.
    pub inputs: Vec<ObjectRef>,
    /// Where the value goes.
    pub target: TransferTarget,
    /// How much.
    pub amount: Amount,
    /// Proof the sender agreed to exactly this.
    pub signature: Signature,
}

impl ObjectTx {
    /// The hash the signature covers, which is also the transaction's id.
    pub fn signing_message(&self) -> [u8; 32] {
        signing_message(&self.inputs, &self.target, self.amount)
    }

    /// The transaction id. Any object this transaction creates is named after it.
    pub fn id(&self) -> TxId {
        TxId::from_bytes(self.signing_message())
    }
}

fn signing_message(inputs: &[ObjectRef], target: &TransferTarget, amount: Amount) -> [u8; 32] {
    let mut buffer = Vec::with_capacity(64 + inputs.len() * 40);
    put_len(&mut buffer, inputs.len());
    for input in inputs {
        buffer.extend_from_slice(input.id.as_bytes());
        buffer.extend_from_slice(&input.version.to_be_bytes());
    }
    match target {
        TransferTarget::Owner(address) => {
            buffer.push(0);
            buffer.extend_from_slice(address.as_bytes());
        }
        TransferTarget::Shared(id) => {
            buffer.push(1);
            buffer.extend_from_slice(id.as_bytes());
        }
    }
    buffer.extend_from_slice(&amount.to_be_bytes());
    crypto::hash(&[DOMAIN_TX, &buffer])
}

fn created_object_id(txid: TxId) -> ObjectId {
    ObjectId::from_bytes(crypto::hash(&[DOMAIN_NEW_OBJECT, txid.as_bytes()]))
}

fn genesis_object_id(txid: TxId, position: usize) -> ObjectId {
    // Position is widened to u64, which loses nothing on any target this builds for.
    ObjectId::from_bytes(crypto::hash(&[
        DOMAIN_GENESIS_OBJECT,
        txid.as_bytes(),
        &(position as u64).to_be_bytes(),
    ]))
}

/// A map from object id to owner, version and value, plus a note of which address a screen calls
/// each shared object.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ObjectLedger {
    objects: BTreeMap<ObjectId, ObjectEntry>,
    /// The address standing for each shared object, so the same `to` a screen types reaches the
    /// pool here and an ordinary account in the other two models.
    pools: BTreeMap<Address, ObjectId>,
}

impl ObjectLedger {
    /// An empty map.
    pub fn new() -> Self {
        Self::default()
    }

    /// The opening holdings, one object per genesis coin. A coin marked shared becomes a shared
    /// object owned by nobody.
    pub fn from_genesis(genesis: &Genesis) -> Self {
        let txid = genesis.txid();
        let mut objects = BTreeMap::new();
        let mut pools = BTreeMap::new();
        for (position, coin) in genesis.coins().iter().enumerate() {
            let id = genesis_object_id(txid, position);
            let ownership =
                if coin.shared { Ownership::Shared } else { Ownership::Owned(coin.owner) };
            objects.insert(id, ObjectEntry { value: coin.value, version: 0, ownership });
            if coin.shared {
                pools.insert(coin.owner, id);
            }
        }
        Self { objects, pools }
    }

    /// Every object, in id order.
    pub fn objects(&self) -> impl Iterator<Item = (&ObjectId, &ObjectEntry)> {
        self.objects.iter()
    }

    /// One object, if it still exists.
    pub fn object(&self, id: &ObjectId) -> Option<&ObjectEntry> {
        self.objects.get(id)
    }

    /// The shared object standing behind an address, if that address is a pool.
    pub fn shared_pool(&self, who: &Address) -> Option<ObjectId> {
        self.pools.get(who).copied()
    }

    /// Every shared object in this ledger, with the address a screen calls it by.
    pub fn shared_pools(&self) -> impl Iterator<Item = (&Address, &ObjectId)> {
        self.pools.iter()
    }

    /// What the transaction's inputs are worth right now, and what would be left over.
    /// `None` when an input is missing or the numbers do not work.
    fn change_of(&self, tx: &ObjectTx) -> Option<Amount> {
        let mut total: Amount = 0;
        for input in &tx.inputs {
            total = total.checked_add(self.objects.get(&input.id)?.value)?;
        }
        total.checked_sub(tx.amount)
    }

    fn touched(&self, tx: &ObjectTx) -> TouchedEntries {
        let mut reads: Vec<EntryKey> =
            tx.inputs.iter().map(|input| EntryKey::Object(input.id)).collect();
        // Every input is rewritten: one keeps the change, the rest are folded away.
        let mut writes = reads.clone();
        match tx.target {
            TransferTarget::Shared(pool) => {
                reads.push(EntryKey::Object(pool));
                writes.push(EntryKey::Object(pool));
            }
            TransferTarget::Owner(_) => {
                // A transfer of the whole value hands the object over and creates nothing; a
                // transfer of part of it splits a new object off.
                if self.change_of(tx).is_some_and(|change| change > 0) {
                    writes.push(EntryKey::Object(created_object_id(tx.id())));
                }
            }
        }
        TouchedEntries::new(reads, writes)
    }

    fn validate(&self, tx: &ObjectTx) -> Result<Amount, (CheckStep, Rejection)> {
        if tx.amount == 0 {
            return Err((CheckStep::Shape, Rejection::ZeroAmount));
        }
        if tx.inputs.is_empty() {
            return Err((CheckStep::Shape, Rejection::NoInputs));
        }
        let mut seen = BTreeSet::new();
        for input in &tx.inputs {
            if !seen.insert(input.id) {
                return Err((
                    CheckStep::Shape,
                    Rejection::DuplicateInput(EntryKey::Object(input.id)),
                ));
            }
        }

        let mut held = Vec::with_capacity(tx.inputs.len());
        for input in &tx.inputs {
            match self.objects.get(&input.id) {
                Some(entry) => held.push((input, *entry)),
                None => return Err((CheckStep::StateLookup, Rejection::ObjectNotFound(input.id))),
            }
        }
        let pool = match tx.target {
            TransferTarget::Shared(id) => {
                let entry = self
                    .objects
                    .get(&id)
                    .ok_or((CheckStep::StateLookup, Rejection::ObjectNotFound(id)))?;
                if entry.ownership != Ownership::Shared {
                    return Err((CheckStep::StateLookup, Rejection::NotSharedObject(id)));
                }
                Some(*entry)
            }
            TransferTarget::Owner(_) => None,
        };

        let signer = tx.signature.signer();
        for (_, entry) in &held {
            match entry.ownership {
                Ownership::Owned(owner) if owner == signer => {}
                // A shared object cannot be spent as an input here: it is not anybody's to spend.
                _ => return Err((CheckStep::Authorization, Rejection::NotOwner { signer })),
            }
        }
        if !tx.signature.covers(&tx.signing_message()) {
            return Err((CheckStep::Authorization, Rejection::BadSignature));
        }

        // The version is the whole defence. A second spend of the same object was built against
        // the version the first one left behind.
        for (input, entry) in &held {
            if input.version != entry.version {
                return Err((
                    CheckStep::Freshness,
                    Rejection::StaleObjectVersion { expected: entry.version, found: input.version },
                ));
            }
        }

        let mut total: Amount = 0;
        for (_, entry) in &held {
            total = total
                .checked_add(entry.value)
                .ok_or((CheckStep::Value, Rejection::AmountOverflow))?;
        }
        if total < tx.amount {
            return Err((
                CheckStep::Value,
                Rejection::InsufficientBalance { available: total, needed: tx.amount },
            ));
        }
        if let Some(entry) = pool
            && entry.value.checked_add(tx.amount).is_none()
        {
            return Err((CheckStep::Value, Rejection::AmountOverflow));
        }
        Ok(total)
    }
}

impl Ledger for ObjectLedger {
    fn model(&self) -> Model {
        Model::Object
    }

    fn entry_kind(&self) -> EntryKind {
        EntryKind::Object
    }

    fn entry_bytes(&self) -> u64 {
        OBJECT_ENTRY_BYTES
    }

    fn entry_count(&self) -> usize {
        self.objects.len()
    }

    fn build_transfer(&self, request: &TransferRequest) -> Result<Transaction, Rejection> {
        if request.amount == 0 {
            return Err(Rejection::ZeroAmount);
        }
        let sender = request.sender();
        let held: Vec<(ObjectId, ObjectEntry)> = self
            .objects
            .iter()
            .filter(|(_, entry)| entry.ownership == Ownership::Owned(sender))
            .map(|(id, entry)| (*id, *entry))
            .collect();
        if held.is_empty() {
            return Err(Rejection::NoCoinAvailable);
        }

        let chosen: Vec<(ObjectId, ObjectEntry)> = match request.coin {
            CoinChoice::Index(index) => {
                let coin =
                    *held.get(index).ok_or(Rejection::NoSuchCoin { index, held: held.len() })?;
                if coin.1.value < request.amount {
                    return Err(Rejection::CoinTooSmall {
                        coin: coin.1.value,
                        needed: request.amount,
                    });
                }
                vec![coin]
            }
            CoinChoice::Automatic => {
                let mut picked = Vec::new();
                let mut total: Amount = 0;
                for coin in &held {
                    picked.push(*coin);
                    total = total.checked_add(coin.1.value).ok_or(Rejection::AmountOverflow)?;
                    if total >= request.amount {
                        break;
                    }
                }
                if total < request.amount {
                    return Err(Rejection::InsufficientBalance {
                        available: total,
                        needed: request.amount,
                    });
                }
                picked
            }
        };

        // Paying the address a shared object stands behind means paying into that object, which
        // is the one move in this model that cannot run beside another.
        let target = match self.pools.get(&request.to) {
            Some(pool) => TransferTarget::Shared(*pool),
            None => TransferTarget::Owner(request.to),
        };
        let inputs: Vec<ObjectRef> = chosen
            .iter()
            .map(|(id, entry)| ObjectRef { id: *id, version: entry.version })
            .collect();
        let signature = request.from.sign(&signing_message(&inputs, &target, request.amount));
        Ok(Transaction::Object(ObjectTx { inputs, target, amount: request.amount, signature }))
    }

    fn apply(&mut self, tx: &Transaction) -> ApplyOutcome {
        let entries_before = self.objects.len();
        let size_before = self.state_size_bytes();
        let Transaction::Object(tx) = tx else {
            return ApplyOutcome::rejected(
                Model::Object,
                CheckStep::Shape,
                Rejection::ModelMismatch,
                TouchedEntries::default(),
                entries_before,
                size_before,
            );
        };
        let touched = self.touched(tx);
        let total = match self.validate(tx) {
            Ok(total) => total,
            Err((step, reason)) => {
                return ApplyOutcome::rejected(
                    Model::Object,
                    step,
                    reason,
                    touched,
                    entries_before,
                    size_before,
                );
            }
        };
        let Some(first) = tx.inputs.first().map(|input| input.id) else {
            return ApplyOutcome::rejected(
                Model::Object,
                CheckStep::Shape,
                Rejection::NoInputs,
                touched,
                entries_before,
                size_before,
            );
        };

        let txid = tx.id();
        let change = total.saturating_sub(tx.amount);
        // Everything after the first input is folded into it and disappears.
        for input in tx.inputs.iter().skip(1) {
            self.objects.remove(&input.id);
        }
        match tx.target {
            TransferTarget::Owner(to) => {
                if change > 0 {
                    if let Some(entry) = self.objects.get_mut(&first) {
                        entry.value = change;
                        entry.version = entry.version.saturating_add(1);
                    }
                    self.objects.insert(
                        created_object_id(txid),
                        ObjectEntry {
                            value: tx.amount,
                            version: 0,
                            ownership: Ownership::Owned(to),
                        },
                    );
                } else if let Some(entry) = self.objects.get_mut(&first) {
                    // The whole object changes hands. Its id survives; only the owner and the
                    // version move. No entry is created and none is destroyed.
                    entry.ownership = Ownership::Owned(to);
                    entry.version = entry.version.saturating_add(1);
                }
            }
            TransferTarget::Shared(pool) => {
                if change > 0 {
                    if let Some(entry) = self.objects.get_mut(&first) {
                        entry.value = change;
                        entry.version = entry.version.saturating_add(1);
                    }
                } else {
                    self.objects.remove(&first);
                }
                if let Some(entry) = self.objects.get_mut(&pool) {
                    entry.value = entry.value.saturating_add(tx.amount);
                    entry.version = entry.version.saturating_add(1);
                }
            }
        }

        ApplyOutcome {
            model: Model::Object,
            result: ApplyResult::Accepted(txid),
            touched,
            entries_before,
            entries_after: self.objects.len(),
            size_before,
            size_after: self.state_size_bytes(),
        }
    }

    fn holdings_of(&self, who: &Address) -> Vec<Holding> {
        // Only what this address can spend. A shared object is nobody's to spend, so it is not
        // listed here even though its value counts towards the pool address's balance.
        self.objects
            .iter()
            .filter(|(_, entry)| entry.ownership == Ownership::Owned(*who))
            .map(|(id, entry)| Holding { entry: EntryKey::Object(*id), value: entry.value })
            .collect()
    }

    fn balance_of(&self, who: &Address) -> Amount {
        let owned = self
            .objects
            .values()
            .filter(|entry| entry.ownership == Ownership::Owned(*who))
            .fold(0u64, |total, entry| total.saturating_add(entry.value));
        match self.pools.get(who).and_then(|id| self.objects.get(id)) {
            Some(pool) => owned.saturating_add(pool.value),
            None => owned,
        }
    }

    fn touched_entries(&self, tx: &Transaction) -> TouchedEntries {
        match tx {
            Transaction::Object(tx) => self.touched(tx),
            _ => TouchedEntries::default(),
        }
    }

    fn facts(&self) -> ModelFacts {
        ModelFacts {
            model: Model::Object,
            entry_kind: EntryKind::Object,
            entry_bytes: OBJECT_ENTRY_BYTES,
            entry_count: self.objects.len(),
            state_size_bytes: self.state_size_bytes(),
            // The object is still there the second time, but not at the version the second
            // transaction was built against.
            double_spend_step: CheckStep::Freshness,
            double_spend_kind: RejectionKind::StaleObjectVersion,
            // Two objects are two entries with two versions: one sender can move both at once.
            parallel_from_one_sender: true,
            // Being paid means receiving an object, not sharing an entry — unless the recipient
            // is a shared object, which is what has_shared_state warns about.
            parallel_to_one_recipient: true,
            has_shared_state: !self.pools.is_empty(),
        }
    }
}
