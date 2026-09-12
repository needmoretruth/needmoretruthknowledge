//! One transfer, three models, side by side.
//!
//! A screen writes a transfer once and gets back three answers: what each model did with it, what
//! each model's state now costs, and — for a pair of transfers — which models could have run them
//! at the same time.

use crate::{
    AccountLedger, Address, Amount, ApplyOutcome, CheckStep, Conflict, Genesis, Holding, Ledger,
    Model, ModelFacts, ObjectLedger, Rejection, RejectionKind, TouchedEntries, Transaction,
    TransferRequest, UtxoLedger,
};

/// The same thing, once per model.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct SideBySide<T> {
    /// The UTXO model's answer.
    pub utxo: T,
    /// The account model's answer.
    pub account: T,
    /// The object model's answer.
    pub object: T,
}

impl<T> SideBySide<T> {
    /// One model's answer.
    pub fn get(&self, model: Model) -> &T {
        match model {
            Model::Utxo => &self.utxo,
            Model::Account => &self.account,
            Model::Object => &self.object,
        }
    }

    /// All three, in the order a screen shows them.
    pub fn iter(&self) -> impl Iterator<Item = (Model, &T)> {
        [(Model::Utxo, &self.utxo), (Model::Account, &self.account), (Model::Object, &self.object)]
            .into_iter()
    }

    /// The same three answers, put through a function.
    pub fn map<U>(self, mut f: impl FnMut(Model, T) -> U) -> SideBySide<U> {
        SideBySide {
            utxo: f(Model::Utxo, self.utxo),
            account: f(Model::Account, self.account),
            object: f(Model::Object, self.object),
        }
    }
}

impl<T: PartialEq> SideBySide<T> {
    /// Whether all three models said the same thing.
    pub fn all_agree(&self) -> bool {
        self.utxo == self.account && self.account == self.object
    }
}

/// What happened when the same money was spent twice.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DoubleSpendReport {
    /// Which model.
    pub model: Model,
    /// The first spend, built and applied against the opening state.
    pub first: ApplyOutcome,
    /// The second spend, built against that same opening state and applied afterwards.
    pub second: ApplyOutcome,
}

impl DoubleSpendReport {
    /// Whether the model did its job: the first went in, the second did not.
    pub fn stopped(&self) -> bool {
        self.first.accepted() && !self.second.accepted()
    }

    /// The step the second spend died at.
    pub fn stopped_at(&self) -> Option<CheckStep> {
        self.second.rejection().map(|(step, _)| step)
    }

    /// The reason it died, with its numbers stripped off so models can be compared.
    pub fn stopped_by(&self) -> Option<RejectionKind> {
        self.second.rejection().map(|(_, reason)| reason.kind())
    }
}

/// All three models, opened at the same holdings and driven together.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Scenario {
    utxo: UtxoLedger,
    account: AccountLedger,
    object: ObjectLedger,
}

impl Scenario {
    /// Opens all three at the same holdings.
    pub fn new(genesis: &Genesis) -> Self {
        Self {
            utxo: UtxoLedger::from_genesis(genesis),
            account: AccountLedger::from_genesis(genesis),
            object: ObjectLedger::from_genesis(genesis),
        }
    }

    /// The UTXO set, for a screen that wants to draw the coins.
    pub fn utxo(&self) -> &UtxoLedger {
        &self.utxo
    }

    /// The accounts, for a screen that wants to draw the balances.
    pub fn account(&self) -> &AccountLedger {
        &self.account
    }

    /// The objects, for a screen that wants to draw owners and versions.
    pub fn object(&self) -> &ObjectLedger {
        &self.object
    }

    /// One model, read only.
    pub fn ledger(&self, model: Model) -> &dyn Ledger {
        match model {
            Model::Utxo => &self.utxo,
            Model::Account => &self.account,
            Model::Object => &self.object,
        }
    }

    /// One model, to apply a transaction to.
    pub fn ledger_mut(&mut self, model: Model) -> &mut dyn Ledger {
        match model {
            Model::Utxo => &mut self.utxo,
            Model::Account => &mut self.account,
            Model::Object => &mut self.object,
        }
    }

    /// Builds this transfer in each model's own shape and applies it. A transfer that cannot even
    /// be built comes back refused at [`CheckStep::Build`], so every model answers the same shape.
    pub fn transfer(&mut self, request: &TransferRequest) -> SideBySide<ApplyOutcome> {
        SideBySide {
            utxo: self.utxo.submit(request),
            account: self.account.submit(request),
            object: self.object.submit(request),
        }
    }

    /// Builds this transfer in each model without applying it.
    pub fn build(&self, request: &TransferRequest) -> SideBySide<Result<Transaction, Rejection>> {
        SideBySide {
            utxo: self.utxo.build_transfer(request),
            account: self.account.build_transfer(request),
            object: self.object.build_transfer(request),
        }
    }

    /// What an address is worth, according to each model. Valid transfers keep these three equal.
    pub fn balances(&self, who: &Address) -> SideBySide<Amount> {
        SideBySide {
            utxo: self.utxo.balance_of(who),
            account: self.account.balance_of(who),
            object: self.object.balance_of(who),
        }
    }

    /// The balance all three agree on, or `None` when they do not — which would mean a bug in one
    /// of them, since no fee is charged anywhere.
    pub fn agreed_balance(&self, who: &Address) -> Option<Amount> {
        let balances = self.balances(who);
        balances.all_agree().then_some(balances.utxo)
    }

    /// Where each model says the sender's value is held. One entry per coin, one entry in total,
    /// or one entry per object.
    pub fn holdings(&self, who: &Address) -> SideBySide<Vec<Holding>> {
        SideBySide {
            utxo: self.utxo.holdings_of(who),
            account: self.account.holdings_of(who),
            object: self.object.holdings_of(who),
        }
    }

    /// What each model stores and what it costs right now.
    pub fn facts(&self) -> SideBySide<ModelFacts> {
        SideBySide {
            utxo: self.utxo.facts(),
            account: self.account.facts(),
            object: self.object.facts(),
        }
    }

    /// State size in bytes, per model.
    pub fn state_sizes(&self) -> SideBySide<u64> {
        SideBySide {
            utxo: self.utxo.state_size_bytes(),
            account: self.account.state_size_bytes(),
            object: self.object.state_size_bytes(),
        }
    }

    /// The entries one transfer would read and write, per model, without applying anything.
    pub fn touched(&self, request: &TransferRequest) -> SideBySide<TouchedEntries> {
        self.build(request).map(|model, built| match built {
            Ok(tx) => self.ledger(model).touched_entries(&tx),
            Err(_) => TouchedEntries::default(),
        })
    }

    /// Spends the same money twice: both transfers are built against the state as it is now, then
    /// applied one after the other. Each model refuses the second one for its own reason.
    pub fn double_spend(
        &mut self,
        first: &TransferRequest,
        second: &TransferRequest,
    ) -> SideBySide<DoubleSpendReport> {
        SideBySide {
            utxo: double_spend_on(&mut self.utxo, first, second),
            account: double_spend_on(&mut self.account, first, second),
            object: double_spend_on(&mut self.object, first, second),
        }
    }

    /// Whether two transfers could run at the same time, per model.
    ///
    /// Both are built against the state as it is now and neither is applied, so this answers
    /// "could a validator have taken these in either order, or at once".
    pub fn conflicts(
        &self,
        a: &TransferRequest,
        b: &TransferRequest,
    ) -> SideBySide<Result<Conflict, Rejection>> {
        SideBySide {
            utxo: conflicts_on(&self.utxo, a, b),
            account: conflicts_on(&self.account, a, b),
            object: conflicts_on(&self.object, a, b),
        }
    }
}

fn double_spend_on(
    ledger: &mut dyn Ledger,
    first: &TransferRequest,
    second: &TransferRequest,
) -> DoubleSpendReport {
    let model = ledger.model();
    // Both are built before either is applied. That is what a double spend is: two transactions
    // written against the same view of the world.
    let built_first = ledger.build_transfer(first);
    let built_second = ledger.build_transfer(second);
    let first = apply_built(ledger, built_first);
    let second = apply_built(ledger, built_second);
    DoubleSpendReport { model, first, second }
}

fn apply_built(ledger: &mut dyn Ledger, built: Result<Transaction, Rejection>) -> ApplyOutcome {
    match built {
        Ok(tx) => ledger.apply(&tx),
        Err(reason) => ApplyOutcome::rejected(
            ledger.model(),
            CheckStep::Build,
            reason,
            TouchedEntries::default(),
            ledger.entry_count(),
            ledger.state_size_bytes(),
        ),
    }
}

fn conflicts_on(
    ledger: &dyn Ledger,
    a: &TransferRequest,
    b: &TransferRequest,
) -> Result<Conflict, Rejection> {
    let a = ledger.build_transfer(a)?;
    let b = ledger.build_transfer(b)?;
    Ok(ledger.conflicts_with(&a, &b))
}
