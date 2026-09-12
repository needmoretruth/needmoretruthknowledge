//! Stage 4 — a real circuit, compiled, proved and verified by `halo2_proofs`, with no setup to trust.
//!
//! The circuit is the balance check of a shielded payment: the prover knows two amounts, each
//! inside a range, that add up to a public total. The range part is not decoration. Field
//! arithmetic wraps, so without it an attacker can "send" more than they hold and cover the
//! difference with a negative-looking amount, and the sum still comes out right. That is the
//! forgery this stage runs, and the bit decomposition in the circuit is what stops it.
//!
//! The generators come from `Params::new`, which derives them by hashing — there is no ceremony
//! here, no `tau`, and nothing that had to be destroyed. That is the difference stage 3 sets up.

use halo2_proofs::arithmetic::Field;
use halo2_proofs::circuit::{Layouter, SimpleFloorPlanner, Value};
use halo2_proofs::pasta::group::ff::PrimeField;
use halo2_proofs::pasta::{EqAffine, Fp};
use halo2_proofs::plonk::{
    Advice, Circuit, Column, ConstraintSystem, Error as PlonkError, Expression, Fixed, Instance,
    ProvingKey, Selector, SingleVerifier, create_proof, keygen_pk, keygen_vk, verify_proof,
};
use halo2_proofs::poly::Rotation;
use halo2_proofs::poly::commitment::Params;
use halo2_proofs::transcript::{Blake2bRead, Blake2bWrite, Challenge255};
use nmtk_core::{MachineProfile, SizeClass};

use crate::rng::DeterministicRng;
use crate::{ProofStep, ZkError};

/// Advice columns the circuit uses.
pub const ADVICE_COLUMNS: usize = 5;
/// Fixed columns the circuit uses.
pub const FIXED_COLUMNS: usize = 1;
/// Instance columns the circuit uses.
pub const INSTANCE_COLUMNS: usize = 1;
/// Custom gates the circuit declares.
pub const GATES: usize = 3;
/// The narrowest range the module will build a circuit for.
pub const MIN_VALUE_BITS: u32 = 8;
/// The widest range the module will build a circuit for: a whole `u64` amount.
pub const MAX_VALUE_BITS: u32 = 64;

/// How wide the amounts are and how many rows the circuit gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Bits each amount is constrained to.
    pub value_bits: u32,
    /// The circuit has `2^k` rows.
    pub k: u32,
}

impl Config {
    /// Picks a width and the rows it needs. A wider range is a bigger circuit and a slower proof,
    /// which is the thing a reader is meant to feel.
    pub fn new(value_bits: u32) -> Result<Self, ZkError> {
        if !(MIN_VALUE_BITS..=MAX_VALUE_BITS).contains(&value_bits) {
            return Err(ZkError::ValueBitsUnsupported);
        }
        let needed = value_bits as usize + 1 + reserved_rows();
        let mut k = 4u32;
        while (1usize << k) < needed {
            k += 1;
        }
        Ok(Self { value_bits, k })
    }

    /// The width this machine starts on. A four-core laptop should not open on the widest circuit
    /// the first time a reader presses the key.
    pub fn for_machine(profile: &MachineProfile) -> Self {
        let bits = match profile.size_class() {
            SizeClass::Small => 16,
            SizeClass::Medium => 32,
            SizeClass::Large => 64,
        };
        // `bits` is inside the supported range by construction, so the fallback is never reached.
        Self::new(bits).unwrap_or(Self { value_bits: MIN_VALUE_BITS, k: 5 })
    }

    /// The largest amount this circuit will accept.
    pub fn max_value(&self) -> u64 {
        if self.value_bits >= 64 { u64::MAX } else { (1u64 << self.value_bits) - 1 }
    }

    /// Rows the witness occupies.
    pub fn rows_used(&self) -> usize {
        self.value_bits as usize + 1
    }
}

fn reserved_rows() -> usize {
    let mut meta = ConstraintSystem::<Fp>::default();
    BalanceCircuit::configure(&mut meta);
    meta.minimum_rows()
}

/// The columns, selectors and gates of the balance circuit.
#[derive(Clone, Debug)]
pub struct BalanceConfig {
    bit_a: Column<Advice>,
    acc_a: Column<Advice>,
    bit_b: Column<Advice>,
    acc_b: Column<Advice>,
    out: Column<Advice>,
    coeff: Column<Fixed>,
    instance: Column<Instance>,
    s_init: Selector,
    s_bit: Selector,
    s_sum: Selector,
}

/// The circuit itself.
///
/// Two amounts are written out bit by bit down the rows. Each bit is forced to be a bit, each
/// accumulator picks up `bit * 2^i` from the fixed column beside it, and the last row adds the two
/// accumulators and is copied into the public instance.
#[derive(Clone, Debug)]
pub struct BalanceCircuit {
    a: Option<Fp>,
    b: Option<Fp>,
    bits: usize,
}

impl BalanceCircuit {
    /// The circuit with a witness in it.
    pub fn new(a: Fp, b: Fp, bits: usize) -> Self {
        Self { a: Some(a), b: Some(b), bits }
    }

    /// The circuit with no witness, which is all key generation needs to see.
    pub fn without_witness(bits: usize) -> Self {
        Self { a: None, b: None, bits }
    }
}

fn value_of(value: Option<Fp>) -> Value<Fp> {
    match value {
        Some(value) => Value::known(value),
        None => Value::unknown(),
    }
}

fn low_bits(value: &Fp, bits: usize) -> Vec<bool> {
    let repr = value.to_repr();
    (0..bits).map(|i| (repr[i / 8] >> (i % 8)) & 1 == 1).collect()
}

impl Circuit<Fp> for BalanceCircuit {
    type Config = BalanceConfig;
    type FloorPlanner = SimpleFloorPlanner;

    fn without_witnesses(&self) -> Self {
        Self::without_witness(self.bits)
    }

    fn configure(meta: &mut ConstraintSystem<Fp>) -> Self::Config {
        let bit_a = meta.advice_column();
        let acc_a = meta.advice_column();
        let bit_b = meta.advice_column();
        let acc_b = meta.advice_column();
        let out = meta.advice_column();
        let coeff = meta.fixed_column();
        let instance = meta.instance_column();

        meta.enable_equality(out);
        meta.enable_equality(instance);

        let s_init = meta.selector();
        let s_bit = meta.selector();
        let s_sum = meta.selector();

        // Both running totals start at zero.
        meta.create_gate("", |meta| {
            let s = meta.query_selector(s_init);
            let a = meta.query_advice(acc_a, Rotation::cur());
            let b = meta.query_advice(acc_b, Rotation::cur());
            vec![s.clone() * a, s * b]
        });

        // Each row: the two cells are bits, and each running total picks up `bit * 2^i`.
        meta.create_gate("", |meta| {
            let s = meta.query_selector(s_bit);
            let one = Expression::Constant(Fp::ONE);
            let ba = meta.query_advice(bit_a, Rotation::cur());
            let bb = meta.query_advice(bit_b, Rotation::cur());
            let a_cur = meta.query_advice(acc_a, Rotation::cur());
            let a_next = meta.query_advice(acc_a, Rotation::next());
            let b_cur = meta.query_advice(acc_b, Rotation::cur());
            let b_next = meta.query_advice(acc_b, Rotation::next());
            let weight = meta.query_fixed(coeff);
            vec![
                s.clone() * ba.clone() * (one.clone() - ba.clone()),
                s.clone() * bb.clone() * (one - bb.clone()),
                s.clone() * (a_next - a_cur - ba * weight.clone()),
                s * (b_next - b_cur - bb * weight),
            ]
        });

        // The last row holds the sum that is copied out to the public input.
        meta.create_gate("", |meta| {
            let s = meta.query_selector(s_sum);
            let a = meta.query_advice(acc_a, Rotation::cur());
            let b = meta.query_advice(acc_b, Rotation::cur());
            let out = meta.query_advice(out, Rotation::cur());
            vec![s * (out - a - b)]
        });

        BalanceConfig { bit_a, acc_a, bit_b, acc_b, out, coeff, instance, s_init, s_bit, s_sum }
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<Fp>,
    ) -> Result<(), PlonkError> {
        let bits = self.bits;
        let a_bits = self.a.map(|a| low_bits(&a, bits));
        let b_bits = self.b.map(|b| low_bits(&b, bits));

        let sum_cell = layouter.assign_region(
            || "",
            |mut region| {
                config.s_init.enable(&mut region, 0)?;
                let mut acc_a: Option<Fp> = a_bits.as_ref().map(|_| Fp::ZERO);
                let mut acc_b: Option<Fp> = b_bits.as_ref().map(|_| Fp::ZERO);
                region.assign_advice(|| "", config.acc_a, 0, move || value_of(acc_a))?;
                region.assign_advice(|| "", config.acc_b, 0, move || value_of(acc_b))?;

                let mut weight = Fp::ONE;
                for row in 0..bits {
                    config.s_bit.enable(&mut region, row)?;
                    region.assign_fixed(|| "", config.coeff, row, move || Value::known(weight))?;

                    let bit_a = a_bits.as_ref().map(|v| if v[row] { Fp::ONE } else { Fp::ZERO });
                    let bit_b = b_bits.as_ref().map(|v| if v[row] { Fp::ONE } else { Fp::ZERO });
                    region.assign_advice(|| "", config.bit_a, row, move || value_of(bit_a))?;
                    region.assign_advice(|| "", config.bit_b, row, move || value_of(bit_b))?;

                    acc_a = match (acc_a, bit_a) {
                        (Some(acc), Some(bit)) => Some(acc + bit * weight),
                        _ => None,
                    };
                    acc_b = match (acc_b, bit_b) {
                        (Some(acc), Some(bit)) => Some(acc + bit * weight),
                        _ => None,
                    };
                    region.assign_advice(|| "", config.acc_a, row + 1, move || value_of(acc_a))?;
                    region.assign_advice(|| "", config.acc_b, row + 1, move || value_of(acc_b))?;
                    weight = weight.double();
                }

                config.s_sum.enable(&mut region, bits)?;
                let total = match (acc_a, acc_b) {
                    (Some(a), Some(b)) => Some(a + b),
                    _ => None,
                };
                let cell =
                    region.assign_advice(|| "", config.out, bits, move || value_of(total))?;
                Ok(cell.cell())
            },
        )?;

        layouter.constrain_instance(sum_cell, config.instance, 0)
    }
}

/// The generators and the proving key: everything a prover needs that is not the witness.
///
/// Building this is the only slow part of the stage, and it is not a ceremony — the generators are
/// derived by hashing, and no secret exists at any point that would have to be destroyed.
pub struct Keys {
    config: Config,
    params: Params<EqAffine>,
    pk: ProvingKey<EqAffine>,
}

impl Keys {
    /// Derives the generators and the keys. Returns the time it took, which a screen puts in the
    /// setup column beside stage 3's ceremony.
    pub fn generate(config: Config) -> Result<(Self, u64), ZkError> {
        let started = std::time::Instant::now();
        let params: Params<EqAffine> = Params::new(config.k);
        let empty = BalanceCircuit::without_witness(config.value_bits as usize);
        let vk = keygen_vk(&params, &empty).map_err(|_| ZkError::Proving(ProofStep::Keygen))?;
        let pk = keygen_pk(&params, vk, &empty).map_err(|_| ZkError::Proving(ProofStep::Keygen))?;
        Ok((Self { config, params, pk }, started.elapsed().as_nanos() as u64))
    }

    /// The width and row count these keys were built for.
    pub fn config(&self) -> Config {
        self.config
    }
}

impl core::fmt::Debug for Keys {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Keys").field("config", &self.config).finish_non_exhaustive()
    }
}

/// Proves that two secret amounts add to a public total, each inside the range.
pub fn prove(
    keys: &Keys,
    a: Fp,
    b: Fp,
    total: Fp,
    rng: &mut DeterministicRng,
) -> Result<Vec<u8>, ZkError> {
    let circuit = BalanceCircuit::new(a, b, keys.config.value_bits as usize);
    let instance = [total];
    let column: &[&[Fp]] = &[&instance];
    let instances: &[&[&[Fp]]] = &[column];
    let mut transcript = Blake2bWrite::<_, _, Challenge255<_>>::init(Vec::new());
    create_proof(&keys.params, &keys.pk, &[circuit], instances, rng, &mut transcript)
        .map_err(|_| ZkError::Proving(ProofStep::Prove))?;
    Ok(transcript.finalize())
}

/// Checks a proof against a public total. Nothing secret is needed and nothing secret is learned.
pub fn verify(keys: &Keys, proof: &[u8], total: Fp) -> bool {
    let instance = [total];
    let column: &[&[Fp]] = &[&instance];
    let instances: &[&[&[Fp]]] = &[column];
    let strategy = SingleVerifier::new(&keys.params);
    let mut transcript = Blake2bRead::<_, _, Challenge255<_>>::init(proof);
    verify_proof(&keys.params, keys.pk.get_vk(), strategy, instances, &mut transcript).is_ok()
}

/// The size and shape of the circuit, as a screen reports it beside the other three stages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CircuitShape {
    /// Bits each amount is held to.
    pub value_bits: u32,
    /// The circuit has `2^k` rows.
    pub k: u32,
    /// Rows the witness fills.
    pub rows_used: usize,
    /// Rows the proof system reserves for blinding.
    pub rows_reserved: usize,
    /// Advice columns.
    pub advice_columns: usize,
    /// Fixed columns.
    pub fixed_columns: usize,
    /// Instance columns.
    pub instance_columns: usize,
    /// Custom gates.
    pub gates: usize,
    /// Degree of the constraint system.
    pub degree: usize,
}

impl CircuitShape {
    /// Reads the shape out of the constraint system the circuit builds.
    pub fn of(config: Config) -> Self {
        let mut meta = ConstraintSystem::<Fp>::default();
        BalanceCircuit::configure(&mut meta);
        Self {
            value_bits: config.value_bits,
            k: config.k,
            rows_used: config.rows_used(),
            rows_reserved: meta.minimum_rows(),
            advice_columns: ADVICE_COLUMNS,
            fixed_columns: FIXED_COLUMNS,
            instance_columns: INSTANCE_COLUMNS,
            gates: GATES,
            degree: meta.degree(),
        }
    }
}

/// The attacker's attempt: spend more than the note holds and cover it with a wrapped-around
/// amount that is negative in every sense except the field's.
#[derive(Clone, Debug)]
pub struct ForgeryRecord {
    /// Whether the prover would even produce bytes for this witness.
    pub proof_produced: bool,
    /// The bytes, if any.
    pub proof_bytes: usize,
    /// What the verifier said.
    pub accepted: bool,
    /// The amount the attacker tried to send, above the total it was allowed.
    pub overspend: u64,
}

/// Everything stage 4 produces.
#[derive(Clone, Debug)]
pub struct Run {
    /// The width and rows used.
    pub config: Config,
    /// The circuit's shape.
    pub shape: CircuitShape,
    /// The public total.
    pub total: u128,
    /// The amount actually sent.
    pub sent: u64,
    /// The change that went back.
    pub change: u64,
    /// The proof bytes.
    pub proof: Vec<u8>,
    /// What the verifier said.
    pub accepted: bool,
    /// The attacker.
    pub forgery: ForgeryRecord,
    /// Size of the honest proof.
    pub proof_bytes: usize,
    /// Time to derive generators and keys.
    pub setup_nanos: u64,
    /// Time to prove.
    pub prove_nanos: u64,
    /// Time to verify.
    pub verify_nanos: u64,
}

/// Runs stage 4 end to end: build the keys, prove an honest split of a note, verify it, then let an
/// attacker try to overspend and watch the same verifier refuse.
pub fn run(
    config: Config,
    sent: u64,
    change: u64,
    rng: &mut DeterministicRng,
) -> Result<Run, ZkError> {
    let limit = config.max_value();
    if sent > limit || change > limit {
        return Err(ZkError::ValueOutOfRange);
    }
    let total = sent as u128 + change as u128;
    let total_field = Fp::from_u128(total);

    let (keys, setup_nanos) = Keys::generate(config)?;

    let start_prove = std::time::Instant::now();
    let proof = prove(&keys, Fp::from(sent), Fp::from(change), total_field, rng)?;
    let prove_nanos = start_prove.elapsed().as_nanos() as u64;

    let start_verify = std::time::Instant::now();
    let accepted = verify(&keys, &proof, total_field);
    let verify_nanos = start_verify.elapsed().as_nanos() as u64;

    // The attacker claims to send one more than the note holds, and offsets it with `-1`, which is
    // a perfectly good field element and adds up to exactly the right total.
    let overspend = 1u64;
    let forged_sent = total_field + Fp::from(overspend);
    let forged_change = -Fp::from(overspend);
    let forged = prove(&keys, forged_sent, forged_change, total_field, rng);
    let forgery = match forged {
        Ok(bytes) => ForgeryRecord {
            proof_produced: true,
            proof_bytes: bytes.len(),
            accepted: verify(&keys, &bytes, total_field),
            overspend,
        },
        Err(_) => {
            ForgeryRecord { proof_produced: false, proof_bytes: 0, accepted: false, overspend }
        }
    };

    Ok(Run {
        config,
        shape: CircuitShape::of(config),
        total,
        sent,
        change,
        proof_bytes: proof.len(),
        proof,
        accepted,
        forgery,
        setup_nanos,
        prove_nanos,
        verify_nanos,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Seed;
    use halo2_proofs::dev::MockProver;

    fn rng() -> DeterministicRng {
        DeterministicRng::new(Seed::fixed())
    }

    fn mock(
        bits: u32,
        a: Fp,
        b: Fp,
        total: Fp,
    ) -> Result<(), Vec<halo2_proofs::dev::VerifyFailure>> {
        let config = Config::new(bits).expect("supported width");
        let circuit = BalanceCircuit::new(a, b, bits as usize);
        MockProver::run(config.k, &circuit, vec![vec![total]])
            .expect("the circuit fits its rows")
            .verify()
    }

    #[test]
    fn an_honest_witness_satisfies_the_circuit() {
        assert!(mock(16, Fp::from(9_000), Fp::from(1_000), Fp::from(10_000)).is_ok());
    }

    #[test]
    fn an_out_of_range_witness_does_not() {
        let total = Fp::from(10_000u64);
        assert!(mock(16, total + Fp::ONE, -Fp::ONE, total).is_err());
    }

    #[test]
    fn a_witness_that_does_not_add_up_does_not() {
        assert!(mock(16, Fp::from(9_000), Fp::from(999), Fp::from(10_000)).is_err());
    }

    #[test]
    fn an_amount_above_the_range_does_not() {
        // 2^16 needs seventeen bits; the circuit only writes down sixteen.
        assert!(mock(16, Fp::from(65_536), Fp::from(0), Fp::from(65_536)).is_err());
    }

    #[test]
    fn an_honest_proof_verifies() {
        let config = Config::new(16).expect("supported width");
        let run = run(config, 9_000, 1_000, &mut rng()).expect("stage 4 runs");
        assert!(run.accepted);
        assert!(run.proof_bytes > 0);
    }

    #[test]
    fn an_overspending_proof_does_not() {
        let config = Config::new(16).expect("supported width");
        let run = run(config, 9_000, 1_000, &mut rng()).expect("stage 4 runs");
        assert!(!run.forgery.accepted);
    }

    #[test]
    fn a_proof_does_not_verify_against_a_different_total() {
        let config = Config::new(16).expect("supported width");
        let (keys, _) = Keys::generate(config).expect("keys");
        let total = Fp::from(10_000u64);
        let proof =
            prove(&keys, Fp::from(9_000u64), Fp::from(1_000u64), total, &mut rng()).expect("proof");
        assert!(verify(&keys, &proof, total));
        assert!(!verify(&keys, &proof, total + Fp::ONE));
    }

    #[test]
    fn a_tampered_proof_does_not_verify() {
        let config = Config::new(16).expect("supported width");
        let (keys, _) = Keys::generate(config).expect("keys");
        let total = Fp::from(10_000u64);
        let mut proof =
            prove(&keys, Fp::from(9_000u64), Fp::from(1_000u64), total, &mut rng()).expect("proof");
        proof[0] ^= 0x01;
        assert!(!verify(&keys, &proof, total));
    }

    #[test]
    fn widths_outside_the_supported_range_are_refused() {
        assert_eq!(Config::new(4).err(), Some(ZkError::ValueBitsUnsupported));
        assert_eq!(Config::new(65).err(), Some(ZkError::ValueBitsUnsupported));
    }

    #[test]
    fn every_supported_width_leaves_room_for_its_rows() {
        for bits in MIN_VALUE_BITS..=MAX_VALUE_BITS {
            let config = Config::new(bits).expect("supported width");
            let shape = CircuitShape::of(config);
            assert!((1usize << config.k) >= shape.rows_used + shape.rows_reserved);
        }
    }

    #[test]
    fn an_amount_above_the_configured_range_is_refused_before_proving() {
        let config = Config::new(16).expect("supported width");
        assert_eq!(run(config, 65_536, 0, &mut rng()).err(), Some(ZkError::ValueOutOfRange));
    }
}
