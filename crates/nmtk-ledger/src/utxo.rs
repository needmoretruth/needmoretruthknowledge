//! The UTXO model, as Bitcoin records ownership.
//!
//! State is a set of unspent transaction outputs. There is no such thing as a balance in this
//! state: a balance is something you compute by walking the set. A transaction names outputs to
//! consume and outputs to create, and each output can be consumed exactly once, because consuming
//! it removes it from the set. That single fact is what stops a double spend, and it is why the
//! second spend dies at [`CheckStep::StateLookup`] rather than at some later test of freshness.
//!
//! No fees: a transaction's outputs must total its inputs exactly, so the three models can be
//! compared coin for coin.

use std::collections::{BTreeMap, BTreeSet};

use crate::crypto::{Signature, put_len};
use crate::genesis::Genesis;
use crate::{
    ADDRESS_BYTES, Address, Amount, ApplyOutcome, ApplyResult, CheckStep, CoinChoice, EntryKey,
    EntryKind, Holding, Ledger, MAX_OUTPUTS_PER_TX, Model, ModelFacts, OutPoint, Rejection,
    RejectionKind, TouchedEntries, Transaction, TransferRequest, TxId, UTXO_ENTRY_BYTES, crypto,
};

const DOMAIN_TX: &[u8] = b"nmtk.ledger.utxo.tx.v1";

/// One output: an amount, locked to an address.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TxOut {
    /// What it is worth.
    pub value: Amount,
    /// Who may spend it.
    pub owner: Address,
}

/// One input: the output being consumed, and the signature that unlocks it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct UtxoInput {
    /// Which unspent output is being consumed.
    pub outpoint: OutPoint,
    /// Proof the spender may consume it.
    pub signature: Signature,
}

/// A Bitcoin-shaped transaction: consume these, create those.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct UtxoTx {
    /// The outputs being consumed.
    pub inputs: Vec<UtxoInput>,
    /// The outputs being created. A transfer normally makes two: the payment and the change.
    pub outputs: Vec<TxOut>,
}

impl UtxoTx {
    /// The hash the signatures cover: everything the transaction says except the signatures
    /// themselves, which is also what gives the transaction its id.
    pub fn signing_message(&self) -> [u8; 32] {
        let outpoints: Vec<OutPoint> = self.inputs.iter().map(|input| input.outpoint).collect();
        signing_message(&outpoints, &self.outputs)
    }

    /// The transaction id. The outputs this transaction creates are named by it.
    pub fn id(&self) -> TxId {
        TxId::from_bytes(self.signing_message())
    }

    /// What the inputs are worth is not knowable from the transaction alone — only the ledger
    /// knows — but what the outputs ask for is.
    pub fn output_total(&self) -> Option<Amount> {
        self.outputs.iter().try_fold(0u64, |total, out| total.checked_add(out.value))
    }
}

fn signing_message(outpoints: &[OutPoint], outputs: &[TxOut]) -> [u8; 32] {
    let mut buffer =
        Vec::with_capacity(16 + outpoints.len() * 36 + outputs.len() * (8 + ADDRESS_BYTES));
    put_len(&mut buffer, outpoints.len());
    for outpoint in outpoints {
        buffer.extend_from_slice(outpoint.txid.as_bytes());
        buffer.extend_from_slice(&outpoint.index.to_be_bytes());
    }
    put_len(&mut buffer, outputs.len());
    for out in outputs {
        buffer.extend_from_slice(&out.value.to_be_bytes());
        buffer.extend_from_slice(out.owner.as_bytes());
    }
    crypto::hash(&[DOMAIN_TX, &buffer])
}

/// The unspent-output set, and nothing else.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct UtxoLedger {
    set: BTreeMap<OutPoint, TxOut>,
}

impl UtxoLedger {
    /// An empty set.
    pub fn new() -> Self {
        Self::default()
    }

    /// The opening holdings, as one transaction that creates every genesis output.
    pub fn from_genesis(genesis: &Genesis) -> Self {
        let txid = genesis.txid();
        let mut set = BTreeMap::new();
        for (position, coin) in genesis.coins().iter().enumerate() {
            // A genesis with more coins than a u32 can index is not a scenario anyone will build;
            // stopping is still better than wrapping an index onto an earlier output.
            let Ok(index) = u32::try_from(position) else { break };
            set.insert(OutPoint { txid, index }, TxOut { value: coin.value, owner: coin.owner });
        }
        Self { set }
    }

    /// Every unspent output, in outpoint order.
    pub fn outputs(&self) -> impl Iterator<Item = (&OutPoint, &TxOut)> {
        self.set.iter()
    }

    /// One unspent output, if the set still holds it.
    pub fn output(&self, outpoint: &OutPoint) -> Option<&TxOut> {
        self.set.get(outpoint)
    }

    fn touched(&self, tx: &UtxoTx) -> TouchedEntries {
        let reads: Vec<EntryKey> =
            tx.inputs.iter().map(|input| EntryKey::Output(input.outpoint)).collect();
        // Consuming an input removes it, so every input is written as well as read.
        let mut writes = reads.clone();
        let txid = tx.id();
        for position in 0..tx.outputs.len() {
            let Ok(index) = u32::try_from(position) else { break };
            writes.push(EntryKey::Output(OutPoint { txid, index }));
        }
        TouchedEntries::new(reads, writes)
    }

    fn validate(&self, tx: &UtxoTx) -> Result<(), (CheckStep, Rejection)> {
        if tx.inputs.is_empty() {
            return Err((CheckStep::Shape, Rejection::NoInputs));
        }
        if tx.outputs.is_empty() || tx.outputs.iter().any(|out| out.value == 0) {
            return Err((CheckStep::Shape, Rejection::ZeroAmount));
        }
        if tx.outputs.len() > MAX_OUTPUTS_PER_TX {
            return Err((
                CheckStep::Shape,
                Rejection::TooManyOutputs { outputs: tx.outputs.len(), limit: MAX_OUTPUTS_PER_TX },
            ));
        }
        let mut seen = BTreeSet::new();
        for input in &tx.inputs {
            if !seen.insert(input.outpoint) {
                return Err((
                    CheckStep::Shape,
                    Rejection::DuplicateInput(EntryKey::Output(input.outpoint)),
                ));
            }
        }

        // The set either still holds the output or it does not. A second spend of the same coin
        // finds nothing here, and that is the whole double-spend defence of this model.
        let mut spent = Vec::with_capacity(tx.inputs.len());
        for input in &tx.inputs {
            match self.set.get(&input.outpoint) {
                Some(out) => spent.push((input, *out)),
                None => {
                    return Err((CheckStep::StateLookup, Rejection::InputNotFound(input.outpoint)));
                }
            }
        }

        let message = tx.signing_message();
        for (input, out) in &spent {
            let signer = input.signature.signer();
            if signer != out.owner {
                return Err((CheckStep::Authorization, Rejection::NotOwner { signer }));
            }
            if !input.signature.covers(&message) {
                return Err((CheckStep::Authorization, Rejection::BadSignature));
            }
        }

        let mut inputs_total: Amount = 0;
        for (_, out) in &spent {
            inputs_total = inputs_total
                .checked_add(out.value)
                .ok_or((CheckStep::Value, Rejection::AmountOverflow))?;
        }
        let outputs_total =
            tx.output_total().ok_or((CheckStep::Value, Rejection::AmountOverflow))?;
        if inputs_total != outputs_total {
            return Err((
                CheckStep::Value,
                Rejection::ValueNotConserved { inputs: inputs_total, outputs: outputs_total },
            ));
        }
        Ok(())
    }
}

impl Ledger for UtxoLedger {
    fn model(&self) -> Model {
        Model::Utxo
    }

    fn entry_kind(&self) -> EntryKind {
        EntryKind::UnspentOutput
    }

    fn entry_bytes(&self) -> u64 {
        UTXO_ENTRY_BYTES
    }

    fn entry_count(&self) -> usize {
        self.set.len()
    }

    fn build_transfer(&self, request: &TransferRequest) -> Result<Transaction, Rejection> {
        if request.amount == 0 {
            return Err(Rejection::ZeroAmount);
        }
        let sender = request.sender();
        let held: Vec<(OutPoint, TxOut)> = self
            .set
            .iter()
            .filter(|(_, out)| out.owner == sender)
            .map(|(outpoint, out)| (*outpoint, *out))
            .collect();
        if held.is_empty() {
            return Err(Rejection::NoCoinAvailable);
        }

        let chosen: Vec<(OutPoint, TxOut)> = match request.coin {
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

        let mut total: Amount = 0;
        for (_, out) in &chosen {
            total = total.checked_add(out.value).ok_or(Rejection::AmountOverflow)?;
        }
        let mut outputs = vec![TxOut { value: request.amount, owner: request.to }];
        if total > request.amount {
            // Change is explicit here: what is not paid out has to be paid back to the sender in a
            // new output, or it would simply be gone.
            outputs.push(TxOut { value: total - request.amount, owner: sender });
        }

        let outpoints: Vec<OutPoint> = chosen.iter().map(|(outpoint, _)| *outpoint).collect();
        let signature = request.from.sign(&signing_message(&outpoints, &outputs));
        let inputs =
            outpoints.into_iter().map(|outpoint| UtxoInput { outpoint, signature }).collect();
        Ok(Transaction::Utxo(UtxoTx { inputs, outputs }))
    }

    fn apply(&mut self, tx: &Transaction) -> ApplyOutcome {
        let entries_before = self.set.len();
        let size_before = self.state_size_bytes();
        let Transaction::Utxo(tx) = tx else {
            return ApplyOutcome::rejected(
                Model::Utxo,
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
                Model::Utxo,
                step,
                reason,
                touched,
                entries_before,
                size_before,
            );
        }

        for input in &tx.inputs {
            self.set.remove(&input.outpoint);
        }
        let txid = tx.id();
        // The shape check caps outputs at MAX_OUTPUTS_PER_TX, so this counter cannot run past a
        // u32 and cannot name an output twice.
        let mut index: u32 = 0;
        for out in &tx.outputs {
            self.set.insert(OutPoint { txid, index }, *out);
            index = index.saturating_add(1);
        }

        ApplyOutcome {
            model: Model::Utxo,
            result: ApplyResult::Accepted(txid),
            touched,
            entries_before,
            entries_after: self.set.len(),
            size_before,
            size_after: self.state_size_bytes(),
        }
    }

    fn holdings_of(&self, who: &Address) -> Vec<Holding> {
        self.set
            .iter()
            .filter(|(_, out)| out.owner == *who)
            .map(|(outpoint, out)| Holding { entry: EntryKey::Output(*outpoint), value: out.value })
            .collect()
    }

    fn balance_of(&self, who: &Address) -> Amount {
        self.set
            .values()
            .filter(|out| out.owner == *who)
            .fold(0u64, |total, out| total.saturating_add(out.value))
    }

    fn touched_entries(&self, tx: &Transaction) -> TouchedEntries {
        match tx {
            Transaction::Utxo(tx) => self.touched(tx),
            _ => TouchedEntries::default(),
        }
    }

    fn facts(&self) -> ModelFacts {
        ModelFacts {
            model: Model::Utxo,
            entry_kind: EntryKind::UnspentOutput,
            entry_bytes: UTXO_ENTRY_BYTES,
            entry_count: self.set.len(),
            state_size_bytes: self.state_size_bytes(),
            // Spending a coin removes it from the set, so the second spend finds nothing to look
            // up. No nonce, no version, no later check is reached.
            double_spend_step: CheckStep::StateLookup,
            double_spend_kind: RejectionKind::InputNotFound,
            // Two coins are two entries: one sender can spend both at once.
            parallel_from_one_sender: true,
            // Paying the same person twice writes two new outputs, not one shared entry.
            parallel_to_one_recipient: true,
            has_shared_state: false,
        }
    }
}
