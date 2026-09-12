//! One scenario, three ledger models: UTXO, account-based and object-based.
//!
//! A reader writes one transfer and watches it land in all three models at once — UTXO as
//! Bitcoin records it, accounts as Ethereum records them, objects as Sui records them — then
//! tries to spend the same money twice and sees each model refuse for a different reason at a
//! different step.
//!
//! Everything here is data. The crate never produces a word of text: the screen crate reads these
//! enums and structs and decides how to say them, in whichever language is switched on.
//!
//! Where to start: [`Genesis`] sets up the opening holdings, [`Scenario`] drives all three
//! models from one [`TransferRequest`], and [`Ledger`] is what a single model offers.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod account;
mod crypto;
mod genesis;
mod object;
mod scenario;
mod utxo;

#[cfg(test)]
mod tests;

pub use account::{AccountEntry, AccountLedger, AccountTx};
pub use crypto::{ADDRESS_BYTES, Address, Key, Signature};
pub use genesis::{Genesis, GenesisCoin};
pub use object::{
    ObjectEntry, ObjectLedger, ObjectRef, ObjectTx, Ownership, TransferTarget, Version,
};
pub use scenario::{DoubleSpendReport, Scenario, SideBySide};
pub use utxo::{TxOut, UtxoInput, UtxoLedger, UtxoTx};

/// A quantity of value. One unit is the smallest the ledger can move — a satoshi, a wei, a MIST.
pub type Amount = u64;

/// Which of the three ways of recording ownership.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Model {
    /// Bitcoin-style: state is a set of unspent outputs, each spendable exactly once.
    Utxo,
    /// Ethereum-style: state is a map from address to balance and nonce.
    Account,
    /// Sui-style: state is a map from object id to owner, version and value.
    Object,
}

impl Model {
    /// All three, in the order a screen shows them.
    pub const ALL: [Model; 3] = [Model::Utxo, Model::Account, Model::Object];
}

/// Bytes one entry of the unspent-output set occupies: a 32-byte transaction id, a 4-byte output
/// index, an 8-byte value and a 20-byte owner.
pub const UTXO_ENTRY_BYTES: u64 = 64;

/// Bytes one account entry occupies: a 20-byte address, an 8-byte balance and an 8-byte nonce.
pub const ACCOUNT_ENTRY_BYTES: u64 = 36;

/// Bytes one object entry occupies: a 32-byte id, a 20-byte owner, an 8-byte version, an 8-byte
/// value and one byte saying whether the object is owned or shared.
pub const OBJECT_ENTRY_BYTES: u64 = 69;

/// The identifier of a transaction: the hash of everything it says except its signatures.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct TxId([u8; 32]);

impl TxId {
    /// Wraps bytes that are already a transaction id.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The raw bytes, for a screen that wants to show a short prefix.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// The identifier of an object in the object model.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct ObjectId([u8; 32]);

impl ObjectId {
    /// Wraps bytes that are already an object id.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The raw bytes, for a screen that wants to show a short prefix.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// One output of one transaction: the thing a UTXO ledger spends.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct OutPoint {
    /// The transaction that created the output.
    pub txid: TxId,
    /// Its position among that transaction's outputs.
    pub index: u32,
}

/// One addressable entry of state, named the way its own model names it.
///
/// This is what makes the three models comparable: a transfer touches entries, and two transfers
/// that touch no entry in common could have run at the same time, whichever model they are in.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum EntryKey {
    /// An entry of the unspent-output set.
    Output(OutPoint),
    /// An account's balance and nonce.
    Account(Address),
    /// One object.
    Object(ObjectId),
}

/// What a model's state is made of.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum EntryKind {
    /// Unspent outputs, added and removed as coins move.
    UnspentOutput,
    /// Accounts, one per address that has ever held value.
    Account,
    /// Objects, each with an owner and a version.
    Object,
}

/// Something a holder can spend, with the key a screen shows beside it.
///
/// The list this comes from is the model's answer to "where do you read a balance from": one
/// entry per coin in the UTXO model, exactly one entry in the account model, one entry per object
/// in the object model.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Holding {
    /// The state entry holding the value.
    pub entry: EntryKey,
    /// How much sits in it.
    pub value: Amount,
}

/// The step of validation a transaction died at.
///
/// The order of the variants is the order the checks run in, so a screen can line the three
/// models up against each other and show where each one stops.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum CheckStep {
    /// Before a transaction exists: the sender could not even assemble one.
    Build,
    /// The transaction read on its own: amounts above zero, no input named twice.
    Shape,
    /// Looking every input, account or object up in the current state.
    StateLookup,
    /// Checking that whoever signed is allowed to move what is being moved.
    Authorization,
    /// Checking that the transaction was built against the state as it is now: the nonce the
    /// account expects, the version the object is at.
    Freshness,
    /// Checking that the numbers add up.
    Value,
}

/// Why a transaction was refused.
///
/// Variants, never sentences: the wording on screen belongs to the screen crate.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Rejection {
    /// A transfer of nothing, or an output worth nothing.
    ZeroAmount,
    /// A transaction with nothing to spend.
    NoInputs,
    /// More outputs than [`MAX_OUTPUTS_PER_TX`].
    TooManyOutputs {
        /// How many the transaction asked for.
        outputs: usize,
        /// The limit it passed.
        limit: usize,
    },
    /// The same entry named twice by one transaction.
    DuplicateInput(EntryKey),
    /// An input that the unspent-output set does not hold: either it never existed, or it was
    /// spent already. This is how the UTXO model catches a double spend.
    InputNotFound(OutPoint),
    /// The signature does not cover this exact transaction.
    BadSignature,
    /// The signer does not own what the transaction moves.
    NotOwner {
        /// Who signed.
        signer: Address,
    },
    /// A UTXO transaction whose outputs do not total its inputs.
    ValueNotConserved {
        /// What the inputs are worth.
        inputs: Amount,
        /// What the outputs ask for.
        outputs: Amount,
    },
    /// Adding two amounts would run past the end of the number.
    AmountOverflow,
    /// The account model has no entry for this address, so it has never held value.
    AccountNotFound(Address),
    /// The transaction's nonce is not the one this account is waiting for. This is how the
    /// account model catches a double spend: the second copy carries a nonce already used.
    NonceMismatch {
        /// The nonce the account will accept next.
        expected: u64,
        /// The nonce the transaction carried.
        found: u64,
    },
    /// There is not enough to send.
    InsufficientBalance {
        /// What the sender holds.
        available: Amount,
        /// What the transfer needs.
        needed: Amount,
    },
    /// No object with this id.
    ObjectNotFound(ObjectId),
    /// A transfer aimed at an object that is not shared, or spending a shared object as if it
    /// were owned.
    NotSharedObject(ObjectId),
    /// The transaction names a version the object has moved past. This is how the object model
    /// catches a double spend.
    StaleObjectVersion {
        /// The version the object is at now.
        expected: Version,
        /// The version the transaction was built against.
        found: Version,
    },
    /// The sender holds nothing at all in this model.
    NoCoinAvailable,
    /// The coin the sender picked is smaller than the transfer.
    CoinTooSmall {
        /// What that coin is worth.
        coin: Amount,
        /// What the transfer needs.
        needed: Amount,
    },
    /// The sender picked a coin they do not have.
    NoSuchCoin {
        /// The index asked for.
        index: usize,
        /// How many holdings the sender actually has.
        held: usize,
    },
    /// A transaction built for one model handed to another. A caller mistake, not a ledger event.
    ModelMismatch,
}

/// A rejection with its payload stripped off, so a screen can compare reasons across models and
/// a model can state up front which reason its double spend will hit.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum RejectionKind {
    /// See [`Rejection::ZeroAmount`].
    ZeroAmount,
    /// See [`Rejection::NoInputs`].
    NoInputs,
    /// See [`Rejection::TooManyOutputs`].
    TooManyOutputs,
    /// See [`Rejection::DuplicateInput`].
    DuplicateInput,
    /// See [`Rejection::InputNotFound`].
    InputNotFound,
    /// See [`Rejection::BadSignature`].
    BadSignature,
    /// See [`Rejection::NotOwner`].
    NotOwner,
    /// See [`Rejection::ValueNotConserved`].
    ValueNotConserved,
    /// See [`Rejection::AmountOverflow`].
    AmountOverflow,
    /// See [`Rejection::AccountNotFound`].
    AccountNotFound,
    /// See [`Rejection::NonceMismatch`].
    NonceMismatch,
    /// See [`Rejection::InsufficientBalance`].
    InsufficientBalance,
    /// See [`Rejection::ObjectNotFound`].
    ObjectNotFound,
    /// See [`Rejection::NotSharedObject`].
    NotSharedObject,
    /// See [`Rejection::StaleObjectVersion`].
    StaleObjectVersion,
    /// See [`Rejection::NoCoinAvailable`].
    NoCoinAvailable,
    /// See [`Rejection::CoinTooSmall`].
    CoinTooSmall,
    /// See [`Rejection::NoSuchCoin`].
    NoSuchCoin,
    /// See [`Rejection::ModelMismatch`].
    ModelMismatch,
}

impl Rejection {
    /// This rejection without its numbers.
    pub fn kind(&self) -> RejectionKind {
        match self {
            Rejection::ZeroAmount => RejectionKind::ZeroAmount,
            Rejection::NoInputs => RejectionKind::NoInputs,
            Rejection::TooManyOutputs { .. } => RejectionKind::TooManyOutputs,
            Rejection::DuplicateInput(_) => RejectionKind::DuplicateInput,
            Rejection::InputNotFound(_) => RejectionKind::InputNotFound,
            Rejection::BadSignature => RejectionKind::BadSignature,
            Rejection::NotOwner { .. } => RejectionKind::NotOwner,
            Rejection::ValueNotConserved { .. } => RejectionKind::ValueNotConserved,
            Rejection::AmountOverflow => RejectionKind::AmountOverflow,
            Rejection::AccountNotFound(_) => RejectionKind::AccountNotFound,
            Rejection::NonceMismatch { .. } => RejectionKind::NonceMismatch,
            Rejection::InsufficientBalance { .. } => RejectionKind::InsufficientBalance,
            Rejection::ObjectNotFound(_) => RejectionKind::ObjectNotFound,
            Rejection::NotSharedObject(_) => RejectionKind::NotSharedObject,
            Rejection::StaleObjectVersion { .. } => RejectionKind::StaleObjectVersion,
            Rejection::NoCoinAvailable => RejectionKind::NoCoinAvailable,
            Rejection::CoinTooSmall { .. } => RejectionKind::CoinTooSmall,
            Rejection::NoSuchCoin { .. } => RejectionKind::NoSuchCoin,
            Rejection::ModelMismatch => RejectionKind::ModelMismatch,
        }
    }
}

/// How many outputs one UTXO transaction may carry. Real chains cap this too; here it also keeps
/// the output index inside a `u32` no matter what a caller hands in.
pub const MAX_OUTPUTS_PER_TX: usize = 64;

/// The entries a transaction reads and the entries it writes.
///
/// A write means created, changed or deleted. Two transactions may run at the same time exactly
/// when neither writes an entry the other touches.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct TouchedEntries {
    reads: Vec<EntryKey>,
    writes: Vec<EntryKey>,
}

impl TouchedEntries {
    /// Sorts and de-duplicates, so the counts are counts of distinct entries.
    pub(crate) fn new(mut reads: Vec<EntryKey>, mut writes: Vec<EntryKey>) -> Self {
        reads.sort_unstable();
        reads.dedup();
        writes.sort_unstable();
        writes.dedup();
        Self { reads, writes }
    }

    /// The entries read, sorted.
    pub fn reads(&self) -> &[EntryKey] {
        &self.reads
    }

    /// The entries written, sorted.
    pub fn writes(&self) -> &[EntryKey] {
        &self.writes
    }

    /// How many distinct entries are read.
    pub fn read_count(&self) -> usize {
        self.reads.len()
    }

    /// How many distinct entries are written.
    pub fn write_count(&self) -> usize {
        self.writes.len()
    }

    /// Whether these two sets of entries let their transactions run at the same time.
    pub fn conflict(&self, other: &Self) -> Conflict {
        let write_write = intersect(&self.writes, &other.writes);
        if !write_write.is_empty() {
            return Conflict::WriteWrite(write_write);
        }
        let mut read_write = intersect(&self.writes, &other.reads);
        read_write.extend(intersect(&self.reads, &other.writes));
        read_write.sort_unstable();
        read_write.dedup();
        if !read_write.is_empty() {
            return Conflict::ReadWrite(read_write);
        }
        Conflict::Independent
    }
}

/// Both lists are sorted, so membership is a binary search.
fn intersect(left: &[EntryKey], right: &[EntryKey]) -> Vec<EntryKey> {
    left.iter().filter(|key| right.binary_search(key).is_ok()).copied().collect()
}

/// Whether two transactions could execute at the same time, and if not, on what.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Conflict {
    /// No entry in common. A scheduler may run both at once, in either order, on either machine.
    Independent,
    /// Both write the same entry. Whichever lands second sees a state the other has changed.
    WriteWrite(Vec<EntryKey>),
    /// One writes an entry the other reads.
    ReadWrite(Vec<EntryKey>),
    /// The two transactions belong to different models and cannot be compared.
    ModelMismatch,
}

impl Conflict {
    /// Whether the pair can run in parallel.
    pub fn is_parallel_safe(&self) -> bool {
        matches!(self, Conflict::Independent)
    }

    /// The entries the pair collides on, empty when it does not.
    pub fn entries(&self) -> &[EntryKey] {
        match self {
            Conflict::WriteWrite(entries) | Conflict::ReadWrite(entries) => entries,
            Conflict::Independent | Conflict::ModelMismatch => &[],
        }
    }
}

/// What became of a transaction.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ApplyResult {
    /// It went in, and this is its id.
    Accepted(TxId),
    /// It was refused, at this step, for this reason.
    Rejected {
        /// Where validation stopped.
        step: CheckStep,
        /// Why it stopped there.
        reason: Rejection,
    },
}

/// Everything one attempt says about the model that handled it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ApplyOutcome {
    /// Which model produced this.
    pub model: Model,
    /// Accepted, or refused at a named step.
    pub result: ApplyResult,
    /// The entries the transaction reads and writes — reported whether or not it was accepted,
    /// because "what would this have touched" is half the parallelism story.
    pub touched: TouchedEntries,
    /// Entries in state before the attempt.
    pub entries_before: usize,
    /// Entries in state after it. Unchanged when the transaction was refused.
    pub entries_after: usize,
    /// State size in bytes before the attempt.
    pub size_before: u64,
    /// State size in bytes after it.
    pub size_after: u64,
}

impl ApplyOutcome {
    /// Whether the transaction went in.
    pub fn accepted(&self) -> bool {
        matches!(self.result, ApplyResult::Accepted(_))
    }

    /// The step and reason, when it was refused.
    pub fn rejection(&self) -> Option<(CheckStep, Rejection)> {
        match self.result {
            ApplyResult::Rejected { step, reason } => Some((step, reason)),
            ApplyResult::Accepted(_) => None,
        }
    }

    /// The transaction id, when it went in.
    pub fn txid(&self) -> Option<TxId> {
        match self.result {
            ApplyResult::Accepted(txid) => Some(txid),
            ApplyResult::Rejected { .. } => None,
        }
    }

    /// How many entries state gained, negative when it shrank.
    pub fn entry_delta(&self) -> i64 {
        difference(self.entries_after, self.entries_before)
    }

    /// How many bytes state gained, negative when it shrank.
    pub fn size_delta(&self) -> i64 {
        let after = i64::try_from(self.size_after).unwrap_or(i64::MAX);
        let before = i64::try_from(self.size_before).unwrap_or(i64::MAX);
        after.saturating_sub(before)
    }

    pub(crate) fn rejected(
        model: Model,
        step: CheckStep,
        reason: Rejection,
        touched: TouchedEntries,
        entries: usize,
        size: u64,
    ) -> Self {
        Self {
            model,
            result: ApplyResult::Rejected { step, reason },
            touched,
            entries_before: entries,
            entries_after: entries,
            size_before: size,
            size_after: size,
        }
    }
}

fn difference(after: usize, before: usize) -> i64 {
    let after = i64::try_from(after).unwrap_or(i64::MAX);
    let before = i64::try_from(before).unwrap_or(i64::MAX);
    after.saturating_sub(before)
}

/// The facts about a model that a screen can show before anything has been run.
///
/// The two parallelism flags are the model's rule, not a reading of one particular pair; for a
/// concrete pair, ask [`Ledger::conflicts_with`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ModelFacts {
    /// Which model.
    pub model: Model,
    /// What its state is made of.
    pub entry_kind: EntryKind,
    /// Bytes one entry occupies.
    pub entry_bytes: u64,
    /// Entries held right now.
    pub entry_count: usize,
    /// Bytes held right now.
    pub state_size_bytes: u64,
    /// The step at which this model catches a coin being spent twice.
    pub double_spend_step: CheckStep,
    /// The reason it gives.
    pub double_spend_kind: RejectionKind,
    /// Whether one sender can have two transfers in flight at the same time.
    pub parallel_from_one_sender: bool,
    /// Whether two senders can pay the same recipient at the same time.
    pub parallel_to_one_recipient: bool,
    /// Whether the model has state that several senders must queue behind. Only the object model
    /// does, and only for the objects marked shared.
    pub has_shared_state: bool,
}

/// Which of a sender's holdings to spend.
///
/// The account model ignores this, and that is the lesson: an account is one balance, so there is
/// nothing to choose and nothing to spend in parallel.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CoinChoice {
    /// Let the model pick: the first holdings, in the order [`Ledger::holdings_of`] lists them,
    /// that together cover the transfer.
    #[default]
    Automatic,
    /// Spend the holding at this position in [`Ledger::holdings_of`].
    ///
    /// The position is into that model's own list, and the three models do not order a sender's
    /// holdings the same way, so the same index can name different coins in different models. A
    /// screen offering the choice should offer each model's own list.
    Index(usize),
}

/// One transfer, written once and handed to all three models.
///
/// `from` is a key rather than an address because sending needs the power to sign; the sender's
/// address is [`TransferRequest::sender`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TransferRequest {
    /// The sender's key.
    pub from: Key,
    /// Where the value goes.
    pub to: Address,
    /// How much.
    pub amount: Amount,
    /// Which holding to spend.
    pub coin: CoinChoice,
}

impl TransferRequest {
    /// A transfer that lets each model pick the coins.
    pub fn new(from: Key, to: Address, amount: Amount) -> Self {
        Self { from, to, amount, coin: CoinChoice::Automatic }
    }

    /// The same transfer, spending a named holding.
    pub fn with_coin(mut self, coin: CoinChoice) -> Self {
        self.coin = coin;
        self
    }

    /// The sender's address.
    pub fn sender(&self) -> Address {
        self.from.address()
    }
}

/// A transaction, in whichever shape its model gives it.
///
/// The shapes differ because the models differ: a UTXO transaction names outputs to consume and
/// outputs to create, an account transaction names a nonce, an object transaction names a version.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Transaction {
    /// A Bitcoin-shaped transaction.
    Utxo(UtxoTx),
    /// An Ethereum-shaped transaction.
    Account(AccountTx),
    /// A Sui-shaped transaction.
    Object(ObjectTx),
}

impl Transaction {
    /// Which model this was built for.
    pub fn model(&self) -> Model {
        match self {
            Transaction::Utxo(_) => Model::Utxo,
            Transaction::Account(_) => Model::Account,
            Transaction::Object(_) => Model::Object,
        }
    }

    /// Its id: the hash of everything it says except its signatures.
    pub fn id(&self) -> TxId {
        match self {
            Transaction::Utxo(tx) => tx.id(),
            Transaction::Account(tx) => tx.id(),
            Transaction::Object(tx) => tx.id(),
        }
    }
}

/// What one ledger model offers.
///
/// Object-safe on purpose: a screen can hold the three models in one array and walk them.
pub trait Ledger {
    /// Which model this is.
    fn model(&self) -> Model;

    /// What its state is made of.
    fn entry_kind(&self) -> EntryKind;

    /// Bytes one entry occupies.
    fn entry_bytes(&self) -> u64;

    /// Entries held right now.
    fn entry_count(&self) -> usize;

    /// Turns a transfer into a transaction of this model's shape, against state as it is now.
    ///
    /// Building does not change anything, so two transactions built before either is applied are
    /// both built against the same state — which is how a double spend is staged.
    fn build_transfer(&self, request: &TransferRequest) -> Result<Transaction, Rejection>;

    /// Validates a transaction and, if it passes every step, writes it into state.
    fn apply(&mut self, tx: &Transaction) -> ApplyOutcome;

    /// What the sender holds, in the units this model holds it in.
    fn holdings_of(&self, who: &Address) -> Vec<Holding>;

    /// What an address is worth.
    fn balance_of(&self, who: &Address) -> Amount;

    /// The entries this transaction reads and writes, worked out against state as it is now.
    fn touched_entries(&self, tx: &Transaction) -> TouchedEntries;

    /// Bytes of state.
    ///
    /// This counts the entries a node must keep to validate the next transaction; it is not the
    /// size of the Rust values in memory.
    fn state_size_bytes(&self) -> u64 {
        // usize to u64 loses nothing on any target this builds for.
        self.entry_bytes().saturating_mul(self.entry_count() as u64)
    }

    /// Whether these two transactions could run at the same time in this model.
    fn conflicts_with(&self, a: &Transaction, b: &Transaction) -> Conflict {
        if a.model() != self.model() || b.model() != self.model() {
            return Conflict::ModelMismatch;
        }
        self.touched_entries(a).conflict(&self.touched_entries(b))
    }

    /// Build and apply in one go, so a build failure comes back shaped like every other refusal.
    fn submit(&mut self, request: &TransferRequest) -> ApplyOutcome {
        match self.build_transfer(request) {
            Ok(tx) => self.apply(&tx),
            Err(reason) => ApplyOutcome::rejected(
                self.model(),
                CheckStep::Build,
                reason,
                TouchedEntries::default(),
                self.entry_count(),
                self.state_size_bytes(),
            ),
        }
    }

    /// The facts that separate this model from the other two.
    fn facts(&self) -> ModelFacts;
}
