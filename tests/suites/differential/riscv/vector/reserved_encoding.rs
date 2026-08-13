//! Reserved-encoding and architectural-constraint differential coverage.

use super::*;

#[test]
fn diff_v_reserved_encoding_validation() {
    const E8_M1: u64 = 0x00;
    const E8_M2: u64 = 0x01;
    const E16_MF8: u64 = 0x0d;
    const E32_M1: u64 = 0x10;
    const E32_M2: u64 = 0x11;
    const E32_M4: u64 = 0x12;
    const E32_M8: u64 = 0x13;
    const E32_MF4: u64 = 0x16;
    const E32_MF2: u64 = 0x17;
    const E64_M8: u64 = 0x1b;

    fn state(vtype: u64, vl: u64) -> VState {
        let mut rng = Rng::new(0x7EC_19_201);
        let mut st = VState::zeroed();
        for value in &mut st.x[1..] {
            *value = rng.next();
        }
        for value in &mut st.f {
            *value = 0xffff_ffff_0000_0000 | (rng.next() as u32 as u64);
        }
        for value in &mut st.v {
            *value = rng.next();
        }
        for value in &mut st.scratch {
            *value = rng.next();
        }
        st.vtype = vtype;
        st.vl = vl;
        st
    }

    let mut batch = Vec::new();

    // vmsbf.m/vmsof.m/vmsif.m: vd must not overlap vs2, masked forms cannot
    // target v0, and the operations are not restartable.
    for (name, selector) in [
        ("vmsbf.m", 0b00001),
        ("vmsof.m", 0b00010),
        ("vmsif.m", 0b00011),
    ] {
        batch.push((
            format!("{name}.vd-vs2-overlap"),
            op_iv(0b010100, 1, 2, selector, 0b010, 2),
            state(E8_M1, 8),
        ));
        batch.push((
            format!("{name}.masked-vd-v0"),
            op_iv(0b010100, 0, 2, selector, 0b010, 0),
            state(E8_M1, 8),
        ));
        let mut nonrestartable = state(E8_M1, 8);
        nonrestartable.vstart = 1;
        batch.push((
            format!("{name}.vstart"),
            op_iv(0b010100, 1, 2, selector, 0b010, 1),
            nonrestartable,
        ));
    }

    // vid.v is VMUNARY0: vs2 is reserved and must encode v0 in both masked
    // and unmasked forms.
    for (name, vm, vs2) in [
        ("vid.v.vs2-v16", 1, 16),
        ("vid.v.masked-vs2-v3", 0, 3),
        ("vid.v.control", 1, 0),
        ("vid.v.masked-control", 0, 0),
    ] {
        batch.push((
            name.into(),
            op_iv(0b010100, vm, vs2, 0b10001, 0b010, 1),
            state(E8_M1, 8),
        ));
    }

    // Vector/scalar moves reserve every masked encoding in both directions.
    for (name, funct3, vs2, src, vd) in [
        ("vmv.x.s", 0b010, 2, 0, 1),
        ("vmv.s.x", 0b110, 0, 5, 2),
        ("vfmv.f.s", 0b001, 2, 0, 1),
        ("vfmv.s.f", 0b101, 0, 5, 2),
    ] {
        batch.push((
            format!("{name}.masked-reserved"),
            op_iv(0b010000, 0, vs2, src, funct3, vd),
            state(E32_M1, 4),
        ));
        batch.push((
            format!("{name}.unmasked-control"),
            op_iv(0b010000, 1, vs2, src, funct3, vd),
            state(E32_M1, 4),
        ));
    }

    // vrgatherei16.vv derives index EMUL=(16/SEW)*LMUL. At e8,m2 the
    // index occupies four registers and must start at a multiple of four.
    for (name, index) in [("misaligned-index", 6), ("aligned-index-control", 8)] {
        batch.push((
            format!("vrgatherei16.vv.{name}"),
            op_iv(0b001110, 1, 2, index, 0b000, 0),
            state(E8_M2, 4),
        ));
    }
    batch.push((
        "vrgatherei16.vv.mixed-eew-source-alias".into(),
        op_iv(0b001110, 1, 2, 2, 0b000, 0),
        state(E8_M1, 4),
    ));
    batch.push((
        "vrgatherei16.vv.same-eew-source-alias-control".into(),
        op_iv(0b001110, 1, 2, 2, 0b000, 0),
        state(0x09, 4), // e16,m2
    ));

    // Averaging add/subtract forms use complete LMUL-sized groups for every
    // vector operand; vx forms retain scalar rs1.
    for (name, funct6) in [
        ("vaaddu", 0b001000),
        ("vaadd", 0b001001),
        ("vasubu", 0b001010),
        ("vasub", 0b001011),
    ] {
        batch.push((
            format!("{name}.vv.misaligned-vd"),
            op_iv(funct6, 1, 2, 4, 0b010, 1),
            state(E32_M2, 2),
        ));
        batch.push((
            format!("{name}.vv.misaligned-vs1"),
            op_iv(funct6, 1, 2, 5, 0b010, 0),
            state(E32_M2, 2),
        ));
        batch.push((
            format!("{name}.vx.scalar-rs1-control"),
            op_iv(funct6, 1, 2, 5, 0b110, 0),
            state(E32_M2, 2),
        ));
    }

    // viota.m is nonrestartable; vd may overlap neither vs2 nor the masked
    // execution register v0.
    batch.push((
        "viota.m.masked-vd-v0".into(),
        op_iv(0b010100, 0, 2, 0b10000, 0b010, 0),
        state(E8_M1, 4),
    ));
    batch.push((
        "viota.m.vd-vs2-overlap".into(),
        op_iv(0b010100, 1, 2, 0b10000, 0b010, 2),
        state(E8_M1, 4),
    ));
    let mut iota_nonrestartable = state(E8_M1, 4);
    iota_nonrestartable.vstart = 1;
    batch.push((
        "viota.m.nonzero-vstart".into(),
        op_iv(0b010100, 1, 4, 0b10000, 0b010, 2),
        iota_nonrestartable,
    ));
    for (name, vm) in [("masked-control", 0), ("unmasked-control", 1)] {
        batch.push((
            format!("viota.m.{name}"),
            op_iv(0b010100, vm, 4, 0b10000, 0b010, 2),
            state(E8_M1, 4),
        ));
    }

    // vadc/vsbc consume v0 as carry/borrow-in, so vm=1 and vd=v0 are reserved
    // for every defined vv/vx/vi form.
    for (name, funct6, funct3) in [
        ("vadc.vvm", 0b010000, 0b000),
        ("vadc.vxm", 0b010000, 0b100),
        ("vadc.vim", 0b010000, 0b011),
        ("vsbc.vvm", 0b010010, 0b000),
        ("vsbc.vxm", 0b010010, 0b100),
    ] {
        batch.push((
            format!("{name}.vm-one"),
            op_iv(funct6, 1, 2, 3, funct3, 1),
            state(E8_M1, 8),
        ));
        batch.push((
            format!("{name}.vd-v0"),
            op_iv(funct6, 0, 2, 3, funct3, 0),
            state(E8_M1, 8),
        ));
    }
    batch.push((
        "vmul.vv.aligned-control".into(),
        op_iv(0b100101, 1, 2, 4, 0b010, 0),
        state(E32_M2, 2),
    ));

    // Upward slides prohibit any source/destination group overlap. The exact
    // slide-by-one encodings use OPMVX/OPFVF funct3 values 110/101. Downward
    // forms allow overlap and serve as differential controls.
    for (name, funct3) in [
        ("vslideup.vx", 0b100),
        ("vslideup.vi", 0b011),
        ("vslide1up.vx", 0b110),
        ("vfslide1up.vf", 0b101),
    ] {
        let src = if matches!(funct3, 0b100 | 0b110) {
            5
        } else {
            3
        };
        batch.push((
            format!("{name}.overlap"),
            op_iv(0b001110, 1, 2, src, funct3, 2),
            state(E32_M2, 4),
        ));
    }
    for (name, funct3) in [
        ("vslidedown.vx", 0b100),
        ("vslidedown.vi", 0b011),
        ("vslide1down.vx", 0b110),
        ("vfslide1down.vf", 0b101),
    ] {
        let src = if matches!(funct3, 0b100 | 0b110) {
            5
        } else {
            3
        };
        batch.push((
            format!("{name}.overlap-control"),
            op_iv(0b001111, 1, 2, src, funct3, 2),
            state(E32_M2, 4),
        ));
    }

    // Complete same-width groups, mask-result overlap, and the generic masked
    // destination rule are all checked before any architectural state changes.
    for (name, funct6, funct3) in [
        ("vmul.vv", 0b100101, 0b010),
        ("vdivu.vv", 0b100000, 0b010),
        ("vsaddu.vv", 0b100000, 0b000),
        ("vssrl.vv", 0b101010, 0b000),
        ("vsmul.vv", 0b100111, 0b000),
    ] {
        batch.push((
            format!("{name}.misaligned-vd"),
            op_iv(funct6, 1, 2, 4, funct3, 1),
            state(E32_M2, 2),
        ));
    }
    batch.push((
        "vslide1down.vx.misaligned-vs2".into(),
        op_iv(0b001111, 1, 3, 5, 0b110, 2),
        state(E32_M2, 2),
    ));
    batch.push((
        "vid.v.misaligned-vd".into(),
        op_iv(0b010100, 1, 0, 0b10001, 0b010, 1),
        state(E32_M2, 2),
    ));
    batch.push((
        "vmseq.vv.nonlowest-source-overlap".into(),
        op_iv(0b011000, 1, 2, 4, 0b000, 3),
        state(E32_M2, 2),
    ));
    batch.push((
        "vmadc.vv.nonlowest-source-overlap".into(),
        op_iv(0b010001, 1, 2, 4, 0b000, 3),
        state(E32_M2, 2),
    ));
    for (name, funct6) in [("vmseq.vv", 0b011000), ("vmadc.vv", 0b010001)] {
        batch.push((
            format!("{name}.lowest-source-overlap-control"),
            op_iv(funct6, 1, 2, 4, 0b000, 2),
            state(E32_M2, 2),
        ));
    }
    batch.push((
        "vadd.vv.masked-vd-v0".into(),
        op_iv(0b000000, 0, 2, 4, 0b000, 0),
        state(E32_M2, 2),
    ));
    batch.push((
        "vmseq.vv.masked-vd-v0-control".into(),
        op_iv(0b011000, 0, 2, 4, 0b000, 0),
        state(E32_M2, 2),
    ));

    // Vector memory operands use data or index EMUL as selected by the
    // addressing mode; segment fields each start at an aligned group.
    let vle32_misaligned = (1 << 25) | (10 << 15) | (0b110 << 12) | (1 << 7) | 0x07;
    batch.push((
        "vle32.v.misaligned-vd".into(),
        vle32_misaligned,
        state(E32_M2, 2),
    ));
    let vluxei64_misaligned_index =
        (0b01 << 26) | (1 << 25) | (2 << 20) | (10 << 15) | (0b111 << 12) | (4 << 7) | 0x07;
    batch.push((
        "vluxei64.v.misaligned-index".into(),
        vluxei64_misaligned_index,
        state(E32_M2, 2),
    ));
    let vlseg2e32_misaligned = (1 << 29) | (1 << 25) | (10 << 15) | (0b110 << 12) | (1 << 7) | 0x07;
    batch.push((
        "vlseg2e32.v.misaligned-vd".into(),
        vlseg2e32_misaligned,
        state(E32_M2, 2),
    ));
    batch.push((
        "vlseg2e32.v.aligned-control".into(),
        (1 << 29) | (1 << 25) | (10 << 15) | (0b110 << 12) | (2 << 7) | 0x07,
        {
            let mut control = state(E32_M2, 2);
            control.x[10] = SCRATCH_BASE;
            control
        },
    ));
    batch.push((
        "vluxseg2ei32.v.destination-index-overlap".into(),
        (1 << 29)
            | (0b01 << 26)
            | (1 << 25)
            | (4 << 20)
            | (10 << 15)
            | (0b110 << 12)
            | (4 << 7)
            | 0x07,
        state(E32_M2, 2),
    ));

    // Whole-register moves resume in SEW-sized effective elements. Segment
    // fault-only-first loads with nf>0 are legal unit-stride encodings.
    let mut vmvr_resume = state(E32_M1, 0);
    vmvr_resume.vstart = 2;
    batch.push((
        "vmv2r.v.vstart-two".into(),
        op_iv(0b100111, 1, 2, 1, 0b011, 4),
        vmvr_resume,
    ));
    let mut segment_fof = state(E8_M1, 2);
    segment_fof.x[10] = SCRATCH_BASE;
    batch.push((
        "vlseg2e8ff.v.control".into(),
        (1 << 29) | (1 << 25) | (0b10000 << 20) | (10 << 15) | (1 << 7) | 0x07,
        segment_fof,
    ));

    let narrowing: Vec<(&str, u32, u32)> = [
        ("vnsrl.wv", 0b101100, 0),
        ("vnsra.wv", 0b101101, 0),
        ("vnclipu.wv", 0b101110, 0),
        ("vnclip.wv", 0b101111, 0),
    ]
    .into_iter()
    .chain(
        [
            "vfncvt.xu.f.w",
            "vfncvt.x.f.w",
            "vfncvt.f.xu.w",
            "vfncvt.f.x.w",
            "vfncvt.f.f.w",
            "vfncvt.rod.f.f.w",
            "vfncvt.rtz.xu.f.w",
            "vfncvt.rtz.x.f.w",
        ]
        .into_iter()
        .enumerate()
        .map(|(index, name)| (name, 0b010010, 0b10000 + index as u32)),
    )
    .collect();
    for (name, funct6, selector) in narrowing {
        let funct3 = if funct6 == 0b010010 && selector >= 0b10000 {
            0b001
        } else {
            0b000
        };
        let src = if funct3 == 0b001 { selector } else { 4 };
        batch.push((
            format!("{name}.upper-overlap"),
            op_iv(funct6, 1, 2, src, funct3, 3),
            state(E32_M1, 2),
        ));
        batch.push((
            format!("{name}.same-lowest-control"),
            op_iv(funct6, 1, 2, src, funct3, 2),
            state(E32_M1, 2),
        ));
        batch.push((
            format!("{name}.fractional-odd-control"),
            op_iv(funct6, 1, 1, src, funct3, 1),
            state(E32_MF2, 2),
        ));
        batch.push((
            format!("{name}.emul-16"),
            op_iv(funct6, 1, 0, src, funct3, 0),
            state(E32_M8, 1),
        ));
    }

    // Widening destinations may overlap a narrow source only in their
    // highest-numbered part, and only when source EMUL is at least one.
    // Exercise integer, FP, conversion, and multiply-accumulate families.
    for (name, funct6, funct3, src) in [
        ("vwadd.vv", 0b110000, 0b010, 2),
        ("vwadd.vx", 0b110000, 0b110, 5),
        ("vwmulu.vv", 0b111000, 0b010, 2),
        ("vfwadd.vv", 0b110000, 0b001, 2),
        ("vfwadd.vf", 0b110000, 0b101, 5),
        ("vfwmul.vv", 0b111000, 0b001, 2),
        ("vfwcvt.xu.f.v", 0b010010, 0b001, 0b01000),
    ] {
        batch.push((
            format!("{name}.low-overlap"),
            op_iv(funct6, 1, 0, src, funct3, 0),
            state(E32_M1, 2),
        ));
        batch.push((
            format!("{name}.high-overlap-control"),
            op_iv(funct6, 1, 1, src, funct3, 0),
            state(E32_M1, 2),
        ));
    }

    // Widening MAC/FMA instructions read the wide destination as an addend.
    // A narrow source cannot overlap it at either the low or high part because
    // that would read one register at two EEWs in the same instruction.
    for (name, funct6, funct3, src) in [
        ("vwmacc.vv", 0b111101, 0b010, 3),
        ("vwmaccus.vx", 0b111110, 0b110, 5),
        ("vfwmacc.vv", 0b111100, 0b001, 3),
    ] {
        batch.push((
            format!("{name}.low-overlap"),
            op_iv(funct6, 1, 0, src, funct3, 0),
            state(E32_M1, 2),
        ));
        batch.push((
            format!("{name}.high-overlap"),
            op_iv(funct6, 1, 1, src, funct3, 0),
            state(E32_M1, 2),
        ));
        batch.push((
            format!("{name}.disjoint-control"),
            op_iv(funct6, 1, 2, src, funct3, 0),
            state(E32_M1, 2),
        ));
    }

    // .w forms read a same-width wide vs2, which may alias vd. A narrow vector
    // vs1 must not overlap wide vs2, and otherwise follows the high-part rule
    // when it overlaps vd.
    for (name, funct6, funct3) in [
        ("vwadd.wv", 0b110100, 0b010),
        ("vfwadd.wv", 0b110100, 0b001),
    ] {
        batch.push((
            format!("{name}.wide-alias-control"),
            op_iv(funct6, 1, 0, 2, funct3, 0),
            state(E32_M1, 2),
        ));
        batch.push((
            format!("{name}.narrow-low-overlap"),
            op_iv(funct6, 1, 0, 0, funct3, 0),
            state(E32_M1, 2),
        ));
        batch.push((
            format!("{name}.narrow-high-source-conflict"),
            op_iv(funct6, 1, 0, 1, funct3, 0),
            state(E32_M1, 2),
        ));
        batch.push((
            format!("{name}.destination-high-overlap-control"),
            op_iv(funct6, 1, 4, 1, funct3, 0),
            state(E32_M1, 2),
        ));
    }

    // Integer LMUL frontiers: m2/m4 retain the legal high-part case, m8
    // would require a reserved EMUL=16 destination, and fractional narrow
    // sources cannot overlap the destination at all.
    for (vtype, high_source, label) in [(E32_M2, 2, "m2"), (E32_M4, 4, "m4")] {
        batch.push((
            format!("vwadd.vv.{label}.high-overlap-control"),
            op_iv(0b110000, 1, high_source, 8, 0b010, 0),
            state(vtype, 2),
        ));
        batch.push((
            format!("vwadd.vv.{label}.low-overlap"),
            op_iv(0b110000, 1, 0, 8, 0b010, 0),
            state(vtype, 2),
        ));
    }
    batch.push((
        "vwadd.vv.m8.emul-16".into(),
        op_iv(0b110000, 1, 8, 16, 0b010, 0),
        state(E32_M8, 1),
    ));
    for (vtype, label) in [(E16_MF8, "mf8"), (E32_MF4, "mf4"), (E32_MF2, "mf2")] {
        batch.push((
            format!("vwadd.vv.{label}.same-register-overlap"),
            op_iv(0b110000, 1, 1, 2, 0b010, 1),
            state(vtype, 1),
        ));
        batch.push((
            format!("vwadd.vv.{label}.disjoint-control"),
            op_iv(0b110000, 1, 2, 3, 0b010, 1),
            state(vtype, 1),
        ));
    }

    // Integer extension source EMUL is LMUL divided by vf2/vf4/vf8.
    for (name, selector, vtype, high_source) in [
        ("vzext.vf2", 0b00110, E32_M2, 1),
        ("vzext.vf4", 0b00100, E32_M4, 3),
        ("vzext.vf8", 0b00010, E64_M8, 7),
    ] {
        batch.push((
            format!("{name}.high-overlap-control"),
            op_iv(0b010010, 1, high_source, selector, 0b010, 0),
            state(vtype, 1),
        ));
        batch.push((
            format!("{name}.low-overlap"),
            op_iv(0b010010, 1, 0, selector, 0b010, 0),
            state(vtype, 1),
        ));
    }
    // Widening reductions use scalar EMUL=1 for vd/vs1 but current LMUL for
    // their narrow vs2 group.
    for (name, funct6, funct3) in [
        ("vwredsumu.vs", 0b110000, 0b000),
        ("vfwredusum.vs", 0b110001, 0b001),
    ] {
        batch.push((
            format!("{name}.high-overlap-control"),
            op_iv(funct6, 1, 0, 3, funct3, 1),
            state(E32_M2, 2),
        ));
        batch.push((
            format!("{name}.low-overlap-control"),
            op_iv(funct6, 1, 0, 3, funct3, 0),
            state(E32_M2, 2),
        ));
        batch.push((
            format!("{name}.misaligned-vs2"),
            op_iv(funct6, 1, 1, 3, funct3, 4),
            state(E32_M2, 2),
        ));
    }

    // vmv.v.v/vx/vi reserve vs2 and require it to encode v0.
    for (name, funct3, src) in [
        ("vmv.v.v", 0b000, 4),
        ("vmv.v.x", 0b100, 5),
        ("vmv.v.i", 0b011, 4),
    ] {
        batch.push((
            format!("{name}.reserved-vs2"),
            op_iv(0b010111, 1, 7, src, funct3, 2),
            state(E8_M1, 4),
        ));
        batch.push((
            format!("{name}.vs2-v0-control"),
            op_iv(0b010111, 1, 0, src, funct3, 2),
            state(E8_M1, 4),
        ));
    }

    // Same-width ALU operands name complete LMUL-sized register groups.
    for (name, vd, vs2, vs1) in [
        ("vadd.vv.misaligned-vd", 1, 2, 4),
        ("vadd.vv.misaligned-vs2", 0, 3, 4),
        ("vadd.vv.misaligned-vs1", 0, 2, 5),
        ("vadd.vv.aligned-control", 0, 2, 4),
    ] {
        batch.push((
            name.into(),
            op_iv(0b000000, 1, vs2, vs1, 0b000, vd),
            state(E32_M2, 2),
        ));
    }

    // Every integer/FP, single-width/widening reduction is non-restartable.
    for (name, funct6, funct3) in [
        ("vredsum.vs", 0b000000, 0b010),
        ("vfredusum.vs", 0b000001, 0b001),
        ("vwredsumu.vs", 0b110000, 0b000),
        ("vfwredusum.vs", 0b110001, 0b001),
    ] {
        let mut nonrestartable = state(E32_M2, 2);
        nonrestartable.vstart = 1;
        batch.push((
            format!("{name}.nonzero-vstart"),
            op_iv(funct6, 1, 2, 3, funct3, 1),
            nonrestartable,
        ));
        batch.push((
            format!("{name}.vstart-zero-control"),
            op_iv(funct6, 1, 2, 3, funct3, 1),
            state(E32_M2, 2),
        ));
    }

    // All vsetvl forms reset vstart on successful completion.
    let mut configured = state(E8_M1, 4);
    configured.vstart = 7;
    batch.push((
        "vsetvli.resets-vstart".into(),
        (7 << 12) | (1 << 7) | 0x57,
        configured,
    ));

    // FP operands with EEW=8 are unsupported. Zvfh still defines the
    // integer-to-FP widening and FP-to-integer narrowing directions at SEW=8
    // because their FP operand is 16 bits; integer instructions remain
    // unaffected.
    for (name, instruction) in [
        ("vfadd.vv.e8", op_iv(0b000000, 1, 2, 3, 0b001, 1)),
        ("vfslide1up.vf.e8", op_iv(0b001110, 1, 2, 3, 0b101, 1)),
        ("vfwadd.vv.e8", op_iv(0b110000, 1, 2, 3, 0b001, 0)),
        ("vfwcvt.xu.f.v.e8", op_iv(0b010010, 1, 2, 0b01000, 0b001, 0)),
        ("vfncvt.f.xu.w.e8", op_iv(0b010010, 1, 2, 0b10010, 0b001, 1)),
        (
            "vfwcvt.f.xu.v.e8.control",
            op_iv(0b010010, 1, 2, 0b01010, 0b001, 0),
        ),
        (
            "vfncvt.xu.f.w.e8.control",
            op_iv(0b010010, 1, 2, 0b10000, 0b001, 1),
        ),
        (
            "vfncvt.x.f.w.e8.control",
            op_iv(0b010010, 1, 2, 0b10001, 0b001, 1),
        ),
        (
            "vfncvt.rtz.xu.f.w.e8.control",
            op_iv(0b010010, 1, 2, 0b10110, 0b001, 1),
        ),
        (
            "vfncvt.rtz.x.f.w.e8.control",
            op_iv(0b010010, 1, 2, 0b10111, 0b001, 1),
        ),
        ("vadd.vv.e8.control", op_iv(0b000000, 1, 2, 3, 0b000, 1)),
    ] {
        batch.push((name.into(), instruction, state(E8_M1, 2)));
    }

    // Every OPFVV/OPFVF instruction must reject reserved frm=5/6/7 before
    // either vl=0 or vstart>=vl could suppress its element loop.
    let fp_representatives = [
        ("vfadd.vv", op_iv(0b000000, 1, 2, 3, 0b001, 1)),
        ("vfmin.vv", op_iv(0b000100, 1, 2, 3, 0b001, 1)),
        ("vfsgnj.vv", op_iv(0b001000, 1, 2, 3, 0b001, 1)),
        ("vmfeq.vv", op_iv(0b011000, 1, 2, 3, 0b001, 1)),
        ("vfclass.v", op_iv(0b010011, 1, 2, 0b10000, 0b001, 1)),
        ("vfmv.f.s", op_iv(0b010000, 1, 2, 0, 0b001, 1)),
        ("vfmv.s.f", op_iv(0b010000, 1, 0, 3, 0b101, 1)),
        ("vfrsqrt7.v", op_iv(0b010011, 1, 2, 0b00100, 0b001, 1)),
        ("vfrec7.v", op_iv(0b010011, 1, 2, 0b00101, 0b001, 1)),
        ("vfcvt.rtz.xu.f.v", op_iv(0b010010, 1, 2, 0b00110, 0b001, 1)),
        ("vfslide1up.vf", op_iv(0b001110, 1, 2, 3, 0b101, 1)),
        ("vfslide1down.vf", op_iv(0b001111, 1, 2, 3, 0b101, 1)),
        ("vfredusum.vs", op_iv(0b000001, 1, 2, 3, 0b001, 1)),
        ("vfwadd.vv", op_iv(0b110000, 1, 2, 3, 0b001, 1)),
    ];
    for (name, insn) in fp_representatives {
        for frm in 5..=7u64 {
            let mut vl_zero = state(E32_M1, 0);
            vl_zero.fcsr = frm << 5;
            batch.push((format!("{name}.frm-{frm}.vl-zero"), insn, vl_zero));

            let mut completed = state(E32_M1, 4);
            completed.fcsr = frm << 5;
            completed.vstart = 4;
            batch.push((format!("{name}.frm-{frm}.vstart-at-vl"), insn, completed));
        }
    }

    run_batch(&batch);
}
