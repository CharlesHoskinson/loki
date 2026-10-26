//! Independent stock verifier for the Sherlock chain benchmark.
//!
//! Built only in `bench/verifier`: registry `midnight-proofs =0.8.3` (default
//! features plus the disclosed `circuit-params`), no Loki, no Moriarty producer
//! module. Logic follows the pinned `mori-recursion/examples/verify_chain.rs`.
//!
//! `stock-verifier serve --srs K20_SRS` loads and authenticates the SRS once,
//! prints `{"ready":true,...}`, then answers one JSON request per stdin line:
//!   {"stage":"children","dir":D}
//!   {"stage":"parent","dir":D,"children":C}
//!   {"stage":"decider","dir":D,"parent":P}
//!   {"stage":"qualify","dir":D,"kind":"children|parent|decider"}  (mutation controls)
//!   {"stage":"exit"}
//! Every proof receipt requires full transcript consumption and the final KZG
//! pairing (`batch_verify` of one proof). Receipts carry digests only.

#[path = "common.rs"]
#[allow(dead_code)]
mod common;

use std::io::{BufRead, Write};

use common::{error, Result};
use serde_json::{json, Value};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let r = match args.as_slice() {
        [cmd, flag, srs] if cmd == "serve" && flag == "--srs" => serve(srs),
        [cmd] if cmd == "identity" => {
            println!("{}", json!({"schema": "loki-bench-verifier-identity/1", "prover": "midnight-proofs 0.8.3 registry stock", "circuits": cfg!(feature = "circuits")}));
            Ok(())
        }
        _ => Err(error("usage: stock-verifier serve --srs K20_SRS | identity")),
    };
    if let Err(e) = r {
        eprintln!("stock-verifier: {e}");
        std::process::exit(1);
    }
}

#[cfg(not(feature = "circuits"))]
fn serve(_: &str) -> Result<()> {
    Err(error("built without the `circuits` feature"))
}

#[cfg(feature = "circuits")]
fn serve(srs: &str) -> Result<()> {
    let started = std::time::Instant::now();
    let v = chain::StockChain::load(std::path::Path::new(srs))?;
    println!(
        "{}",
        json!({"ready": true, "init_ms": started.elapsed().as_secs_f64() * 1e3,
               "prover": "midnight-proofs 0.8.3 registry stock", "status": common::status_bytes()})
    );
    std::io::stdout().flush()?;
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = line?;
        let req: Value = serde_json::from_str(&line)?;
        let path = |k: &str| std::path::PathBuf::from(req[k].as_str().unwrap_or_default());
        let t = std::time::Instant::now();
        let result = match req["stage"].as_str().unwrap_or_default() {
            "exit" => return Ok(()),
            "children" => v.children(&path("dir")),
            "parent" => v.parent(&path("dir"), &path("children")),
            "decider" => v.decider(&path("dir"), &path("parent")),
            "qualify" => v.qualify(&path("dir"), req["kind"].as_str().unwrap_or_default()),
            other => Err(error(format!("unknown stage {other}"))),
        };
        let receipt = match result {
            Ok(mut r) => {
                r["accepted"] = json!(true);
                r
            }
            Err(e) => json!({"accepted": false, "error": e.to_string()}),
        };
        let mut receipt = receipt;
        receipt["verifier_ms"] = json!(t.elapsed().as_secs_f64() * 1e3);
        receipt["verifier_status"] = common::status_bytes();
        println!("{receipt}");
        std::io::stdout().flush()?;
    }
    Ok(())
}

#[cfg(feature = "circuits")]
mod chain {
    use super::common::{self, error, Result};
    use ff::{Field, PrimeField};
    use group::Group;
    use midnight_circuits::{
        hash::poseidon::{PoseidonChip, PoseidonState},
        instructions::hash::HashCPU,
        types::Instantiable,
        verifier::{fixed_bases, Accumulator, AssignedAccumulator, BlstrsEmulation, Msm, SelfEmulation},
    };
    use midnight_curves::{Bls12, Fq};
    use midnight_proofs::{
        plonk,
        poly::kzg::{
            params::{ParamsKZG, ParamsVerifierKZG},
            KZGCommitmentScheme,
        },
        transcript::{CircuitTranscript, Transcript},
        utils::SerdeFormat,
    };
    use midnight_transient_crypto::{
        curve::Fr,
        proofs::{Proof, VerifierKey, PARAMS_VERIFIER},
    };
    use midnight_zk_stdlib::{batch_verify, MidnightVK};
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use std::{collections::BTreeMap, fs, io::Cursor, path::Path};

    type S = BlstrsEmulation;
    type AP = <S as SelfEmulation>::AssignedPoint;

    const SRS_LEN: u64 = 201_326_980;
    const SRS_SHA256: &str = "1cc62978558fdc1e445cd70cfd9a86ec3c2e2151b6d74811232d37faf9133ff1";
    /// Historical child VK pins from the pinned verify_chain example.
    const A102_VK_PIN: &str = "a6e6ed282daba635711e5d15863441a4888eb28407ec3899ee8722b5e1b02ab9";
    const A80_VK_PIN: &str = "65414136eabb682558c39e8a76d8d5f436641fe2d76a87bbe88a5b26b093c104";
    const INSTANCE_ID: &str = "489080d3e57cacab615cd0cde4c87914b47c4ff0ab5857d865ee2474ecd45c00";
    const ACCEPT_TEMPLATE: &[u8] =
        include_bytes!("../deps/moriarty/crates/mori-recursion/tests/fixtures/accept-template.json");

    fn field(s: &str) -> Result<Fq> {
        if s.len() != 64 {
            return Err(error("field hex must be 64 chars"));
        }
        let raw: [u8; 32] = hex::decode(s)?.try_into().map_err(|_| error("field length"))?;
        Option::from(Fq::from_repr(raw)).ok_or_else(|| error("non-canonical scalar"))
    }

    fn instance(p: &Path) -> Result<Vec<Fq>> {
        let lanes: Vec<String> = serde_json::from_slice(&fs::read(p)?)?;
        lanes.iter().map(|s| field(s)).collect()
    }

    /// Checked processed-VK decoding: must consume all bytes and round-trip.
    fn key(raw: &[u8]) -> Result<MidnightVK> {
        let mut reader = raw;
        let k = MidnightVK::read(&mut reader, SerdeFormat::Processed)?;
        if !reader.is_empty() {
            return Err(error("trailing VK bytes"));
        }
        let mut bytes = Vec::new();
        k.write(&mut bytes, SerdeFormat::Processed)?;
        if bytes != raw {
            return Err(error("VK does not round-trip"));
        }
        Ok(k)
    }

    fn domain(s: &str) -> Fq {
        let mut bytes = [0u8; 16];
        bytes[..s.len()].copy_from_slice(s.as_bytes());
        Fq::from_u128(u128::from_le_bytes(bytes))
    }

    fn hash(s: &str, xs: &[Fq]) -> Fq {
        <PoseidonChip<Fq> as HashCPU<Fq, Fq>>::hash(&[&[domain(s)], xs].concat())
    }

    struct Art {
        vk_raw: Vec<u8>,
        vk: MidnightVK,
        pi: Vec<Fq>,
        proof: Vec<u8>,
    }

    fn art(dir: &Path, name: &str) -> Result<Art> {
        let vk_raw = fs::read(dir.join(format!("{name}.vk")))?;
        Ok(Art {
            vk: key(&vk_raw)?,
            vk_raw,
            pi: instance(&dir.join(format!("{name}.instance.json")))?,
            proof: fs::read(dir.join(format!("{name}.proof")))?,
        })
    }

    fn ids(a: &Art) -> Value {
        json!({"vk_sha256": common::sha256_hex(&a.vk_raw), "proof_sha256": common::sha256_hex(&a.proof),
               "proof_bytes": a.proof.len(), "lanes": a.pi.len(), "k": a.vk.k()})
    }

    pub struct StockChain {
        vp: ParamsVerifierKZG<Bls12>,
        s_g2_ok: bool,
    }

    impl StockChain {
        pub fn load(srs: &Path) -> Result<Self> {
            if fs::metadata(srs)?.len() != SRS_LEN {
                return Err(error("SRS length differs from the pinned ceremony"));
            }
            let bytes = fs::read(srs)?;
            if common::sha256_hex(&bytes) != SRS_SHA256 {
                return Err(error("SRS digest differs from the pinned ceremony"));
            }
            let mut cursor = Cursor::new(bytes);
            let params = ParamsKZG::<Bls12>::read_custom(&mut cursor, SerdeFormat::RawBytesUnchecked)?;
            if cursor.position() != cursor.get_ref().len() as u64 {
                return Err(error("trailing SRS bytes"));
            }
            Ok(Self {
                vp: params.verifier_params(),
                s_g2_ok: true,
            })
        }

        /// One proof: full transcript consumption and final pairing.
        fn one(&self, a: &Art) -> Result<()> {
            batch_verify::<PoseidonState<Fq>>(
                &self.vp,
                std::slice::from_ref(&a.vk),
                std::slice::from_ref(&a.pi),
                std::slice::from_ref(&a.proof),
            )
            .map_err(|e| error(format!("stock verification failed: {e:?}")))
        }

        pub fn children(&self, dir: &Path) -> Result<Value> {
            let a = art(dir, "a102")?;
            let b = art(dir, "a80")?;
            self.one(&a)?;
            self.one(&b)?;
            let (x, y) = (&a.pi, &b.pi);
            if (x.len(), y.len()) != (419, 419) {
                return Err(error(format!("child lanes {}/{} (expected 419/419)", x.len(), y.len())));
            }
            if x[3..159] != y[3..159] || x[288..417] != y[159..288] {
                return Err(error("child linkage lanes differ"));
            }
            if x[417..] != [Fq::from(3), Fq::ONE] || y[417..] != [Fq::ZERO, Fq::ZERO] {
                return Err(error("child envelope lanes differ"));
            }
            Ok(json!({
                "stage": "children",
                "a102": ids(&a), "a80": ids(&b),
                "a102_vk_matches_historical_pin": common::sha256_hex(&a.vk_raw) == A102_VK_PIN,
                "a80_vk_matches_historical_pin": common::sha256_hex(&b.vk_raw) == A80_VK_PIN,
                "checks": ["a102 stock pairing+consumption", "a80 stock pairing+consumption", "419/419 lanes", "linkage", "envelopes"],
            }))
        }

        pub fn parent(&self, dir: &Path, children: &Path) -> Result<Value> {
            let a = art(children, "a102")?;
            let b = art(children, "a80")?;
            let w = art(dir, "w")?;
            self.one(&w)?;
            let (x, y, wp) = (&a.pi, &b.pi, &w.pi);
            if wp.len() != 28 {
                return Err(error("parent instance is not 28 lanes"));
            }
            let mut limbs = Vec::new();
            for raw in [&a.vk_raw, &b.vk_raw] {
                let digest = Sha256::digest(raw);
                for half in digest.chunks_exact(16) {
                    limbs.push(Fq::from_u128(u128::from_le_bytes(half.try_into().unwrap())));
                }
            }
            limbs.extend([Fq::from(419), Fq::from(156), Fq::from(129)]);
            let keys_id = hash("SHK_KEYS", &limbs);
            let prog = hash("SHK_PROG", &x[3..159]);
            let pre = hash("SHK_STATE", &x[159..288]);
            let mid = hash("SHK_STATE", &x[288..417]);
            let post = hash("SHK_STATE", &y[288..417]);
            let trace = hash(
                "SHK_TRACE",
                &[keys_id, x[1], x[2], y[1], y[2], prog, pre, Fq::ZERO, x[0], Fq::from(3), Fq::ONE, mid,
                  Fq::ONE, y[0], Fq::ZERO, Fq::ZERO, post, Fq::from(2), Fq::from(2), Fq::ONE],
            );
            let expected = [keys_id, x[1], x[2], y[1], y[2], prog, pre, Fq::ZERO, mid, post, Fq::from(2),
                            x[0], y[0], Fq::from(2), Fq::ONE, trace];
            if wp[..16] != expected {
                return Err(error("parent statement does not correspond to the children"));
            }
            // Rebuild the carried accumulator from the exact child proof bytes.
            let mut bases = BTreeMap::new();
            let mut owns = Vec::new();
            for (c, prefix) in [(&a, "sherlock_a102"), (&b, "sherlock_a80")] {
                let name = format!("{prefix}_{}", &common::sha256_hex(&c.vk_raw)[..16]);
                let fixed = fixed_bases::<S>(&name, c.vk.vk());
                let mut transcript = CircuitTranscript::<PoseidonState<Fq>>::init_from_bytes(&c.proof);
                let dual = plonk::prepare::<Fq, KZGCommitmentScheme<Bls12>, CircuitTranscript<PoseidonState<Fq>>>(
                    c.vk.vk(),
                    &[&[midnight_curves::G1Projective::identity()]],
                    &[&[&c.pi]],
                    &mut transcript,
                )?;
                transcript.assert_empty()?;
                owns.push(Accumulator::<S>::from_dual_msm(dual, &name, &fixed));
                bases.extend(fixed);
            }
            let mut folded = Accumulator::<S>::accumulate(&owns);
            folded.resolve_fixed_bases(&bases);
            folded.collapse();
            if <AssignedAccumulator<S> as Instantiable<Fq>>::as_public_input(&folded) != wp[16..] {
                return Err(error("carried accumulator differs from the children"));
            }
            let side = |fields: &[Fq]| -> Result<Msm<S>> {
                if fields[5] != Fq::ONE {
                    return Err(error("accumulator side flag"));
                }
                let point = <AP as Instantiable<Fq>>::from_public_input(&fields[..5])
                    .ok_or_else(|| error("accumulator point is not in the subgroup"))?;
                Ok(Msm::<S>::new(&[point], &[Fq::ONE], &BTreeMap::new()))
            };
            let carried = Accumulator::<S>::new(side(&wp[16..22])?, side(&wp[22..28])?);
            if !carried.check(&self.vp, &BTreeMap::new()) {
                return Err(error("parent carried pairing fails"));
            }
            Ok(json!({"stage": "parent", "w": ids(&w),
                      "checks": ["w stock pairing+consumption", "statement/children correspondence", "carried accumulator rebuild", "carried pairing"]}))
        }

        pub fn decider(&self, dir: &Path, parent: &Path) -> Result<Value> {
            let w = art(parent, "w")?;
            let vk_raw = fs::read(dir.join("key/decider.vk"))?;
            let vk = key(&vk_raw)?;
            let public = instance(&dir.join("terminal.public.json"))?;
            let proof = fs::read(dir.join("terminal.proof"))?;
            if public.len() != 15 {
                return Err(error("terminal public vector is not 15 lanes"));
            }
            let template: Value = serde_json::from_slice(ACCEPT_TEMPLATE)?;
            for (slot, value) in template["constants"].as_object().ok_or_else(|| error("template"))? {
                if public[slot.parse::<usize>()?] != field(value.as_str().unwrap_or_default())? {
                    return Err(error(format!("terminal constant slot {slot} differs")));
                }
            }
            let id = field(INSTANCE_ID)?;
            if public[10] != hash("SHK_STMT", &[&[id, Fq::ZERO], &w.pi[..16]].concat()) {
                return Err(error("terminal statement does not bind the parent"));
            }
            batch_verify::<blake2b_simd::State>(
                &self.vp,
                std::slice::from_ref(&vk),
                std::slice::from_ref(&public),
                std::slice::from_ref(&proof),
            )
            .map_err(|e| error(format!("decider stock verification failed: {e:?}")))?;
            // Native ledger-format acceptance (offline; no deployment).
            let ledger = VerifierKey::from(vk.clone());
            ledger
                .verify(&PARAMS_VERIFIER, &Proof(proof.clone()), public.iter().copied().map(Fr))
                .map_err(|e| error(format!("ledger-format verification failed: {e:?}")))?;
            let tagged = fs::read(dir.join("key/acceptValidity.verifier"))?;
            let decoded: VerifierKey = midnight_serialize::tagged_deserialize(&tagged[..])?;
            if decoded.original_bytes() != vk_raw {
                return Err(error("tagged ledger key differs from the decider VK"));
            }
            Ok(json!({"stage": "decider", "vk_sha256": common::sha256_hex(&vk_raw), "k": vk.k(),
                      "proof_sha256": common::sha256_hex(&proof), "proof_bytes": proof.len(),
                      "s_g2_from_authenticated_srs": self.s_g2_ok,
                      "checks": ["decider stock pairing+consumption (blake2b)", "template constants", "statement binds parent", "ledger-format VerifierKey accept", "tagged key round trip"]}))
        }

        /// Qualification-only mutation controls (outside timed receipts).
        pub fn qualify(&self, dir: &Path, kind: &str) -> Result<Value> {
            let mut refused = 0usize;
            let mut attempts = 0usize;
            let mut must_reject = |r: Result<()>| -> Result<()> {
                attempts += 1;
                if r.is_ok() {
                    return Err(error(format!("mutation {attempts} was accepted")));
                }
                refused += 1;
                Ok(())
            };
            let names: &[&str] = match kind {
                "children" => &["a102", "a80"],
                "parent" => &["w"],
                "decider" => &[],
                _ => return Err(error("kind must be children|parent|decider")),
            };
            for name in names {
                let a = art(dir, name)?;
                self.one(&a)?;
                for lane in 0..a.pi.len() {
                    let mut bad = Art { vk_raw: a.vk_raw.clone(), vk: key(&a.vk_raw)?, pi: a.pi.clone(), proof: a.proof.clone() };
                    bad.pi[lane] += Fq::ONE;
                    must_reject(self.one(&bad))?;
                    if kind == "children" && lane >= 8 {
                        break; // 419 lanes per child: sample the first nine
                    }
                }
                for mutate in 0..4 {
                    let mut p = a.proof.clone();
                    match mutate {
                        0 => p.clear(),
                        1 => p.truncate(p.len() - 1),
                        2 => p.push(0),
                        _ => p[0] ^= 1,
                    }
                    let bad = Art { vk_raw: a.vk_raw.clone(), vk: key(&a.vk_raw)?, pi: a.pi.clone(), proof: p };
                    must_reject(self.one(&bad))?;
                }
            }
            if kind == "decider" {
                let vk = key(&fs::read(dir.join("key/decider.vk"))?)?;
                let public = instance(&dir.join("terminal.public.json"))?;
                let proof = fs::read(dir.join("terminal.proof"))?;
                let one = |pi: &Vec<Fq>, pr: &Vec<u8>| -> Result<()> {
                    batch_verify::<blake2b_simd::State>(&self.vp, std::slice::from_ref(&vk), std::slice::from_ref(pi), std::slice::from_ref(pr))
                        .map_err(|e| error(format!("{e:?}")))
                };
                one(&public, &proof)?;
                for lane in 0..public.len() {
                    let mut bad = public.clone();
                    bad[lane] += Fq::ONE;
                    must_reject(one(&bad, &proof))?;
                }
                let mut bad = proof.clone();
                bad[0] ^= 1;
                must_reject(one(&public, &bad))?;
                let mut ext = proof.clone();
                ext.push(0);
                must_reject(one(&public, &ext))?;
            }
            Ok(json!({"stage": "qualify", "kind": kind, "attempts": attempts, "refused": refused}))
        }
    }
}
