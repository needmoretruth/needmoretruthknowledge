//! The account model, as Ethereum records ownership.
//!
//! State is a map from address to balance and nonce. A transfer debits one entry and credits
//! another; the balance is read straight out of the entry rather than computed from a set of
//! coins. Nothing is consumed, so nothing is missing the second time a transfer is submitted —
//! what stops the replay is the nonce, a counter the account keeps and the transaction pins.
//! That is why the second spend dies at [`CheckStep::Freshness`] rather than at a lookup.
//!
//! The account model ignores [`crate::CoinChoice`]: an account has one balance, so there is no coin to
//! choose. That is the lesson, not an omission — it is also why one sender cannot have two
//! transfers running at the same time here.

use std::collections::BTreeMap;

use crate::crypto::{Signature, put_len};
use crate::genesis::Genesis;
use crate::{
    ACCOUNT_ENTRY_BYTES, Address, Amount, ApplyOutcome, ApplyResult, CheckStep, EntryKey,
    EntryKind, Holding, Ledger, Model, ModelFacts, Rejection, RejectionKind, TouchedEntries,
    Transaction, TransferRequest, TxId, crypto,
};

const DOMAIN_TX: &[u8] = b"nmtk.ledger.account.tx.v1";

/// What the ledger keeps for one address.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct AccountEntry {
    /// What the address is worth.
    pub balance: Amount,
    /// The number the next transaction from this address must carry.
    pub nonce: u64,
}

/// An Ethereum-shaped transaction: move this much from here to there, as transfer number `nonce`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AccountTx {
    /// Who pays.
    pub from: Address,
    /// Who is paid.
    pub to: Address,
    /// How much.
    pub amount: Amount,
    /// Which transfer of the sender's this is. Reusing one is how a replay is caught.
    pub nonce: u64,
    /// Proof the sender agreed to exactly this.
    pub signature: Signature,
}

impl AccountTx {
    /// The hash the signature covers, which is also the transaction's id.
    pub fn signing_message(&self) -> [u8; 32] {
        signing_message(&self.from, &self.to, self.amount, self.nonce)
    }

    /// The transaction id.
    pub fn id(&self) -> TxId {
        TxId::from_bytes(self.signing_message())
    }
}

fn signing_message(from: &Address, to: &Address, amount: Amount, nonce: u64) -> [u8; 32] {
    let mut buffer = Vec::with_capacity(64);
    put_len(&mut buffer, 1);
    buffer.extend_from_slice(from.as_bytes());
    buffer.extend_from_slice(to.as_bytes());
    buffer.extend_from_slice(&amount.to_be_bytes());
    buffer.extend_from_slice(&nonce.to_be_bytes());
    crypto::hash(&[DOMAIN_TX, &buffer])
}

/// A map from address to balance and nonce, and nothing else.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct AccountLedger {
    accounts: BTreeMap<Address, AccountEntry>,
}

impl AccountLedger {
    /// An empty map.
    pub fn new() -> Self {
        Self::default()
    }

    /// The opening holdings. Several genesis coins for one address collapse into one balance,
    /// which is the first thing this model loses compared with the other two.
    pub fn from_genesis(genesis: &Genesis) -> Self {
        let mut accounts: BTreeMap<Address, AccountEntry> = BTreeMap::new();
        for coin in genesis.coins() {
            let entry = accounts.entry(coin.owner).or_default();
            entry.balance = entry.balance.saturating_add(coin.value);
        }
        Self { accounts }
    }

    /// Every account, in address order.
    pub fn accounts(&self) -> impl Iterator<Item = (&Address, &AccountEntry)> {
        self.accounts.iter()
    }

    /// One account, if it has ever held value.
    pub fn account(&self, who: &Address) -> Option<&AccountEntry> {
        self.accounts.get(who)
    }

    /// The nonce this address's next transaction must carry.
    pub fn next_nonce(&self, who: &Address) -> u64 {
        self.accounts.get(who).map_or(0, |entry| entry.nonce)
    }

    fn touched(&self, tx: &AccountTx) -> TouchedEntries {
        // Crediting the recipient is a read and a write of the recipient's entry, which is why
        // two people paying the same person cannot be run at the same time in this model.
        let entries = vec![EntryKey::Account(tx.from), EntryKey::Account(tx.to)];
        TouchedEntries::new(entries.clone(), entries)
    }

    fn validate(&self, tx: &AccountTx) -> Result<(), (CheckStep, Rejection)> {
        if tx.amount == 0 {
            return Err((CheckStep::Shape, Rejection::ZeroAmount));
        }

        let sender = self
            .accounts
            .get(&tx.from)
            .ok_or((CheckStep::StateLookup, Rejection::AccountNotFound(tx.from)))?;

        let signer = tx.signature.signer();
        if signer != tx.from {
            return Err((CheckStep::Authorization, Rejection::NotOwner { signer }));
        }
        if !tx.signature.covers(&tx.signing_message()) {
            return Err((CheckStep::Authorization, Rejection::BadSignature));
        }

        // Nothing was consumed by the first copy of this transfer, so only the counter can tell
        // the ledger it has seen this one before.
        if tx.nonce != sender.nonce {
            return Err((
                CheckStep::Freshness,
                Rejection::NonceMismatch { expected: sender.nonce, found: tx.nonce },
            ));
        }

        if sender.balance < tx.amount {
            return Err((
                CheckStep::Value,
                Rejection::InsufficientBalance { available: sender.balance, needed: tx.amount },
            ));
        }
        if tx.from != tx.to {
            let recipient = self.accounts.get(&tx.to).map_or(0, |entry| entry.balance);
            if recipient.checked_add(tx.amount).is_none() {
                return Err((CheckStep::Value, Rejection::AmountOverflow));
            }
        }
        Ok(())
    }
}

impl Ledger for AccountLedger {
    fn model(&self) -> Model {
        Model::Account
    }

    fn entry_kind(&self) -> EntryKind {
        EntryKind::Account
    }

    fn entry_bytes(&self) -> u64 {
        ACCOUNT_ENTRY_BYTES
    }

    fn entry_count(&self) -> usize {
        self.accounts.len()
    }

    fn build_transfer(&self, request: &TransferRequest) -> Result<Transaction, Rejection> {
        if request.amount == 0 {
            return Err(Rejection::ZeroAmount);
        }
        let sender = request.sender();
        let entry = self.accounts.get(&sender).ok_or(Rejection::AccountNotFound(sender))?;
        // request.coin is deliberately unread: see the note at the top of this file.
        let nonce = entry.nonce;
        let signature =
            request.from.sign(&signing_message(&sender, &request.to, request.amount, nonce));
        Ok(Transaction::Account(AccountTx {
            from: sender,
            to: request.to,
            amount: request.amount,
            nonce,
            signature,
        }))
    }

    fn apply(&mut self, tx: &Transaction) -> ApplyOutcome {
        let entries_before = self.accounts.len();
        let size_before = self.state_size_bytes();
        let Transaction::Account(tx) = tx else {
            return ApplyOutcome::rejected(
                Model::Account,
                CheckStep::Shape,
                Rejection::ModelMismatch,
                TouchedEntries::default(),
                entries_before,
                size_before,
            );
        };
        let touched = self.touched(tx);
        if let Err((step, reason)) = self.validate(tx) {
            return ApplyOutcome::rejected(
                Model::Account,
                step,
                reason,
                touched,
                entries_before,
                size_before,
            );
        }

        if tx.from == tx.to {
            // Paying yourself moves nothing. Debiting and crediting through two copies of the
            // same entry is how a ledger mints money by accident, so it is done as one step.
            let sender = self.accounts.entry(tx.from).or_default();
            sender.nonce = sender.nonce.saturating_add(1);
        } else {
            let sender = self.accounts.entry(tx.from).or_default();
            sender.balance = sender.balance.saturating_sub(tx.amount);
            sender.nonce = sender.nonce.saturating_add(1);
            let recipient = self.accounts.entry(tx.to).or_default();
            recipient.balance = recipient.balance.saturating_add(tx.amount);
        }

        ApplyOutcome {
            model: Model::Account,
            result: ApplyResult::Accepted(tx.id()),
            touched,
            entries_before,
            entries_after: self.accounts.len(),
            size_before,
            size_after: self.state_size_bytes(),
        }
    }

    fn holdings_of(&self, who: &Address) -> Vec<Holding> {
        // Exactly one, or none. There is nothing to pick between.
        match self.accounts.get(who) {
            Some(entry) => {
                vec![Holding { entry: EntryKey::Account(*who), value: entry.balance }]
            }
            None => Vec::new(),
        }
    }

    fn balance_of(&self, who: &Address) -> Amount {
        self.accounts.get(who).map_or(0, |entry| entry.balance)
    }

    fn touched_entries(&self, tx: &Transaction) -> TouchedEntries {
        match tx {
            Transaction::Account(tx) => self.touched(tx),
            _ => TouchedEntries::default(),
        }
    }

    fn facts(&self) -> ModelFacts {
        ModelFacts {
            model: Model::Account,
            entry_kind: EntryKind::Account,
            entry_bytes: ACCOUNT_ENTRY_BYTES,
            entry_count: self.accounts.len(),
            state_size_bytes: self.state_size_bytes(),
            // The money is still there the second time, so the counter is what catches it.
            double_spend_step: CheckStep::Freshness,
            double_spend_kind: RejectionKind::NonceMismatch,
            // One balance, one nonce: a sender's transfers are a queue by construction.
            parallel_from_one_sender: false,
            // Two payers both write the recipient's single balance entry.
            parallel_to_one_recipient: false,
            has_shared_state: false,
        }
    }
}
