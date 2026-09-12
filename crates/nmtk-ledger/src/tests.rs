//! What would be silently wrong if one of the three models were subtly broken.

use crate::{
    ACCOUNT_ENTRY_BYTES, Address, Amount, CheckStep, CoinChoice, Conflict, EntryKey, Genesis, Key,
    Model, OBJECT_ENTRY_BYTES, Ownership, RejectionKind, Scenario, Transaction, TransferRequest,
    TransferTarget, UTXO_ENTRY_BYTES,
};

const SUPPLY: Amount = 150;

fn alice() -> Key {
    Key::from_seed(b"alice")
}

fn bob() -> Key {
    Key::from_seed(b"bob")
}

fn carol() -> Key {
    Key::from_seed(b"carol")
}

fn mallory() -> Key {
    Key::from_seed(b"mallory")
}

/// The address every model is told about, and only the object model treats as shared.
fn pool() -> Address {
    Address::from_seed(b"pool")
}

fn genesis() -> Genesis {
    Genesis::new()
        .holding(alice().address(), &[50, 30, 20])
        .holding(bob().address(), &[40])
        .shared_pool(pool(), 10)
}

fn scenario() -> Scenario {
    Scenario::new(&genesis())
}

fn total_held(scenario: &Scenario, model: Model) -> Amount {
    let ledger = scenario.ledger(model);
    [alice().address(), bob().address(), carol().address(), pool()]
        .iter()
        .map(|who| ledger.balance_of(who))
        .sum()
}

#[test]
fn genesis_opens_every_model_on_the_same_money() {
    let genesis = genesis();
    assert_eq!(genesis.total_supply(), SUPPLY);

    let scenario = Scenario::new(&genesis);
    assert_eq!(scenario.agreed_balance(&alice().address()), Some(100));
    assert_eq!(scenario.agreed_balance(&bob().address()), Some(40));
    assert_eq!(scenario.agreed_balance(&pool()), Some(10));
    assert_eq!(scenario.agreed_balance(&carol().address()), Some(0));

    for model in Model::ALL {
        assert_eq!(total_held(&scenario, model), SUPPLY, "{model:?} lost or minted money");
    }
}

#[test]
fn the_models_disagree_about_where_a_balance_is_kept() {
    let scenario = scenario();
    let holdings = scenario.holdings(&alice().address());

    // Three coins stay three things in two models and collapse into one balance in the third.
    assert_eq!(holdings.utxo.len(), 3);
    assert_eq!(holdings.account.len(), 1);
    assert_eq!(holdings.object.len(), 3);
    assert_eq!(holdings.account[0].value, 100);

    // The shared pool's value belongs to nobody, so there is nothing for its address to spend.
    assert!(scenario.holdings(&pool()).object.is_empty());
    assert_eq!(scenario.ledger(Model::Object).balance_of(&pool()), 10);
}

#[test]
fn the_same_double_spend_is_refused_by_all_three_for_three_different_reasons() {
    let mut scenario = scenario();
    let first = TransferRequest::new(alice(), bob().address(), 10);
    let second = TransferRequest::new(alice(), carol().address(), 10);

    let reports = scenario.double_spend(&first, &second);

    for (model, report) in reports.iter() {
        assert!(report.first.accepted(), "{model:?} refused the honest spend");
        assert!(report.stopped(), "{model:?} let the same money be spent twice");
    }

    // Where each model catches it. The UTXO model never reaches a freshness test: the coin is
    // simply not in the set any more.
    assert_eq!(reports.utxo.stopped_at(), Some(CheckStep::StateLookup));
    assert_eq!(reports.account.stopped_at(), Some(CheckStep::Freshness));
    assert_eq!(reports.object.stopped_at(), Some(CheckStep::Freshness));

    // Three different reasons, which is the point of the screen.
    assert_eq!(reports.utxo.stopped_by(), Some(RejectionKind::InputNotFound));
    assert_eq!(reports.account.stopped_by(), Some(RejectionKind::NonceMismatch));
    assert_eq!(reports.object.stopped_by(), Some(RejectionKind::StaleObjectVersion));

    // The money moved exactly once, everywhere.
    assert_eq!(scenario.agreed_balance(&bob().address()), Some(50));
    assert_eq!(scenario.agreed_balance(&carol().address()), Some(0));
    assert_eq!(scenario.agreed_balance(&alice().address()), Some(90));
}

#[test]
fn what_a_model_says_about_double_spends_matches_what_it_does() {
    let mut scenario = scenario();
    let before = scenario.facts();
    let reports = scenario.double_spend(
        &TransferRequest::new(alice(), bob().address(), 10),
        &TransferRequest::new(alice(), carol().address(), 10),
    );
    for (model, report) in reports.iter() {
        let facts = before.get(model);
        assert_eq!(Some(facts.double_spend_step), report.stopped_at(), "{model:?}");
        assert_eq!(Some(facts.double_spend_kind), report.stopped_by(), "{model:?}");
    }
}

#[test]
fn balances_agree_across_models_after_the_same_sequence_of_valid_transfers() {
    let mut scenario = scenario();
    let sequence = [
        TransferRequest::new(alice(), bob().address(), 60),
        TransferRequest::new(bob(), carol().address(), 25),
        TransferRequest::new(carol(), alice().address(), 5),
        TransferRequest::new(alice(), pool(), 10),
    ];

    for (step, request) in sequence.iter().enumerate() {
        let outcomes = scenario.transfer(request);
        for (model, outcome) in outcomes.iter() {
            assert!(outcome.accepted(), "{model:?} refused step {step}: {:?}", outcome.rejection());
        }
    }

    assert_eq!(scenario.agreed_balance(&alice().address()), Some(35));
    assert_eq!(scenario.agreed_balance(&bob().address()), Some(75));
    assert_eq!(scenario.agreed_balance(&carol().address()), Some(20));
    assert_eq!(scenario.agreed_balance(&pool()), Some(20));

    for model in Model::ALL {
        assert_eq!(total_held(&scenario, model), SUPPLY, "{model:?} lost or minted money");
    }
}

#[test]
fn paying_yourself_does_not_mint_money() {
    let mut scenario = scenario();
    let outcomes = scenario.transfer(&TransferRequest::new(alice(), alice().address(), 10));
    for (model, outcome) in outcomes.iter() {
        assert!(outcome.accepted(), "{model:?}: {:?}", outcome.rejection());
    }
    assert_eq!(scenario.agreed_balance(&alice().address()), Some(100));
    for model in Model::ALL {
        assert_eq!(total_held(&scenario, model), SUPPLY, "{model:?}");
    }
}

#[test]
fn two_transfers_from_one_sender_run_at_once_in_two_models_and_queue_in_the_third() {
    let scenario = scenario();
    // Twenty is small enough that any one of Alice's coins covers it, whichever order a model
    // lists them in, so both transfers really do spend two different holdings.
    let a = TransferRequest::new(alice(), bob().address(), 20).with_coin(CoinChoice::Index(0));
    let b = TransferRequest::new(alice(), carol().address(), 20).with_coin(CoinChoice::Index(1));

    let conflicts = scenario.conflicts(&a, &b);

    assert_eq!(conflicts.utxo, Ok(Conflict::Independent));
    assert_eq!(conflicts.object, Ok(Conflict::Independent));

    // One balance and one nonce: the account model has to run them one after the other.
    let account = conflicts.account.clone().expect("the account model builds both");
    assert!(!account.is_parallel_safe());
    assert_eq!(account.entries(), [EntryKey::Account(alice().address())]);
}

#[test]
fn two_payments_into_the_shared_object_queue_only_where_the_object_is_shared() {
    let scenario = scenario();
    let a = TransferRequest::new(alice(), pool(), 20).with_coin(CoinChoice::Index(0));
    let b = TransferRequest::new(bob(), pool(), 20).with_coin(CoinChoice::Index(0));

    let conflicts = scenario.conflicts(&a, &b);

    // Two payers, two coins, two new outputs: nothing is shared in the UTXO model.
    assert_eq!(conflicts.utxo, Ok(Conflict::Independent));

    // The account model shares the recipient's balance entry.
    let account = conflicts.account.clone().expect("the account model builds both");
    assert_eq!(account.entries(), [EntryKey::Account(pool())]);
    assert!(!account.is_parallel_safe());

    // The object model shares the pool object itself.
    let object = conflicts.object.clone().expect("the object model builds both");
    assert!(!object.is_parallel_safe());
    let pool_id = scenario.object().shared_pool(&pool()).expect("genesis made a pool");
    assert_eq!(object.entries(), [EntryKey::Object(pool_id)]);

    // The same pair of transfers: parallel in one model, queued in the other two.
    assert!(conflicts.utxo.clone().is_ok_and(|c| c.is_parallel_safe()));
    assert!(!account.is_parallel_safe() && !object.is_parallel_safe());
}

#[test]
fn a_transfer_touches_a_different_number_of_entries_in_each_model() {
    let scenario = scenario();
    // Spending part of a coin: the UTXO model writes the input plus payment plus change, the
    // account model rewrites two balances, the object model shrinks one object and makes one.
    // Five is under every coin any model would pick, so this really is a part-spend everywhere.
    let touched = scenario.touched(
        &TransferRequest::new(alice(), bob().address(), 5).with_coin(CoinChoice::Index(0)),
    );

    assert_eq!((touched.utxo.read_count(), touched.utxo.write_count()), (1, 3));
    assert_eq!((touched.account.read_count(), touched.account.write_count()), (2, 2));
    assert_eq!((touched.object.read_count(), touched.object.write_count()), (1, 2));
}

#[test]
fn state_grows_the_way_each_model_actually_grows() {
    let mut scenario = scenario();
    let facts = scenario.facts();
    assert_eq!(facts.utxo.entry_count, 5);
    assert_eq!(facts.account.entry_count, 3);
    assert_eq!(facts.object.entry_count, 5);
    assert_eq!(facts.utxo.state_size_bytes, 5 * UTXO_ENTRY_BYTES);
    assert_eq!(facts.account.state_size_bytes, 3 * ACCOUNT_ENTRY_BYTES);
    assert_eq!(facts.object.state_size_bytes, 5 * OBJECT_ENTRY_BYTES);

    // Paying someone who already has an entry. The UTXO set gains one output because change has
    // to go somewhere; the account map gains nothing at all; the object map gains the split.
    let outcomes = scenario.transfer(
        &TransferRequest::new(alice(), bob().address(), 5).with_coin(CoinChoice::Index(0)),
    );
    assert_eq!(outcomes.utxo.entry_delta(), 1);
    assert_eq!(outcomes.utxo.size_delta(), UTXO_ENTRY_BYTES as i64);
    assert_eq!(outcomes.account.entry_delta(), 0);
    assert_eq!(outcomes.account.size_delta(), 0);
    assert_eq!(outcomes.object.entry_delta(), 1);
    assert_eq!(outcomes.object.size_delta(), OBJECT_ENTRY_BYTES as i64);

    // Paying someone with no entry yet. Only the account model pays for the new name.
    let outcomes = scenario.transfer(
        &TransferRequest::new(alice(), carol().address(), 5).with_coin(CoinChoice::Index(0)),
    );
    assert_eq!(outcomes.account.entry_delta(), 1);
    assert_eq!(outcomes.account.size_delta(), ACCOUNT_ENTRY_BYTES as i64);
}

#[test]
fn handing_over_a_whole_object_costs_no_new_entry_and_paying_a_pool_frees_one() {
    let mut scenario = scenario();

    // Spend exactly what one object is worth: it changes hands instead of splitting, and one
    // entry is written — the object itself.
    let exact = whole_coin(&scenario, Model::Object, &alice().address());
    let outcome = scenario.ledger_mut(Model::Object).submit(&exact);
    assert!(outcome.accepted(), "{:?}", outcome.rejection());
    assert_eq!(outcome.entry_delta(), 0);
    assert_eq!(outcome.touched.write_count(), 1);

    // The UTXO model ends up with the same number of entries for a whole coin, but only because
    // it destroys one and makes another: two writes, not one.
    let exact = whole_coin(&scenario, Model::Utxo, &alice().address());
    let outcome = scenario.ledger_mut(Model::Utxo).submit(&exact);
    assert!(outcome.accepted(), "{:?}", outcome.rejection());
    assert_eq!(outcome.entry_delta(), 0);
    assert_eq!(outcome.touched.write_count(), 2);

    // Pay a whole object into the shared pool: the object is gone and the pool absorbs it.
    let exact = whole_coin_to(&scenario, Model::Object, &bob(), pool());
    let outcome = scenario.ledger_mut(Model::Object).submit(&exact);
    assert!(outcome.accepted(), "{:?}", outcome.rejection());
    assert_eq!(outcome.entry_delta(), -1);
    assert_eq!(outcome.size_delta(), -(OBJECT_ENTRY_BYTES as i64));
}

/// A transfer of exactly what this model says the sender's first holding is worth. The three
/// models do not list a sender's holdings in the same order, so each is asked for its own.
fn whole_coin(scenario: &Scenario, model: Model, sender: &Address) -> TransferRequest {
    let key = if *sender == alice().address() { alice() } else { bob() };
    whole_coin_to(scenario, model, &key, bob().address())
}

fn whole_coin_to(scenario: &Scenario, model: Model, sender: &Key, to: Address) -> TransferRequest {
    let holdings = scenario.ledger(model).holdings_of(&sender.address());
    let value = holdings.first().expect("the sender holds something").value;
    TransferRequest::new(*sender, to, value).with_coin(CoinChoice::Index(0))
}

#[test]
fn paying_into_the_shared_object_bumps_its_version_every_time() {
    let mut scenario = scenario();
    let pool_id = scenario.object().shared_pool(&pool()).expect("genesis made a pool");
    assert_eq!(scenario.object().object(&pool_id).map(|entry| entry.version), Some(0));

    for round in 1..=3u64 {
        let outcome = scenario.transfer(&TransferRequest::new(alice(), pool(), 5));
        assert!(outcome.object.accepted(), "{:?}", outcome.object.rejection());
        let entry = scenario.object().object(&pool_id).expect("the pool is still there");
        assert_eq!(entry.version, round);
        assert_eq!(entry.ownership, Ownership::Shared);
    }
    assert_eq!(scenario.agreed_balance(&pool()), Some(25));
}

#[test]
fn a_signature_from_the_wrong_key_is_refused_before_the_numbers_are_checked() {
    let mut scenario = scenario();
    let request = TransferRequest::new(alice(), bob().address(), 10);

    for model in Model::ALL {
        let built = scenario.ledger(model).build_transfer(&request).expect("builds");
        let forged = resign(&built, mallory());
        let outcome = scenario.ledger_mut(model).apply(&forged);
        assert_eq!(
            outcome.rejection().map(|(step, reason)| (step, reason.kind())),
            Some((CheckStep::Authorization, RejectionKind::NotOwner)),
            "{model:?} accepted a spend signed by the wrong key"
        );
    }
    assert_eq!(scenario.agreed_balance(&alice().address()), Some(100));
}

#[test]
fn changing_a_transfer_after_it_is_signed_is_refused() {
    let mut scenario = scenario();
    let request = TransferRequest::new(alice(), bob().address(), 10);
    let built = scenario.ledger(Model::Utxo).build_transfer(&request).expect("builds");
    let Transaction::Utxo(mut tx) = built else { panic!("the UTXO ledger builds UTXO shapes") };
    tx.outputs[0].value = 40;

    let outcome = scenario.ledger_mut(Model::Utxo).apply(&Transaction::Utxo(tx));
    assert_eq!(
        outcome.rejection().map(|(step, reason)| (step, reason.kind())),
        Some((CheckStep::Authorization, RejectionKind::BadSignature))
    );
}

#[test]
fn a_replayed_account_transfer_is_refused_even_when_the_balance_would_cover_it() {
    let mut scenario = scenario();
    let request = TransferRequest::new(alice(), bob().address(), 10);
    let built = scenario.ledger(Model::Account).build_transfer(&request).expect("builds");

    assert!(scenario.ledger_mut(Model::Account).apply(&built).accepted());
    let replay = scenario.ledger_mut(Model::Account).apply(&built);

    assert_eq!(
        replay.rejection(),
        Some((CheckStep::Freshness, crate::Rejection::NonceMismatch { expected: 1, found: 0 }))
    );
    // The balance was never the thing standing in the way.
    assert_eq!(scenario.ledger(Model::Account).balance_of(&alice().address()), 90);
}

#[test]
fn a_transfer_bigger_than_the_sender_holds_is_refused_by_every_model() {
    let mut scenario = scenario();
    let outcomes = scenario.transfer(&TransferRequest::new(carol(), bob().address(), 5));
    for (model, outcome) in outcomes.iter() {
        assert!(!outcome.accepted(), "{model:?} moved money Carol never had");
    }
    assert_eq!(scenario.agreed_balance(&bob().address()), Some(40));
}

#[test]
fn a_transaction_built_for_one_model_is_refused_by_another() {
    let mut scenario = scenario();
    let request = TransferRequest::new(alice(), bob().address(), 10);
    let utxo_tx = scenario.ledger(Model::Utxo).build_transfer(&request).expect("builds");

    let outcome = scenario.ledger_mut(Model::Account).apply(&utxo_tx);
    assert_eq!(
        outcome.rejection().map(|(_, reason)| reason.kind()),
        Some(RejectionKind::ModelMismatch)
    );

    let account_tx = scenario.ledger(Model::Account).build_transfer(&request).expect("builds");
    assert_eq!(
        scenario.ledger(Model::Object).conflicts_with(&utxo_tx, &account_tx),
        Conflict::ModelMismatch
    );
}

#[test]
fn a_shared_object_cannot_be_spent_as_if_somebody_owned_it() {
    let mut scenario = scenario();
    let pool_id = scenario.object().shared_pool(&pool()).expect("genesis made a pool");
    let inputs = vec![crate::ObjectRef { id: pool_id, version: 0 }];
    let target = TransferTarget::Owner(mallory().address());
    let mut tx =
        crate::ObjectTx { inputs, target, amount: 10, signature: mallory().sign(&[0u8; 32]) };
    tx.signature = mallory().sign(&tx.signing_message());

    let outcome = scenario.ledger_mut(Model::Object).apply(&Transaction::Object(tx));
    assert_eq!(
        outcome.rejection().map(|(step, reason)| (step, reason.kind())),
        Some((CheckStep::Authorization, RejectionKind::NotOwner))
    );
    assert_eq!(scenario.ledger(Model::Object).balance_of(&pool()), 10);
}

#[test]
fn asking_for_a_coin_the_sender_does_not_have_is_refused_at_build_time() {
    let mut scenario = scenario();
    let outcomes = scenario.transfer(
        &TransferRequest::new(bob(), carol().address(), 10).with_coin(CoinChoice::Index(4)),
    );
    assert_eq!(
        outcomes.utxo.rejection().map(|(step, reason)| (step, reason.kind())),
        Some((CheckStep::Build, RejectionKind::NoSuchCoin))
    );
    assert_eq!(
        outcomes.object.rejection().map(|(step, reason)| (step, reason.kind())),
        Some((CheckStep::Build, RejectionKind::NoSuchCoin))
    );
    // The account model has nothing to pick between, so the choice passes it by entirely.
    assert!(outcomes.account.accepted());
}

/// Rebuilds a transaction with every signature replaced by this key's.
fn resign(tx: &Transaction, key: Key) -> Transaction {
    match tx {
        Transaction::Utxo(tx) => {
            let signature = key.sign(&tx.signing_message());
            Transaction::Utxo(crate::UtxoTx {
                inputs: tx
                    .inputs
                    .iter()
                    .map(|input| crate::UtxoInput { outpoint: input.outpoint, signature })
                    .collect(),
                outputs: tx.outputs.clone(),
            })
        }
        Transaction::Account(tx) => {
            let mut tx = *tx;
            tx.signature = key.sign(&tx.signing_message());
            Transaction::Account(tx)
        }
        Transaction::Object(tx) => {
            let mut tx = tx.clone();
            tx.signature = key.sign(&tx.signing_message());
            Transaction::Object(tx)
        }
    }
}
