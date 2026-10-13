//! Exhaustive RIP-relative effective-address parity.
//!
//! In 64-bit mode ModR/M `mod=00 rm=101` addresses `RIP + disp32`, where RIP
//! is the address of the next instruction (Intel SDM Vol. 2A section
//! 2.2.1.6): the immediate after the displacement counts. Each decoder path
//! must therefore know the full instruction length before it forms the
//! address, and the legacy 0F/0F3A maps once did not.
//!
//! For every opcode of every encoding family and every ModR/M.reg value (see
//! `corpus`), the sweep places the operand at the unmapped page `TARGET`
//! using the length an independent decoder reports (the strict SMIR lifter,
//! else the stateless metadata decoder) and checks:
//!
//! - the interpreter faults at exactly `TARGET` on the decode-cache miss and
//!   hit paths and in the threaded interpreter;
//! - with `TARGET` mapped, the interpreter's own retired length predicts the
//!   address it faulted at (`fault == CODE + length + disp`), which needs no
//!   other decoder, and agrees with the lifter's length;
//! - the lifted SMIR faults at (or branches through) exactly `TARGET`;
//! - an instruction that one engine reads the operand for, the other does
//!   too; LEA produces `TARGET`.
//!
//! Exceptions, unsupported encodings, and accesses through a register are
//! counted per family and reported, not compared. Divergences must equal
//! `cpu_rip_relative_sweep_known.txt` exactly, and the number of verified
//! cases per family must not fall below `FLOORS`.

#[path = "cpu_rip_relative_sweep_tests/corpus.rs"]
mod corpus;
#[path = "cpu_rip_relative_sweep_tests/machine.rs"]
mod machine;

use std::collections::BTreeMap;
use std::fmt::Write as _;

use corpus::{Case, FAMILIES, Family, corpus};
use machine::{CODE, Interp, Machine, Smir, TARGET, initial_gpr, is_poison};

use crate::isa::x86_64::instruction_metadata as metadata;
use crate::smir::ir::types::SourceArch;
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, LiftResult, SmirLifter};

/// Minimum verified cases per family; see the report's "raise floor" hints.
const FLOORS: [(Family, usize); 7] = [
    (Family::Legacy, 17_094),
    (Family::Addr32, 4_561),
    (Family::Rex2, 2_312),
    (Family::Vex, 11_214),
    (Family::Evex, 19_110),
    (Family::Xop, 354),
    (Family::Map4, 7_132),
];

/// Expected divergences: `family head-hex kind detail reason...`.
const KNOWN: &str = include_str!("cpu_rip_relative_sweep_known.txt");

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Kind {
    /// The interpreter's fault address is not `TARGET`.
    InterpEa,
    /// The SMIR fault or branch-through address is not `TARGET`.
    SmirEa,
    /// The interpreter's fault address disagrees with its own retired length.
    SelfEa,
    /// Two decoders disagree on the instruction length.
    Length,
    /// The decode-cache hit path ended differently from the miss path.
    CacheHit,
    /// The threaded interpreter ended differently from `step()`.
    Threaded,
    /// One engine read the operand and the other completed without it.
    Access,
    /// LEA did not produce `TARGET`.
    Lea,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Divergence {
    family: Family,
    head: String,
    kind: Kind,
    detail: String,
}

#[derive(Default)]
struct Tally {
    cases: usize,
    verified: usize,
    /// Reason -> count, for cases with no address check.
    skipped: BTreeMap<String, usize>,
    /// Interpreter/lifter acceptance asymmetries (reported, not compared).
    asymmetric: BTreeMap<String, usize>,
}

struct Outcome {
    verified: bool,
    skip: Option<String>,
    asymmetry: Option<String>,
    divergences: Vec<(Kind, String)>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

/// The strict lifter's result, unless it rejected the encoding or lifted it
/// to a trap (#UD and similar end execution, so their length has no effect).
fn lift(lifter: &mut X86_64Lifter, bytes: &[u8]) -> Option<LiftResult> {
    lifter
        .lift_insn(CODE, bytes, &mut LiftContext::new(SourceArch::X86_64))
        .ok()
        .filter(|lifted| !matches!(lifted.control_flow, ControlFlow::Trap { .. }))
}

fn falls_through(lifted: &LiftResult) -> bool {
    matches!(
        lifted.control_flow,
        ControlFlow::Fallthrough | ControlFlow::NextInsn
    )
}

/// Where the first faulting byte of the memory operand may lie, as
/// `(start, size)` for an operand whose effective address is `TARGET`.
///
/// Usually exactly `TARGET`. The SDM fixes no access order within an operand,
/// so for the multi-byte save areas any byte of the operand may fault first:
/// FXSAVE/FXRSTOR (0F AE /0 /1, m512byte) and the XSAVE family (0F AE /4-/6,
/// 0F C7 /3-/5, at most the 4 KiB page). BT/BTS/BTR/BTC with a register bit
/// offset (0F A3/AB/B3/BB) address the operand-size unit at
/// `EA + size * (offset SAR log2(bits))` (SDM Vol. 2A BT, "bit base").
fn operand_window(case: &Case) -> (u64, u64) {
    let Some(form) = case.legacy_form().filter(|form| form.map == 1) else {
        return (TARGET, 1);
    };
    match (form.opcode, form.reg) {
        (0xA3 | 0xAB | 0xB3 | 0xBB, reg) => {
            let offset = initial_gpr(reg);
            let unit = if form.rex_w {
                8 * ((offset as i64) >> 6)
            } else if form.opsize16 {
                2 * i64::from(offset as i16 >> 4)
            } else {
                4 * i64::from(offset as i32 >> 5)
            };
            let start = TARGET.wrapping_add(unit as u64);
            let start = if case.family == Family::Addr32 {
                start & 0xFFFF_FFFF
            } else {
                start
            };
            (start, 1)
        }
        (0xAE, 0 | 1) => (TARGET, 512),
        (0xAE, 4..=6) | (0xC7, 3..=5) => (TARGET, 0x1000),
        _ => (TARGET, 1),
    }
}

/// Is `address` inside `window`, i.e. `start <= address < start + size`?
fn within(address: u64, (start, size): (u64, u64)) -> bool {
    address.wrapping_sub(start) < size
}

/// The signed distance of `address` from `start`.
fn delta(address: u64, start: u64) -> String {
    let d = address.wrapping_sub(start) as i64;
    format!("{d:+}")
}

/// LEA (map 0, opcode 8D) and its operand size in bits, for the legacy and
/// REX2 families. The destination is ModR/M.reg (no register extension is
/// set in the corpus).
fn lea_operand_bits(case: &Case) -> Option<u32> {
    let form = case.legacy_form()?;
    (form.map == 0 && form.opcode == 0x8D).then_some(if form.rex_w {
        64
    } else if form.opsize16 {
        16
    } else {
        32
    })
}

fn check(machine: &mut Machine, lifter: &mut X86_64Lifter, case: &Case) -> Outcome {
    let mut out = Outcome {
        verified: false,
        skip: None,
        asymmetry: None,
        divergences: Vec::new(),
    };
    let probe = case.bytes(0);
    let lift_len = lift(lifter, &probe).map(|lifted| lifted.bytes_consumed);
    let meta_len = metadata::decode(64, CODE, &probe)
        .ok()
        .map(|insn| insn.size as usize);
    if let (Some(l), Some(m)) = (lift_len, meta_len)
        && l != m
    {
        out.divergences
            .push((Kind::Length, format!("lifter={l} metadata={m}")));
    }
    let len = lift_len.or(meta_len);
    let assumed = len.unwrap_or(case.disp_end());
    let disp = TARGET.wrapping_sub(CODE + assumed as u64) as u32;
    let bytes = case.bytes(disp);
    let lifted = lift(lifter, &bytes);
    let window = operand_window(case);

    let first = machine.step(&bytes, false);
    let smir = lifted.as_ref().map(|lifted| machine.smir(lifted));

    match &first {
        Interp::Fault(address) if !is_poison(*address) => {
            let hit = machine.step(&bytes, false);
            if hit != first {
                out.divergences
                    .push((Kind::CacheHit, format!("miss={first:?} hit={hit:?}")));
            }
            if matches!(case.family, Family::Legacy | Family::Addr32 | Family::Rex2) {
                let threaded = machine.threaded(&bytes);
                if threaded != first {
                    out.divergences.push((
                        Kind::Threaded,
                        format!("step={first:?} threaded={threaded:?}"),
                    ));
                }
            }
            // With the operand mapped the instruction retires; a fall-through
            // RIP is the interpreter's own length, and it predicts the
            // operand address with no other decoder involved.
            let disp_end = CODE + case.disp_end() as u64;
            if let Interp::Retired(rip) = machine.step(&bytes, true)
                && (disp_end..=disp_end + 4).contains(&rip)
            {
                let own = rip
                    .wrapping_add(disp as i32 as i64 as u64)
                    .wrapping_add(window.0.wrapping_sub(TARGET));
                let own = if case.family == Family::Addr32 {
                    own & 0xFFFF_FFFF
                } else {
                    own
                };
                if !within(*address, (own, window.1)) {
                    out.divergences.push((
                        Kind::SelfEa,
                        format!(
                            "fault{} retired-length={}",
                            delta(*address, own),
                            rip - CODE
                        ),
                    ));
                }
                if let Some(l) = lift_len
                    && rip - CODE != l as u64
                    && lifted.as_ref().is_some_and(falls_through)
                {
                    out.divergences.push((
                        Kind::Length,
                        format!("lifter={l} interpreter={}", rip - CODE),
                    ));
                }
                out.verified = true;
            }
            if len.is_some() {
                if !within(*address, window) {
                    out.divergences
                        .push((Kind::InterpEa, delta(*address, window.0)));
                }
                out.verified = true;
            }
            if smir == Some(Smir::NoAccess) {
                out.divergences
                    .push((Kind::Access, "interpreter read, SMIR did not".into()));
            }
            if lifted.is_none() {
                out.asymmetry = Some("interpreter reads, lifter rejects".into());
            }
            if !out.verified {
                out.skip = Some("fault, no length witness".into());
            }
        }
        Interp::Fault(_) => out.skip = Some("register access first".into()),
        Interp::Event(vector) => {
            out.skip = Some(format!("interpreter vector {vector}"));
            if lifted.is_some() {
                out.asymmetry = Some(format!("interpreter vector {vector}, lifter accepts"));
            }
        }
        Interp::Retired(rip) => {
            out.skip = Some("retired without memory".into());
            if let Some(bits) = lea_operand_bits(case) {
                let reg = (case.head[case.head.len() - 1] >> 3) & 7;
                let value = machine.gpr(reg);
                let want = match bits {
                    64 => TARGET,
                    32 => TARGET & 0xFFFF_FFFF,
                    _ => (initial_gpr(reg) & !0xFFFF) | (TARGET & 0xFFFF),
                };
                if value != want {
                    out.divergences
                        .push((Kind::Lea, format!("{value:#x}_want_{want:#x}")));
                }
                out.verified = true;
            }
            if let Some(lifted) = &lifted
                && falls_through(lifted)
                && *rip != CODE + lifted.bytes_consumed as u64
            {
                out.divergences.push((
                    Kind::Length,
                    format!(
                        "lifter={} interpreter={}",
                        lifted.bytes_consumed,
                        rip.wrapping_sub(CODE)
                    ),
                ));
            }
            if matches!(smir, Some(Smir::Fault(address)) if !is_poison(address)) {
                out.divergences
                    .push((Kind::Access, "SMIR read, interpreter did not".into()));
            }
            if lifted.is_none() {
                out.asymmetry = Some("interpreter retires, lifter rejects".into());
            }
        }
        Interp::Other(what) => {
            let what = what.split([' ', '(', '{']).next().unwrap_or("").to_string();
            out.skip = Some(format!("interpreter {what}"));
        }
    }

    match smir {
        Some(Smir::Fault(address) | Smir::BranchThrough(address)) if !is_poison(address) => {
            if !within(address, window) {
                out.divergences
                    .push((Kind::SmirEa, delta(address, window.0)));
            }
            out.verified = true;
        }
        Some(Smir::Other(what)) if !out.verified => {
            let what = what.split([' ', '(', '{']).next().unwrap_or("").to_string();
            out.asymmetry.get_or_insert(format!("SMIR {what}"));
        }
        _ => {}
    }
    if out.verified {
        out.skip = None;
    }
    out
}

fn known() -> BTreeMap<(Family, String, Kind, String), String> {
    let mut map = BTreeMap::new();
    for line in KNOWN.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        assert!(fields.len() >= 5, "known divergence needs a reason: {line}");
        let family = FAMILIES
            .into_iter()
            .find(|family| format!("{family:?}") == fields[0])
            .unwrap_or_else(|| panic!("unknown family in {line}"));
        let kind = [
            Kind::InterpEa,
            Kind::SmirEa,
            Kind::SelfEa,
            Kind::Length,
            Kind::CacheHit,
            Kind::Threaded,
            Kind::Access,
            Kind::Lea,
        ]
        .into_iter()
        .find(|kind| format!("{kind:?}") == fields[2])
        .unwrap_or_else(|| panic!("unknown kind in {line}"));
        let key = (family, fields[1].to_string(), kind, fields[3].to_string());
        assert!(
            map.insert(key, fields[4..].join(" ")).is_none(),
            "duplicate known divergence: {line}"
        );
    }
    map
}

#[test]
fn rip_relative_operands_resolve_after_the_whole_instruction() {
    let started = std::time::Instant::now();
    let cases = corpus();
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    let chunk = cases.len().div_ceil(workers);
    let results: Vec<Vec<(usize, Outcome)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = cases
            .chunks(chunk)
            .enumerate()
            .map(|(n, slice)| {
                scope.spawn(move || {
                    let mut machine = Machine::new();
                    let mut lifter = X86_64Lifter::strict();
                    slice
                        .iter()
                        .enumerate()
                        .map(|(i, case)| (n * chunk + i, check(&mut machine, &mut lifter, case)))
                        .collect()
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut tallies: BTreeMap<Family, Tally> = BTreeMap::new();
    let mut found = Vec::new();
    for (index, outcome) in results.into_iter().flatten() {
        let case = &cases[index];
        let tally = tallies.entry(case.family).or_default();
        tally.cases += 1;
        tally.verified += usize::from(outcome.verified);
        if let Some(skip) = outcome.skip {
            *tally.skipped.entry(skip).or_default() += 1;
        }
        if let Some(asymmetry) = outcome.asymmetry {
            *tally.asymmetric.entry(asymmetry).or_default() += 1;
        }
        for (kind, detail) in outcome.divergences {
            found.push(Divergence {
                family: case.family,
                head: hex(&case.head),
                kind,
                detail,
            });
        }
    }
    found.sort();

    let mut report = String::new();
    writeln!(
        report,
        "RIP-relative sweep: {} cases in {:.1?}",
        cases.len(),
        started.elapsed()
    )
    .unwrap();
    for (family, floor) in FLOORS {
        let tally = tallies.get(&family).unwrap();
        writeln!(
            report,
            "  {family:?}: {} cases, {} verified (floor {floor}){}",
            tally.cases,
            tally.verified,
            if tally.verified > floor + floor / 20 {
                format!("; raise floor to {}", tally.verified)
            } else {
                String::new()
            }
        )
        .unwrap();
        for (reason, count) in &tally.skipped {
            writeln!(report, "      skipped {count:>6}: {reason}").unwrap();
        }
        for (asymmetry, count) in &tally.asymmetric {
            writeln!(report, "      asymmetric {count:>6}: {asymmetry}").unwrap();
        }
    }

    let known = known();
    let mut unexpected = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for d in &found {
        let key = (d.family, d.head.clone(), d.kind, d.detail.replace(' ', "_"));
        if known.contains_key(&key) {
            seen.insert(key);
        } else {
            unexpected.push(d);
        }
    }
    let resolved: Vec<_> = known.keys().filter(|key| !seen.contains(*key)).collect();

    let mut groups: BTreeMap<(Family, Kind, String), usize> = BTreeMap::new();
    for d in &unexpected {
        let map = d.head.get(..d.head.len().saturating_sub(4)).unwrap_or("");
        *groups
            .entry((d.family, d.kind, map.to_string()))
            .or_default() += 1;
    }
    writeln!(report, "{} unexpected divergences", unexpected.len()).unwrap();
    for ((family, kind, map), count) in &groups {
        writeln!(report, "  {count:>6} {family:?} {kind:?} {map}..").unwrap();
    }
    const SHOWN: usize = 400;
    for d in unexpected.iter().take(SHOWN) {
        writeln!(
            report,
            "    {:?} {} {:?} {}",
            d.family, d.head, d.kind, d.detail
        )
        .unwrap();
    }
    if unexpected.len() > SHOWN {
        writeln!(report, "    ... and {} more", unexpected.len() - SHOWN).unwrap();
    }
    for key in &resolved {
        writeln!(report, "  known divergence no longer occurs: {key:?}").unwrap();
    }
    eprintln!("{report}");

    let below: Vec<_> = FLOORS
        .into_iter()
        .filter(|(family, floor)| tallies[family].verified < *floor)
        .collect();
    assert!(
        unexpected.is_empty() && resolved.is_empty() && below.is_empty(),
        "RIP-relative sweep failed (verified below floor: {below:?}); see the report above"
    );
}
