//! RISC-V disassembler: renders a decoded [`Insn`] as GNU-style assembly with
//! ABI register names. Used for tracing and trap diagnostics.

use std::fmt;

use super::decode::{Insn, Op};
use super::{f_name, x_name};

impl Op {
    /// The assembler mnemonic for this operation.
    pub fn mnemonic(self) -> &'static str {
        use Op::*;
        match self {
            Lui => "lui",
            Auipc => "auipc",
            Jal => "jal",
            Jalr => "jalr",
            Beq => "beq",
            Bne => "bne",
            Blt => "blt",
            Bge => "bge",
            Bltu => "bltu",
            Bgeu => "bgeu",
            Lb => "lb",
            Lh => "lh",
            Lw => "lw",
            Lbu => "lbu",
            Lhu => "lhu",
            Lwu => "lwu",
            Ld => "ld",
            LdPair => "ld",
            Sb => "sb",
            Sh => "sh",
            Sw => "sw",
            Sd => "sd",
            SdPair => "sd",
            Addi => "addi",
            Slti => "slti",
            Sltiu => "sltiu",
            Xori => "xori",
            Ori => "ori",
            Andi => "andi",
            Slli => "slli",
            Srli => "srli",
            Srai => "srai",
            Add => "add",
            Sub => "sub",
            Sll => "sll",
            Slt => "slt",
            Sltu => "sltu",
            Xor => "xor",
            Srl => "srl",
            Sra => "sra",
            Or => "or",
            And => "and",
            Addiw => "addiw",
            Slliw => "slliw",
            Srliw => "srliw",
            Sraiw => "sraiw",
            Addw => "addw",
            Subw => "subw",
            Sllw => "sllw",
            Sltw => "sltw",
            Srlw => "srlw",
            Sraw => "sraw",
            Fence => "fence",
            FenceI => "fence.i",
            Pause => "pause",
            NtlP1 => "ntl.p1",
            NtlPall => "ntl.pall",
            NtlS1 => "ntl.s1",
            NtlAll => "ntl.all",
            CboInval => "cbo.inval",
            CboClean => "cbo.clean",
            CboFlush => "cbo.flush",
            CboZero => "cbo.zero",
            PrefetchI => "prefetch.i",
            PrefetchR => "prefetch.r",
            PrefetchW => "prefetch.w",
            Ecall => "ecall",
            Ebreak => "ebreak",
            CmPush => "cm.push",
            CmPop => "cm.pop",
            CmPopRetz => "cm.popretz",
            CmPopRet => "cm.popret",
            CmMvsa01 => "cm.mvsa01",
            CmMva01s => "cm.mva01s",
            CmJt => "cm.jt",
            CmJalt => "cm.jalt",
            Mret => "mret",
            Sret => "sret",
            Wfi => "wfi",
            WrsNto => "wrs.nto",
            WrsSto => "wrs.sto",
            Uret => "uret",
            SfenceVm => "sfence.vm",
            SfenceVma => "sfence.vma",
            SinvalVma => "sinval.vma",
            SfenceWInval => "sfence.w.inval",
            SfenceInvalIr => "sfence.inval.ir",
            HfenceVvma => "hfence.vvma",
            HfenceGvma => "hfence.gvma",
            HinvalVvma => "hinval.vvma",
            HinvalGvma => "hinval.gvma",
            HlvB => "hlv.b",
            HlvH => "hlv.h",
            HlvW => "hlv.w",
            HlvD => "hlv.d",
            HlvBu => "hlv.bu",
            HlvHu => "hlv.hu",
            HlvWu => "hlv.wu",
            HlvxHu => "hlvx.hu",
            HlvxWu => "hlvx.wu",
            HsvB => "hsv.b",
            HsvH => "hsv.h",
            HsvW => "hsv.w",
            HsvD => "hsv.d",
            Csrrw => "csrrw",
            Csrrs => "csrrs",
            Csrrc => "csrrc",
            Csrrwi => "csrrwi",
            Csrrsi => "csrrsi",
            Csrrci => "csrrci",
            Mul => "mul",
            Mulh => "mulh",
            Mulhsu => "mulhsu",
            Mulhu => "mulhu",
            Div => "div",
            Divu => "divu",
            Rem => "rem",
            Remu => "remu",
            Mulw => "mulw",
            Divw => "divw",
            Divuw => "divuw",
            Remw => "remw",
            Remuw => "remuw",
            LrW => "lr.w",
            ScW => "sc.w",
            AmoswapW => "amoswap.w",
            AmoaddW => "amoadd.w",
            AmoxorW => "amoxor.w",
            AmoandW => "amoand.w",
            AmoorW => "amoor.w",
            AmominW => "amomin.w",
            AmomaxW => "amomax.w",
            AmominuW => "amominu.w",
            AmomaxuW => "amomaxu.w",
            AmocasW => "amocas.w",
            LrD => "lr.d",
            ScD => "sc.d",
            AmoswapD => "amoswap.d",
            AmoaddD => "amoadd.d",
            AmoxorD => "amoxor.d",
            AmoandD => "amoand.d",
            AmoorD => "amoor.d",
            AmominD => "amomin.d",
            AmomaxD => "amomax.d",
            AmominuD => "amominu.d",
            AmomaxuD => "amomaxu.d",
            AmocasD => "amocas.d",
            AmocasQ => "amocas.q",
            Flw => "flw",
            Fsw => "fsw",
            FmaddS => "fmadd.s",
            FmsubS => "fmsub.s",
            FnmsubS => "fnmsub.s",
            FnmaddS => "fnmadd.s",
            FaddS => "fadd.s",
            FsubS => "fsub.s",
            FmulS => "fmul.s",
            FdivS => "fdiv.s",
            FsqrtS => "fsqrt.s",
            FsgnjS => "fsgnj.s",
            FsgnjnS => "fsgnjn.s",
            FsgnjxS => "fsgnjx.s",
            FminS => "fmin.s",
            FmaxS => "fmax.s",
            FcvtWS => "fcvt.w.s",
            FcvtWuS => "fcvt.wu.s",
            FcvtLS => "fcvt.l.s",
            FcvtLuS => "fcvt.lu.s",
            FmvXW => "fmv.x.w",
            FeqS => "feq.s",
            FltS => "flt.s",
            FleS => "fle.s",
            FclassS => "fclass.s",
            FcvtSW => "fcvt.s.w",
            FcvtSWu => "fcvt.s.wu",
            FcvtSL => "fcvt.s.l",
            FcvtSLu => "fcvt.s.lu",
            FmvWX => "fmv.w.x",
            Fld => "fld",
            Fsd => "fsd",
            FmaddD => "fmadd.d",
            FmsubD => "fmsub.d",
            FnmsubD => "fnmsub.d",
            FnmaddD => "fnmadd.d",
            FaddD => "fadd.d",
            FsubD => "fsub.d",
            FmulD => "fmul.d",
            FdivD => "fdiv.d",
            FsqrtD => "fsqrt.d",
            FsgnjD => "fsgnj.d",
            FsgnjnD => "fsgnjn.d",
            FsgnjxD => "fsgnjx.d",
            FminD => "fmin.d",
            FmaxD => "fmax.d",
            FcvtSD => "fcvt.s.d",
            FcvtDS => "fcvt.d.s",
            FeqD => "feq.d",
            FltD => "flt.d",
            FleD => "fle.d",
            FclassD => "fclass.d",
            FcvtWD => "fcvt.w.d",
            FcvtWuD => "fcvt.wu.d",
            FcvtLD => "fcvt.l.d",
            FcvtLuD => "fcvt.lu.d",
            FcvtDW => "fcvt.d.w",
            FcvtDWu => "fcvt.d.wu",
            FcvtDL => "fcvt.d.l",
            FcvtDLu => "fcvt.d.lu",
            FmvXD => "fmv.x.d",
            FmvDX => "fmv.d.x",
            Flq => "flq",
            Fsq => "fsq",
            FmaddQ => "fmadd.q",
            FmsubQ => "fmsub.q",
            FnmsubQ => "fnmsub.q",
            FnmaddQ => "fnmadd.q",
            FaddQ => "fadd.q",
            FsubQ => "fsub.q",
            FmulQ => "fmul.q",
            FdivQ => "fdiv.q",
            FsqrtQ => "fsqrt.q",
            FsgnjQ => "fsgnj.q",
            FsgnjnQ => "fsgnjn.q",
            FsgnjxQ => "fsgnjx.q",
            FminQ => "fmin.q",
            FmaxQ => "fmax.q",
            FcvtSQ => "fcvt.s.q",
            FcvtQS => "fcvt.q.s",
            FcvtDQ => "fcvt.d.q",
            FcvtQD => "fcvt.q.d",
            FcvtHQ => "fcvt.h.q",
            FcvtQH => "fcvt.q.h",
            FeqQ => "feq.q",
            FltQ => "flt.q",
            FleQ => "fle.q",
            FclassQ => "fclass.q",
            FcvtWQ => "fcvt.w.q",
            FcvtWuQ => "fcvt.wu.q",
            FcvtLQ => "fcvt.l.q",
            FcvtLuQ => "fcvt.lu.q",
            FcvtQW => "fcvt.q.w",
            FcvtQWu => "fcvt.q.wu",
            FcvtQL => "fcvt.q.l",
            FcvtQLu => "fcvt.q.lu",
            Sh1add => "sh1add",
            Sh2add => "sh2add",
            Sh3add => "sh3add",
            AddUw => "add.uw",
            Sh1addUw => "sh1add.uw",
            Sh2addUw => "sh2add.uw",
            Sh3addUw => "sh3add.uw",
            SlliUw => "slli.uw",
            Andn => "andn",
            Orn => "orn",
            Xnor => "xnor",
            Clz => "clz",
            Ctz => "ctz",
            Cpop => "cpop",
            Max => "max",
            Maxu => "maxu",
            Min => "min",
            Minu => "minu",
            SextB => "sext.b",
            SextH => "sext.h",
            ZextH => "zext.h",
            Rol => "rol",
            Ror => "ror",
            Rori => "rori",
            Orcb => "orc.b",
            Rev8 => "rev8",
            Clzw => "clzw",
            Ctzw => "ctzw",
            Cpopw => "cpopw",
            Rolw => "rolw",
            Rorw => "rorw",
            Roriw => "roriw",
            Clmul => "clmul",
            Clmulh => "clmulh",
            Clmulr => "clmulr",
            Bclr => "bclr",
            Bclri => "bclri",
            Bext => "bext",
            Bexti => "bexti",
            Binv => "binv",
            Binvi => "binvi",
            Bset => "bset",
            Bseti => "bseti",
            CzeroEqz => "czero.eqz",
            CzeroNez => "czero.nez",
            FliS => "fli.s",
            FliD => "fli.d",
            FminmS => "fminm.s",
            FmaxmS => "fmaxm.s",
            FminmD => "fminm.d",
            FmaxmD => "fmaxm.d",
            FroundS => "fround.s",
            FroundnxS => "froundnx.s",
            FroundD => "fround.d",
            FroundnxD => "froundnx.d",
            FleqS => "fleq.s",
            FltqS => "fltq.s",
            FleqD => "fleq.d",
            FltqD => "fltq.d",
            FcvtmodWD => "fcvtmod.w.d",
            Pack => "pack",
            Packh => "packh",
            Packw => "packw",
            Brev8 => "brev8",
            Zip => "zip",
            Unzip => "unzip",
            Flh => "flh",
            Fsh => "fsh",
            FaddH => "fadd.h",
            FsubH => "fsub.h",
            FmulH => "fmul.h",
            FdivH => "fdiv.h",
            FsqrtH => "fsqrt.h",
            FmaddH => "fmadd.h",
            FmsubH => "fmsub.h",
            FnmsubH => "fnmsub.h",
            FnmaddH => "fnmadd.h",
            FsgnjH => "fsgnj.h",
            FsgnjnH => "fsgnjn.h",
            FsgnjxH => "fsgnjx.h",
            FminH => "fmin.h",
            FmaxH => "fmax.h",
            FeqH => "feq.h",
            FltH => "flt.h",
            FleH => "fle.h",
            FclassH => "fclass.h",
            FcvtSH => "fcvt.s.h",
            FcvtHS => "fcvt.h.s",
            FcvtDH => "fcvt.d.h",
            FcvtHD => "fcvt.h.d",
            FcvtWH => "fcvt.w.h",
            FcvtWuH => "fcvt.wu.h",
            FcvtLH => "fcvt.l.h",
            FcvtLuH => "fcvt.lu.h",
            FcvtHW => "fcvt.h.w",
            FcvtHWu => "fcvt.h.wu",
            FcvtHL => "fcvt.h.l",
            FcvtHLu => "fcvt.h.lu",
            FmvXH => "fmv.x.h",
            FmvHX => "fmv.h.x",
            FliH => "fli.h",
            FminmH => "fminm.h",
            FmaxmH => "fmaxm.h",
            FroundH => "fround.h",
            FroundnxH => "froundnx.h",
            FleqH => "fleq.h",
            FltqH => "fltq.h",
            Xperm4 => "xperm4",
            Xperm8 => "xperm8",
            Sha256Sig0 => "sha256sig0",
            Sha256Sig1 => "sha256sig1",
            Sha256Sum0 => "sha256sum0",
            Sha256Sum1 => "sha256sum1",
            Sha512Sig0 => "sha512sig0",
            Sha512Sig1 => "sha512sig1",
            Sha512Sum0 => "sha512sum0",
            Sha512Sum1 => "sha512sum1",
            Sha512Sig0l => "sha512sig0l",
            Sha512Sig0h => "sha512sig0h",
            Sha512Sig1l => "sha512sig1l",
            Sha512Sig1h => "sha512sig1h",
            Sha512Sum0r => "sha512sum0r",
            Sha512Sum1r => "sha512sum1r",
            Sm3p0 => "sm3p0",
            Sm3p1 => "sm3p1",
            Sm4ed => "sm4ed",
            Sm4ks => "sm4ks",
            Aes32esi => "aes32esi",
            Aes32esmi => "aes32esmi",
            Aes32dsi => "aes32dsi",
            Aes32dsmi => "aes32dsmi",
            Aes64es => "aes64es",
            Aes64esm => "aes64esm",
            Aes64ds => "aes64ds",
            Aes64dsm => "aes64dsm",
            Aes64ks1i => "aes64ks1i",
            Aes64ks2 => "aes64ks2",
            Aes64im => "aes64im",
            Vsetvli => "vsetvli",
            Vsetivli => "vsetivli",
            Vsetvl => "vsetvl",
            Vle => "vle.v",
            Vse => "vse.v",
            Vadd => "vadd",
            Vsub => "vsub",
            Vrsub => "vrsub",
            Vand => "vand",
            Vor => "vor",
            Vxor => "vxor",
            Vminu => "vminu",
            Vmin => "vmin",
            Vmaxu => "vmaxu",
            Vmax => "vmax",
            Vsll => "vsll",
            Vsrl => "vsrl",
            Vsra => "vsra",
            Vmerge => "vmerge",
            Vmseq => "vmseq",
            Vmsne => "vmsne",
            Vmsltu => "vmsltu",
            Vmslt => "vmslt",
            Vmsleu => "vmsleu",
            Vmsle => "vmsle",
            Vmsgtu => "vmsgtu",
            Vmsgt => "vmsgt",
            Vmul => "vmul",
            Vmulh => "vmulh",
            Vmulhu => "vmulhu",
            Vmulhsu => "vmulhsu",
            Vdivu => "vdivu",
            Vdiv => "vdiv",
            Vremu => "vremu",
            Vrem => "vrem",
            Vlse => "vlse",
            Vsse => "vsse",
            Vlxei => "vlxei",
            Vsxei => "vsxei",
            Vlm => "vlm.v",
            Vsm => "vsm.v",
            Vlre => "vlre.v",
            Vsre => "vsre.v",
            Vlseg => "vlseg",
            Vsseg => "vsseg",
            Vleff => "vleff",
            Vfadd => "vfadd",
            Vfsub => "vfsub",
            Vfrsub => "vfrsub",
            Vfmul => "vfmul",
            Vfdiv => "vfdiv",
            Vfrdiv => "vfrdiv",
            Vfsqrt => "vfsqrt",
            Vfmin => "vfmin",
            Vfmax => "vfmax",
            Vfsgnj => "vfsgnj",
            Vfsgnjn => "vfsgnjn",
            Vfsgnjx => "vfsgnjx",
            Vmfeq => "vmfeq",
            Vmfne => "vmfne",
            Vmflt => "vmflt",
            Vmfle => "vmfle",
            Vmfgt => "vmfgt",
            Vmfge => "vmfge",
            Vfmacc => "vfmacc",
            Vfnmacc => "vfnmacc",
            Vfmsac => "vfmsac",
            Vfnmsac => "vfnmsac",
            Vfmadd => "vfmadd",
            Vfnmadd => "vfnmadd",
            Vfmsub => "vfmsub",
            Vfnmsub => "vfnmsub",
            Vredsum => "vredsum",
            Vredand => "vredand",
            Vredor => "vredor",
            Vredxor => "vredxor",
            Vredminu => "vredminu",
            Vredmin => "vredmin",
            Vredmaxu => "vredmaxu",
            Vredmax => "vredmax",
            Vfredusum => "vfredusum",
            Vfredosum => "vfredosum",
            Vfredmin => "vfredmin",
            Vfredmax => "vfredmax",
            VmvXS => "vmv.x.s",
            VmvSX => "vmv.s.x",
            VfmvFS => "vfmv.f.s",
            VfmvSF => "vfmv.s.f",
            Vmand => "vmand",
            Vmnand => "vmnand",
            Vmandn => "vmandn",
            Vmxor => "vmxor",
            Vmor => "vmor",
            Vmnor => "vmnor",
            Vmorn => "vmorn",
            Vmxnor => "vmxnor",
            VzextVf2 => "vzext.vf2",
            VsextVf2 => "vsext.vf2",
            VzextVf4 => "vzext.vf4",
            VsextVf4 => "vsext.vf4",
            VzextVf8 => "vzext.vf8",
            VsextVf8 => "vsext.vf8",
            Vcpop => "vcpop.m",
            Vfirst => "vfirst.m",
            Vmsbf => "vmsbf.m",
            Vmsof => "vmsof.m",
            Vmsif => "vmsif.m",
            Viota => "viota.m",
            Vid => "vid.v",
            Vslideup => "vslideup",
            Vslidedown => "vslidedown",
            Vslide1up => "vslide1up",
            Vslide1down => "vslide1down",
            Vfslide1up => "vfslide1up",
            Vfslide1down => "vfslide1down",
            Vrgather => "vrgather",
            Vrgatherei16 => "vrgatherei16",
            Vcompress => "vcompress",
            Vadc => "vadc",
            Vmadc => "vmadc",
            Vsbc => "vsbc",
            Vmsbc => "vmsbc",
            Vsaddu => "vsaddu",
            Vsadd => "vsadd",
            Vssubu => "vssubu",
            Vssub => "vssub",
            Vaaddu => "vaaddu",
            Vaadd => "vaadd",
            Vasubu => "vasubu",
            Vasub => "vasub",
            Vssrl => "vssrl",
            Vssra => "vssra",
            Vsmul => "vsmul",
            Vwaddu => "vwaddu",
            Vwadd => "vwadd",
            Vwsubu => "vwsubu",
            Vwsub => "vwsub",
            VwadduW => "vwaddu.w",
            VwaddW => "vwadd.w",
            VwsubuW => "vwsubu.w",
            VwsubW => "vwsub.w",
            Vwmulu => "vwmulu",
            Vwmulsu => "vwmulsu",
            Vwmul => "vwmul",
            Vwmaccu => "vwmaccu",
            Vwmacc => "vwmacc",
            Vwmaccsu => "vwmaccsu",
            Vwmaccus => "vwmaccus",
            Vnsrl => "vnsrl",
            Vnsra => "vnsra",
            Vnclipu => "vnclipu",
            Vnclip => "vnclip",
            VfcvtXuF => "vfcvt.xu.f.v",
            VfcvtXF => "vfcvt.x.f.v",
            VfcvtFXu => "vfcvt.f.xu.v",
            VfcvtFX => "vfcvt.f.x.v",
            VfcvtRtzXuF => "vfcvt.rtz.xu.f.v",
            VfcvtRtzXF => "vfcvt.rtz.x.f.v",
            VfwcvtXuF => "vfwcvt.xu.f.v",
            VfwcvtXF => "vfwcvt.x.f.v",
            VfwcvtFXu => "vfwcvt.f.xu.v",
            VfwcvtFX => "vfwcvt.f.x.v",
            VfwcvtFF => "vfwcvt.f.f.v",
            VfwcvtRtzXuF => "vfwcvt.rtz.xu.f.v",
            VfwcvtRtzXF => "vfwcvt.rtz.x.f.v",
            VfncvtXuF => "vfncvt.xu.f.w",
            VfncvtXF => "vfncvt.x.f.w",
            VfncvtFXu => "vfncvt.f.xu.w",
            VfncvtFX => "vfncvt.f.x.w",
            VfncvtFF => "vfncvt.f.f.w",
            VfncvtRodFF => "vfncvt.rod.f.f.w",
            VfncvtRtzXuF => "vfncvt.rtz.xu.f.w",
            VfncvtRtzXF => "vfncvt.rtz.x.f.w",
            Vfwadd => "vfwadd",
            Vfwsub => "vfwsub",
            Vfwmul => "vfwmul",
            VfwaddW => "vfwadd.w",
            VfwsubW => "vfwsub.w",
            Vfwmacc => "vfwmacc",
            Vfwnmacc => "vfwnmacc",
            Vfwmsac => "vfwmsac",
            Vfwnmsac => "vfwnmsac",
            Vwredsumu => "vwredsumu",
            Vwredsum => "vwredsum",
            Vfwredusum => "vfwredusum",
            Vfwredosum => "vfwredosum",
            Vfclass => "vfclass.v",
            Vmvr => "vmvr.v",
            Vfrsqrt7 => "vfrsqrt7.v",
            Vfrec7 => "vfrec7.v",
            Grev => "grev",
            Grevi => "grevi",
            Bitc => "bitc",
            Bitci => "bitci",
            Bits => "bits",
            Bitsi => "bitsi",
            Fls => "fls",
            Pcnt => "pcnt",
            NdsLbgp => "nds.lbgp",
            NdsLbugp => "nds.lbugp",
            NdsLhgp => "nds.lhgp",
            NdsLhugp => "nds.lhugp",
            NdsLwgp => "nds.lwgp",
            NdsLwugp => "nds.lwugp",
            NdsLdgp => "nds.ldgp",
            NdsSbgp => "nds.sbgp",
            NdsShgp => "nds.shgp",
            NdsSwgp => "nds.swgp",
            NdsSdgp => "nds.sdgp",
            NdsAddigp => "nds.addigp",
            NdsBfoz => "nds.bfoz",
            NdsBfos => "nds.bfos",
            NdsBbc => "nds.bbc",
            NdsBbs => "nds.bbs",
            NdsBeqc => "nds.beqc",
            NdsBnec => "nds.bnec",
            NdsLeaH => "nds.lea.h",
            NdsLeaW => "nds.lea.w",
            NdsLeaD => "nds.lea.d",
            NdsLeaBZe => "nds.lea.b.ze",
            NdsLeaHZe => "nds.lea.h.ze",
            NdsLeaWZe => "nds.lea.w.ze",
            NdsLeaDZe => "nds.lea.d.ze",
            NdsFfb => "nds.ffb",
            NdsFfmism => "nds.ffmism",
            NdsFfzmism => "nds.ffzmism",
            NdsFlmism => "nds.flmism",
            ThDcacheCall => "th.dcache.call",
            ThDcacheCiall => "th.dcache.ciall",
            ThDcacheIall => "th.dcache.iall",
            ThDcacheCpa => "th.dcache.cpa",
            ThDcacheCipa => "th.dcache.cipa",
            ThDcacheIpa => "th.dcache.ipa",
            ThDcacheCva => "th.dcache.cva",
            ThDcacheCiva => "th.dcache.civa",
            ThDcacheIva => "th.dcache.iva",
            ThDcacheCsw => "th.dcache.csw",
            ThDcacheCisw => "th.dcache.cisw",
            ThDcacheIsw => "th.dcache.isw",
            ThDcacheCpal1 => "th.dcache.cpal1",
            ThDcacheCval1 => "th.dcache.cval1",
            ThIcacheIall => "th.icache.iall",
            ThIcacheIalls => "th.icache.ialls",
            ThIcacheIpa => "th.icache.ipa",
            ThIcacheIva => "th.icache.iva",
            ThL2cacheCall => "th.l2cache.call",
            ThL2cacheCiall => "th.l2cache.ciall",
            ThL2cacheIall => "th.l2cache.iall",
            ThSfenceVmas => "th.sfence.vmas",
            ThSync => "th.sync",
            ThSyncS => "th.sync.s",
            ThSyncI => "th.sync.i",
            ThSyncIS => "th.sync.is",
            ThIpush => "th.ipush",
            ThIpop => "th.ipop",
            ThAddsl => "th.addsl",
            ThSrri => "th.srri",
            ThSrriw => "th.srriw",
            ThExt => "th.ext",
            ThExtu => "th.extu",
            ThFf0 => "th.ff0",
            ThFf1 => "th.ff1",
            ThRev => "th.rev",
            ThRevw => "th.revw",
            ThTstNbz => "th.tstnbz",
            ThTst => "th.tst",
            ThMveqz => "th.mveqz",
            ThMvnez => "th.mvnez",
            ThMula => "th.mula",
            ThMulah => "th.mulah",
            ThMulaw => "th.mulaw",
            ThMuls => "th.muls",
            ThMulsh => "th.mulsh",
            ThMulsw => "th.mulsw",
            ThFmvHwX => "th.fmv.hw.x",
            ThFmvXHw => "th.fmv.x.hw",
            ThAndn => "th.andn",
            ThOrn => "th.orn",
            ThXorn => "th.xorn",
            ThPackl => "th.packl",
            ThPackh => "th.packh",
            ThPackhl => "th.packhl",
            ThLbia => "th.lbia",
            ThLbib => "th.lbib",
            ThLbuia => "th.lbuia",
            ThLbuib => "th.lbuib",
            ThLhia => "th.lhia",
            ThLhib => "th.lhib",
            ThLhuia => "th.lhuia",
            ThLhuib => "th.lhuib",
            ThLwia => "th.lwia",
            ThLwib => "th.lwib",
            ThLwuia => "th.lwuia",
            ThLwuib => "th.lwuib",
            ThLdia => "th.ldia",
            ThLdib => "th.ldib",
            ThSbia => "th.sbia",
            ThSbib => "th.sbib",
            ThShia => "th.shia",
            ThShib => "th.shib",
            ThSwia => "th.swia",
            ThSwib => "th.swib",
            ThSdia => "th.sdia",
            ThSdib => "th.sdib",
            ThLrb => "th.lrb",
            ThLrbu => "th.lrbu",
            ThLrh => "th.lrh",
            ThLrhu => "th.lrhu",
            ThLrw => "th.lrw",
            ThLrwu => "th.lrwu",
            ThLrd => "th.lrd",
            ThSrb => "th.srb",
            ThSrh => "th.srh",
            ThSrw => "th.srw",
            ThSrd => "th.srd",
            ThLurb => "th.lurb",
            ThLurbu => "th.lurbu",
            ThLurh => "th.lurh",
            ThLurhu => "th.lurhu",
            ThLurw => "th.lurw",
            ThLurwu => "th.lurwu",
            ThLurd => "th.lurd",
            ThSurb => "th.surb",
            ThSurh => "th.surh",
            ThSurw => "th.surw",
            ThSurd => "th.surd",
            ThLdd => "th.ldd",
            ThLwd => "th.lwd",
            ThLwud => "th.lwud",
            ThSdd => "th.sdd",
            ThSwd => "th.swd",
            ThFlrd => "th.flrd",
            ThFlrw => "th.flrw",
            ThFlurd => "th.flurd",
            ThFlurw => "th.flurw",
            ThFsrd => "th.fsrd",
            ThFsrw => "th.fsrw",
            ThFsurd => "th.fsurd",
            ThFsurw => "th.fsurw",
            ThVmaqa => "th.vmaqa",
            ThVmaqau => "th.vmaqau",
            ThVmaqasu => "th.vmaqasu",
            ThVmaqaus => "th.vmaqaus",
            ThVpmaqa => "th.vpmaqa",
            ThVpmaqau => "th.vpmaqau",
            ThVpmaqasu => "th.vpmaqasu",
            ThVpmaqaus => "th.vpmaqaus",
            ThVpnclip => "th.vpnclip",
            ThVpnclipu => "th.vpnclipu",
            ThVpwadd => "th.vpwadd",
            ThVpwaddu => "th.vpwaddu",
            H3Block => "h3.block",
            H3Unblock => "h3.unblock",
            H3Bextm => "h3.bextm",
            H3Bextmi => "h3.bextmi",
            Illegal => "illegal",
        }
    }

    /// Operand layout class for formatting.
    fn class(self) -> Class {
        use Op::*;
        match self {
            Lui | Auipc => Class::U,
            Jal => Class::J,
            Jalr => Class::Jalr,
            Beq | Bne | Blt | Bge | Bltu | Bgeu => Class::B,
            Lb | Lh | Lw | Lbu | Lhu | Lwu | Ld => Class::Load,
            LdPair => Class::LoadPair,
            Sb | Sh | Sw | Sd => Class::Store,
            SdPair => Class::StorePair,
            Flw | Fld | Flh | Flq => Class::FLoad,
            Fsw | Fsd | Fsh | Fsq => Class::FStore,
            Addi | Slti | Sltiu | Xori | Ori | Andi | Addiw => Class::IArith,
            Slli | Srli | Srai | Slliw | Srliw | Sraiw | SlliUw | Rori | Roriw | Bclri | Bexti
            | Binvi | Bseti | Grevi | Bitci | Bitsi => Class::Shift,
            Fence | FenceI | Pause | NtlP1 | NtlPall | NtlS1 | NtlAll | Ecall | Ebreak | Mret
            | Sret | Wfi | WrsNto | WrsSto | Uret | SfenceWInval | SfenceInvalIr | H3Block
            | H3Unblock => Class::Bare,
            SfenceVm => Class::PrivFenceVm,
            SfenceVma | HfenceVvma | HfenceGvma => Class::PrivFence,
            SinvalVma | HinvalVvma | HinvalGvma => Class::PrivFence2,
            HlvB | HlvH | HlvW | HlvD | HlvBu | HlvHu | HlvWu | HlvxHu | HlvxWu => Class::HLoad,
            HsvB | HsvH | HsvW | HsvD => Class::HStore,
            CmPush | CmPop | CmPopRetz | CmPopRet => Class::ZcmpStack,
            CmMvsa01 | CmMva01s => Class::ZcmpMove,
            CmJt | CmJalt => Class::Zcmt,
            NdsLbgp | NdsLbugp | NdsLhgp | NdsLhugp | NdsLwgp | NdsLwugp | NdsLdgp => {
                Class::AndesGpLoad
            }
            NdsSbgp | NdsShgp | NdsSwgp | NdsSdgp => Class::AndesGpStore,
            NdsAddigp => Class::AndesAddigp,
            NdsBfoz | NdsBfos => Class::AndesBfo,
            NdsBbc | NdsBbs | NdsBeqc | NdsBnec => Class::AndesBranch,
            op if thead_bare(op) => Class::Bare,
            op if thead_addr(op) => Class::TheadAddr,
            ThSfenceVmas => Class::TheadFence,
            ThAddsl => Class::TheadAddsl,
            ThSrri | ThSrriw | ThTst => Class::Shift,
            ThExt | ThExtu => Class::TheadExt,
            ThFf0 | ThFf1 | ThRev | ThRevw | ThTstNbz => Class::Unary,
            ThFmvHwX => Class::TheadFmvHwX,
            ThFmvXHw => Class::TheadFmvXHw,
            op if thead_auto_mem(op) => Class::TheadAutoMem,
            op if thead_reg_mem(op) => Class::TheadRegMem,
            op if thead_pair_mem(op) => Class::TheadPairMem,
            op if thead_fmem(op) => Class::TheadFMem,
            op if thead_vec(op) => Class::TheadVec,
            CboInval | CboClean | CboFlush | CboZero => Class::Cbo,
            PrefetchI | PrefetchR | PrefetchW => Class::Prefetch,
            Csrrw | Csrrs | Csrrc => Class::Csr,
            Csrrwi | Csrrsi | Csrrci => Class::Csri,
            LrW | LrD => Class::Lr,
            ScW | ScD | AmoswapW | AmoaddW | AmoxorW | AmoandW | AmoorW | AmominW | AmomaxW
            | AmominuW | AmomaxuW | AmoswapD | AmoaddD | AmoxorD | AmoandD | AmoorD | AmominD
            | AmomaxD | AmominuD | AmomaxuD | AmocasW | AmocasD | AmocasQ => Class::Amo,
            Sm4ed | Sm4ks | Aes32esi | Aes32esmi | Aes32dsi | Aes32dsmi => Class::CryptoBs,
            Clz | Ctz | Cpop | SextB | SextH | ZextH | Clzw | Ctzw | Cpopw | Brev8 | Zip
            | Unzip | Sha256Sig0 | Sha256Sig1 | Sha256Sum0 | Sha256Sum1 | Sha512Sig0
            | Sha512Sig1 | Sha512Sum0 | Sha512Sum1 | Sm3p0 | Sm3p1 | Aes64im | Aes64ks1i | Fls
            | Pcnt => Class::Unary,
            FsqrtS | FsqrtD | FsqrtQ | FroundS | FroundnxS | FroundD | FroundnxD | FsqrtH
            | FroundH | FroundnxH => Class::FUnary,
            FaddS | FsubS | FmulS | FdivS | FsgnjS | FsgnjnS | FsgnjxS | FminS | FmaxS | FaddD
            | FsubD | FmulD | FdivD | FsgnjD | FsgnjnD | FsgnjxD | FminD | FmaxD | FminmS
            | FmaxmS | FminmD | FmaxmD | FaddH | FsubH | FmulH | FdivH | FsgnjH | FsgnjnH
            | FsgnjxH | FminH | FmaxH | FminmH | FmaxmH | FaddQ | FsubQ | FmulQ | FdivQ
            | FsgnjQ | FsgnjnQ | FsgnjxQ | FminQ | FmaxQ => Class::FBin,
            FmaddS | FmsubS | FnmsubS | FnmaddS | FmaddD | FmsubD | FnmsubD | FnmaddD | FmaddH
            | FmsubH | FnmsubH | FnmaddH | FmaddQ | FmsubQ | FnmsubQ | FnmaddQ => Class::FMA,
            FeqS | FltS | FleS | FeqD | FltD | FleD | FleqS | FltqS | FleqD | FltqD | FeqH
            | FltH | FleH | FleqH | FltqH | FeqQ | FltQ | FleQ => Class::FCmp,
            FliS | FliD | FliH => Class::Fli,
            FcvtWS | FcvtWuS | FcvtLS | FcvtLuS | FmvXW | FclassS | FcvtWD | FcvtWuD | FcvtLD
            | FcvtLuD | FmvXD | FclassD | FcvtmodWD | FcvtWH | FcvtWuH | FcvtLH | FcvtLuH
            | FmvXH | FclassH | FcvtWQ | FcvtWuQ | FcvtLQ | FcvtLuQ | FclassQ => Class::FToX,
            FcvtSW | FcvtSWu | FcvtSL | FcvtSLu | FmvWX | FcvtDW | FcvtDWu | FcvtDL | FcvtDLu
            | FmvDX | FcvtHW | FcvtHWu | FcvtHL | FcvtHLu | FmvHX | FcvtQW | FcvtQWu | FcvtQL
            | FcvtQLu => Class::XToF,
            FcvtSD | FcvtDS | FcvtSH | FcvtHS | FcvtDH | FcvtHD | FcvtSQ | FcvtQS | FcvtDQ
            | FcvtQD | FcvtHQ | FcvtQH => Class::FToF,
            Vsetvli | Vsetvl => Class::Vset,
            Vsetivli => Class::Vseti,
            H3Bextm => Class::H3Bextm,
            H3Bextmi => Class::H3Bextmi,
            Illegal => Class::Bare,
            _ => Class::RArith, // OP / OP-32 / M / Zb register-register
        }
    }
}

enum Class {
    U,
    J,
    Jalr,
    B,
    Load,
    LoadPair,
    Store,
    StorePair,
    FLoad,
    FStore,
    IArith,
    Shift,
    RArith,
    Unary,
    Bare,
    PrivFenceVm,
    PrivFence,
    PrivFence2,
    HLoad,
    HStore,
    ZcmpStack,
    ZcmpMove,
    Zcmt,
    Cbo,
    Prefetch,
    Csr,
    Csri,
    Lr,
    Amo,
    CryptoBs,
    FBin,
    FUnary,
    FMA,
    FCmp,
    FToX,
    XToF,
    FToF,
    Fli,
    Vset,
    Vseti,
    AndesGpLoad,
    AndesGpStore,
    AndesAddigp,
    AndesBfo,
    AndesBranch,
    TheadAddr,
    TheadFence,
    TheadAddsl,
    TheadExt,
    TheadFmvHwX,
    TheadFmvXHw,
    TheadAutoMem,
    TheadRegMem,
    TheadPairMem,
    TheadFMem,
    TheadVec,
    H3Bextm,
    H3Bextmi,
}

impl fmt::Display for Insn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let m = self.op.mnemonic();
        let rd = x_name(self.rd);
        let rs1 = x_name(self.rs1);
        let rs2 = x_name(self.rs2);
        let frd = f_name(self.rd);
        let frs1 = f_name(self.rs1);
        let frs2 = f_name(self.rs2);
        let frs3 = f_name(self.rs3);
        let imm = self.imm;
        match self.op.class() {
            Class::U => write!(f, "{m} {rd}, {:#x}", (imm >> 12) & 0xfffff),
            Class::J => write!(f, "{m} {rd}, .{:+#x}", imm),
            Class::Jalr => write!(f, "{m} {rd}, {imm}({rs1})"),
            Class::B => write!(f, "{m} {rs1}, {rs2}, .{:+#x}", imm),
            Class::Load => write!(f, "{m} {rd}, {imm}({rs1})"),
            Class::LoadPair => write!(f, "{m} {rd}/{}, {imm}({rs1})", x_name(self.rd + 1)),
            Class::Store => write!(f, "{m} {rs2}, {imm}({rs1})"),
            Class::StorePair => write!(f, "{m} {rs2}/{}, {imm}({rs1})", x_name(self.rs2 + 1)),
            Class::FLoad => write!(f, "{m} {frd}, {imm}({rs1})"),
            Class::FStore => write!(f, "{m} {frs2}, {imm}({rs1})"),
            Class::IArith => write!(f, "{m} {rd}, {rs1}, {imm}"),
            Class::Shift => write!(f, "{m} {rd}, {rs1}, {imm}"),
            Class::RArith => write!(f, "{m} {rd}, {rs1}, {rs2}"),
            Class::Unary => write!(f, "{m} {rd}, {rs1}"),
            Class::Bare => write!(f, "{m}"),
            Class::PrivFenceVm => {
                if self.rs1 == 0 {
                    write!(f, "{m}")
                } else {
                    write!(f, "{m} {rs1}")
                }
            }
            Class::PrivFence => {
                if self.rs2 == 0 {
                    if self.rs1 == 0 {
                        write!(f, "{m}")
                    } else {
                        write!(f, "{m} {rs1}")
                    }
                } else {
                    write!(f, "{m} {rs1}, {rs2}")
                }
            }
            Class::PrivFence2 => write!(f, "{m} {rs1}, {rs2}"),
            Class::HLoad => write!(f, "{m} {rd}, ({rs1})"),
            Class::HStore => write!(f, "{m} {rs2}, ({rs1})"),
            Class::ZcmpStack => {
                if matches!(self.op, Op::CmPush) {
                    write!(f, "{m} {}, -{}", zcmp_rlist(self.rd), self.imm)
                } else {
                    write!(f, "{m} {}, {}", zcmp_rlist(self.rd), self.imm)
                }
            }
            Class::ZcmpMove => write!(f, "{m} {rd}, {rs1}"),
            Class::Zcmt => write!(f, "{m} {}", self.imm),
            Class::Cbo => write!(f, "{m} ({rs1})"),
            Class::Prefetch => write!(f, "{m} {imm}({rs1})"),
            Class::Csr => write!(f, "{m} {rd}, {:#x}, {rs1}", self.csr),
            Class::Csri => write!(f, "{m} {rd}, {:#x}, {}", self.csr, self.rs1),
            Class::Lr => write!(f, "{m} {rd}, ({rs1})"),
            Class::Amo => write!(f, "{m} {rd}, {rs2}, ({rs1})"),
            Class::CryptoBs => write!(f, "{m} {rd}, {rs1}, {rs2}, {imm}"),
            Class::FBin => write!(f, "{m} {frd}, {frs1}, {frs2}"),
            Class::FUnary => write!(f, "{m} {frd}, {frs1}"),
            Class::FMA => write!(f, "{m} {frd}, {frs1}, {frs2}, {frs3}"),
            Class::FCmp => write!(f, "{m} {rd}, {frs1}, {frs2}"),
            Class::FToX => write!(f, "{m} {rd}, {frs1}"),
            Class::XToF => write!(f, "{m} {frd}, {rs1}"),
            Class::FToF => write!(f, "{m} {frd}, {frs1}"),
            Class::Fli => write!(f, "{m} {frd}, #{}", self.rs1),
            Class::Vset => write!(f, "{m} {rd}, {rs1}, {:#x}", self.imm),
            Class::Vseti => write!(f, "{m} {rd}, {}, {:#x}", self.rs1, self.imm),
            Class::AndesGpLoad => write!(f, "{m} {rd}, {}", self.imm),
            Class::AndesGpStore => write!(f, "{m} {rs2}, {}", self.imm),
            Class::AndesAddigp => write!(f, "{m} {rd}, {}", self.imm),
            Class::AndesBfo => write!(f, "{m} {rd}, {rs1}, {}, {}", self.rs2, self.imm),
            Class::AndesBranch => write!(f, "{m} {rs1}, {}, .{:+#x}", self.rs2, self.imm),
            Class::TheadAddr => write!(f, "{m} {rs1}"),
            Class::TheadFence => write!(f, "{m} {rs1}, {rs2}"),
            Class::TheadAddsl => write!(f, "{m} {rd}, {rs1}, {rs2}, {}", self.imm),
            Class::TheadExt => write!(f, "{m} {rd}, {rs1}, {}, {}", self.rs2, self.imm),
            Class::TheadFmvHwX => write!(f, "{m} {frd}, {rs1}"),
            Class::TheadFmvXHw => write!(f, "{m} {rd}, {frs1}"),
            Class::TheadAutoMem => write!(
                f,
                "{m} {rd}, ({rs1}), {}, {}",
                thead_sext5(self.rs2),
                self.imm
            ),
            Class::TheadRegMem => write!(f, "{m} {rd}, {rs1}, {rs2}, {}", self.imm),
            Class::TheadPairMem => write!(f, "{m} {rd}, {rs2}, {}({rs1})", self.imm),
            Class::TheadFMem => write!(f, "{m} {frd}, {rs1}, {rs2}, {}", self.imm),
            Class::TheadVec => {
                let suffix = if thead_vec_scalar(self) { ".vx" } else { ".vv" };
                let mask = if (self.raw >> 25) & 1 == 0 {
                    ", v0.t"
                } else {
                    ""
                };
                if thead_vec_clip_or_add(self.op) {
                    if thead_vec_scalar(self) {
                        write!(f, "{m}{suffix} v{}, v{}, {rs1}{mask}", self.rd, self.rs2)
                    } else {
                        write!(
                            f,
                            "{m}{suffix} v{}, v{}, v{}{mask}",
                            self.rd, self.rs2, self.rs1
                        )
                    }
                } else if thead_vec_scalar(self) {
                    write!(f, "{m}{suffix} v{}, {rs1}, v{}{mask}", self.rd, self.rs2)
                } else {
                    write!(
                        f,
                        "{m}{suffix} v{}, v{}, v{}{mask}",
                        self.rd, self.rs1, self.rs2
                    )
                }
            }
            Class::H3Bextm => write!(f, "{m} {rd}, {rs1}, {rs2}, {}", self.imm),
            Class::H3Bextmi => write!(f, "{m} {rd}, {rs1}, {}, {}", self.rs2, self.imm),
        }
    }
}

fn zcmp_rlist(rlist: u8) -> String {
    let Some(last) = (match rlist {
        4 => Some("ra"),
        5 => Some("s0"),
        6 => Some("s1"),
        7 => Some("s2"),
        8 => Some("s3"),
        9 => Some("s4"),
        10 => Some("s5"),
        11 => Some("s6"),
        12 => Some("s7"),
        13 => Some("s8"),
        14 => Some("s9"),
        15 => Some("s11"),
        _ => None,
    }) else {
        return format!("{{rlist={rlist}}}");
    };
    match rlist {
        4 => "{ra}".to_string(),
        5 => "{ra,s0}".to_string(),
        _ => format!("{{ra,s0-{last}}}"),
    }
}

fn thead_sext5(field: u8) -> i64 {
    (((field << 3) as i8) >> 3) as i64
}

fn thead_bare(op: Op) -> bool {
    use Op::*;
    matches!(
        op,
        ThDcacheCall
            | ThDcacheCiall
            | ThDcacheIall
            | ThIcacheIall
            | ThIcacheIalls
            | ThL2cacheCall
            | ThL2cacheCiall
            | ThL2cacheIall
            | ThSync
            | ThSyncS
            | ThSyncI
            | ThSyncIS
            | ThIpush
            | ThIpop
    )
}

fn thead_addr(op: Op) -> bool {
    use Op::*;
    matches!(
        op,
        ThDcacheCpa
            | ThDcacheCipa
            | ThDcacheIpa
            | ThDcacheCva
            | ThDcacheCiva
            | ThDcacheIva
            | ThDcacheCsw
            | ThDcacheCisw
            | ThDcacheIsw
            | ThDcacheCpal1
            | ThDcacheCval1
            | ThIcacheIpa
            | ThIcacheIva
    )
}

fn thead_auto_mem(op: Op) -> bool {
    use Op::*;
    matches!(
        op,
        ThLbia
            | ThLbib
            | ThLbuia
            | ThLbuib
            | ThLhia
            | ThLhib
            | ThLhuia
            | ThLhuib
            | ThLwia
            | ThLwib
            | ThLwuia
            | ThLwuib
            | ThLdia
            | ThLdib
            | ThSbia
            | ThSbib
            | ThShia
            | ThShib
            | ThSwia
            | ThSwib
            | ThSdia
            | ThSdib
    )
}

fn thead_reg_mem(op: Op) -> bool {
    use Op::*;
    matches!(
        op,
        ThLrb
            | ThLrbu
            | ThLrh
            | ThLrhu
            | ThLrw
            | ThLrwu
            | ThLrd
            | ThSrb
            | ThSrh
            | ThSrw
            | ThSrd
            | ThLurb
            | ThLurbu
            | ThLurh
            | ThLurhu
            | ThLurw
            | ThLurwu
            | ThLurd
            | ThSurb
            | ThSurh
            | ThSurw
            | ThSurd
    )
}

fn thead_pair_mem(op: Op) -> bool {
    use Op::*;
    matches!(op, ThLdd | ThLwd | ThLwud | ThSdd | ThSwd)
}

fn thead_fmem(op: Op) -> bool {
    use Op::*;
    matches!(
        op,
        ThFlrd | ThFlrw | ThFlurd | ThFlurw | ThFsrd | ThFsrw | ThFsurd | ThFsurw
    )
}

fn thead_vec(op: Op) -> bool {
    use Op::*;
    matches!(
        op,
        ThVmaqa
            | ThVmaqau
            | ThVmaqasu
            | ThVmaqaus
            | ThVpmaqa
            | ThVpmaqau
            | ThVpmaqasu
            | ThVpmaqaus
            | ThVpnclip
            | ThVpnclipu
            | ThVpwadd
            | ThVpwaddu
    )
}

fn thead_vec_clip_or_add(op: Op) -> bool {
    use Op::*;
    matches!(op, ThVpnclip | ThVpnclipu | ThVpwadd | ThVpwaddu)
}

fn thead_vec_scalar(insn: &Insn) -> bool {
    ((insn.raw >> 26) & 1) != 0
}

#[cfg(test)]
mod tests {
    use super::super::decode::{decode, decode_compressed};
    use super::super::{Isa, Xlen};

    fn dis(w: u32) -> String {
        decode(w, Xlen::Rv64, &Isa::rv64gc()).to_string()
    }

    fn dis_rv32(w: u32) -> String {
        decode(w, Xlen::Rv32, &Isa::rv64gc()).to_string()
    }

    #[test]
    fn disasm_samples() {
        // addi a0, a1, 5
        assert_eq!(
            dis((5u32 << 20) | (11 << 15) | (10 << 7) | 0x13),
            "addi a0, a1, 5"
        );
        // add a0, a1, a2
        assert_eq!(
            dis((12u32 << 20) | (11 << 15) | (10 << 7) | 0x33),
            "add a0, a1, a2"
        );
        // ld a0, 16(sp)
        assert_eq!(
            dis((16u32 << 20) | (2 << 15) | (3 << 12) | (10 << 7) | 0x03),
            "ld a0, 16(sp)"
        );
        // sd a0, 8(sp)
        assert_eq!(
            dis((0u32 << 25) | (10 << 20) | (2 << 15) | (3 << 12) | (8 << 7) | 0x23),
            "sd a0, 8(sp)"
        );
        // fadd.s fa0, fa1, fa2
        assert_eq!(
            dis((0u32 << 25) | (12 << 20) | (11 << 15) | (10 << 7) | 0x53),
            "fadd.s fa0, fa1, fa2"
        );
        // ecall
        assert_eq!(dis(0x0000_0073), "ecall");
        assert_eq!(dis(0x0100_000f), "pause");
        assert_eq!(dis(0x00d0_0073), "wrs.nto");
        assert_eq!(
            dis((1 << 20) | (10 << 15) | (2 << 12) | 0x0f),
            "cbo.clean (a0)"
        );
        assert_eq!(
            dis((1 << 20) | (10 << 15) | (6 << 12) | 0x13),
            "prefetch.r 0(a0)"
        );
        assert_eq!(
            dis((0b00101 << 27) | (6 << 20) | (10 << 15) | (0b010 << 12) | (5 << 7) | 0x2f),
            "amocas.w t0, t1, (a0)"
        );
        assert_eq!(
            dis((3 << 30) | (0b11000 << 25) | (7 << 20) | (6 << 15) | (5 << 7) | 0x33),
            "sm4ed t0, t1, t2, 3"
        );
        assert_eq!(
            dis_rv32((2 << 30) | (0b10011 << 25) | (7 << 20) | (6 << 15) | (5 << 7) | 0x33),
            "aes32esmi t0, t1, t2, 2"
        );
        assert_eq!(
            dis_rv32((0x2a << 25) | (7 << 20) | (6 << 15) | (5 << 7) | 0x33),
            "sha512sig0l t0, t1, t2"
        );
    }

    #[test]
    fn disasm_xida_sltw_when_enabled() {
        let mut isa = Isa::rv64gc();
        isa.xida_sltw = true;
        let sltw = (6u32 << 20) | (5 << 15) | (2 << 12) | (7 << 7) | 0x3b;
        assert_eq!(
            decode(sltw, Xlen::Rv64, &isa).to_string(),
            "sltw t2, t0, t1"
        );
    }

    #[test]
    fn disasm_q_extension_when_enabled() {
        let mut isa = Isa::rv64gc();
        isa.q = true;
        let dis_q = |w| decode(w, Xlen::Rv64, &isa).to_string();
        let enc = |funct7: u32, f5: u32, rs1: u32, funct3: u32, rd: u32, opcode: u32| {
            (funct7 << 25) | (f5 << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | opcode
        };

        assert_eq!(
            dis_q((16u32 << 20) | (10 << 15) | (4 << 12) | (10 << 7) | 0x07),
            "flq fa0, 16(a0)"
        );
        assert_eq!(
            dis_q((10u32 << 20) | (10 << 15) | (4 << 12) | (16 << 7) | 0x27),
            "fsq fa0, 16(a0)"
        );
        assert_eq!(
            dis_q(enc(0b0000011, 11, 10, 0, 12, 0x53)),
            "fadd.q fa2, fa0, fa1"
        );
        assert_eq!(
            dis_q((13u32 << 27) | (0b11 << 25) | (12 << 20) | (11 << 15) | (10 << 7) | 0x43),
            "fmadd.q fa0, fa1, fa2, fa3"
        );
        assert_eq!(
            dis_q(enc(0b0100011, 1, 11, 0, 10, 0x53)),
            "fcvt.q.d fa0, fa1"
        );
        assert_eq!(
            dis_q(enc(0b1100011, 0, 11, 0, 10, 0x53)),
            "fcvt.w.q a0, fa1"
        );
        assert_eq!(
            dis_q(enc(0b1110011, 0, 11, 1, 10, 0x53)),
            "fclass.q a0, fa1"
        );
    }

    #[test]
    fn disasm_zcmp_zcmt_and_pair_load_store() {
        let mut isa = Isa::rv64gc();
        isa.zcmp = true;
        isa.zcmt = true;

        let cm_push = ((0b101 << 13) | (0x18 << 8) | (5 << 4) | (1 << 2) | 0b10) as u16;
        assert_eq!(
            decode_compressed(cm_push, Xlen::Rv64, &isa).to_string(),
            "cm.push {ra,s0}, -32"
        );

        let cm_jalt = ((0b101 << 13) | (32 << 2) | 0b10) as u16;
        assert_eq!(
            decode_compressed(cm_jalt, Xlen::Rv64, &isa).to_string(),
            "cm.jalt 32"
        );

        let mut rv32 = Isa::rv64gc();
        rv32.zilsd = true;
        let load_pair = (8u32 << 20) | (10 << 15) | (3 << 12) | (6 << 7) | 0x03;
        assert_eq!(
            decode(load_pair, Xlen::Rv32, &rv32).to_string(),
            "ld t1/t2, 8(a0)"
        );
    }

    #[test]
    fn disasm_privileged_and_zbkb_zip_unzip() {
        let sys =
            |funct7: u32, rs2: u32, rs1: u32| (funct7 << 25) | (rs2 << 20) | (rs1 << 15) | 0x73;
        assert_eq!(
            decode(sys(0x00, 0x02, 0), Xlen::Rv64, &Isa::rv64gc()).to_string(),
            "uret"
        );
        assert_eq!(
            decode(sys(0x08, 0x04, 10), Xlen::Rv64, &Isa::rv64gc()).to_string(),
            "sfence.vm a0"
        );
        assert_eq!(
            decode(sys(0x09, 0, 0), Xlen::Rv64, &Isa::rv64gc()).to_string(),
            "sfence.vma"
        );
        assert_eq!(
            decode(sys(0x09, 0, 10), Xlen::Rv64, &Isa::rv64gc()).to_string(),
            "sfence.vma a0"
        );
        assert_eq!(
            decode(sys(0x09, 11, 10), Xlen::Rv64, &Isa::rv64gc()).to_string(),
            "sfence.vma a0, a1"
        );
        assert_eq!(
            decode(sys(0x0b, 11, 10), Xlen::Rv64, &Isa::rv64gc()).to_string(),
            "sinval.vma a0, a1"
        );
        assert_eq!(
            decode(sys(0x0c, 0, 0), Xlen::Rv64, &Isa::rv64gc()).to_string(),
            "sfence.w.inval"
        );
        assert_eq!(
            decode(sys(0x11, 11, 10), Xlen::Rv64, &Isa::rv64gc()).to_string(),
            "hfence.vvma a0, a1"
        );
        assert_eq!(
            decode(sys(0x33, 11, 10), Xlen::Rv64, &Isa::rv64gc()).to_string(),
            "hinval.gvma a0, a1"
        );
        assert_eq!(
            decode(
                (0x32 << 25) | (3 << 20) | (10 << 15) | (4 << 12) | (5 << 7) | 0x73,
                Xlen::Rv64,
                &Isa::rv64gc(),
            )
            .to_string(),
            "hlvx.hu t0, (a0)"
        );
        assert_eq!(
            decode(
                (0x35 << 25) | (6 << 20) | (10 << 15) | (4 << 12) | 0x73,
                Xlen::Rv64,
                &Isa::rv64gc(),
            )
            .to_string(),
            "hsv.w t1, (a0)"
        );
        assert_eq!(
            decode(
                (0x04 << 25) | (15 << 20) | (10 << 15) | (1 << 12) | (5 << 7) | 0x13,
                Xlen::Rv32,
                &Isa::rv64gc(),
            )
            .to_string(),
            "zip t0, a0"
        );
        assert_eq!(
            decode(
                (0x04 << 25) | (15 << 20) | (10 << 15) | (5 << 12) | (5 << 7) | 0x13,
                Xlen::Rv32,
                &Isa::rv64gc(),
            )
            .to_string(),
            "unzip t0, a0"
        );
    }

    #[test]
    fn disasm_hazard3_vendor_ops() {
        let isa = Isa {
            xhazard3: true,
            ..Isa::rv_i()
        };
        assert_eq!(
            decode(
                (0b0000110 << 25) | (5 << 20) | (10 << 15) | (3 << 7) | 0x0b,
                Xlen::Rv32,
                &isa
            )
            .to_string(),
            "h3.bextm gp, a0, t0, 4"
        );
        let imm12 = (0b101 << 6) | 17;
        assert_eq!(
            decode(
                (imm12 << 20) | (10 << 15) | (0b100 << 12) | (3 << 7) | 0x0b,
                Xlen::Rv32,
                &isa
            )
            .to_string(),
            "h3.bextmi gp, a0, 17, 6"
        );
        assert_eq!(
            decode((1 << 20) | (0b010 << 12) | 0x33, Xlen::Rv32, &isa).to_string(),
            "h3.unblock"
        );
    }

    #[test]
    fn disasm_andes_vendor_ops() {
        let isa = Isa {
            xandes: true,
            ..Isa::rv64gc()
        };
        assert_eq!(
            decode((3 << 21) | (1 << 14) | (5 << 7) | 0x0b, Xlen::Rv64, &isa).to_string(),
            "nds.lbgp t0, 7"
        );
        assert_eq!(
            decode((6 << 20) | (0b11 << 12) | 0x0b, Xlen::Rv64, &isa).to_string(),
            "nds.sbgp t1, 0"
        );
        assert_eq!(
            decode(
                (12 << 26) | (4 << 20) | (10 << 15) | (0b010 << 12) | (5 << 7) | 0x5b,
                Xlen::Rv64,
                &isa,
            )
            .to_string(),
            "nds.bfoz t0, a0, 12, 4"
        );
        assert_eq!(
            decode(
                (0x06 << 25) | (5 << 20) | (4 << 15) | (3 << 7) | 0x5b,
                Xlen::Rv64,
                &isa
            )
            .to_string(),
            "nds.lea.w gp, tp, t0"
        );
        assert_eq!(
            decode(
                (1 << 30) | (5 << 20) | (10 << 15) | (0b101 << 12) | (1 << 7) | 0x5b,
                Xlen::Rv64,
                &isa,
            )
            .to_string(),
            "nds.beqc a0, 101, .+0x0"
        );
    }

    #[test]
    fn disasm_xthead_vendor_ops() {
        let isa = Isa {
            xthead: true,
            ..Isa::rv64gc()
        };
        assert_eq!(
            decode((0b11000 << 20) | 0x0b, Xlen::Rv64, &isa).to_string(),
            "th.sync"
        );
        assert_eq!(
            decode(
                (1 << 25) | (0b01001 << 20) | (10 << 15) | 0x0b,
                Xlen::Rv64,
                &isa
            )
            .to_string(),
            "th.dcache.cpa a0"
        );
        assert_eq!(
            decode(
                (2 << 25) | (6 << 20) | (5 << 15) | (0b001 << 12) | (7 << 7) | 0x0b,
                Xlen::Rv64,
                &isa,
            )
            .to_string(),
            "th.addsl t2, t0, t1, 2"
        );
        assert_eq!(
            decode(
                ((12 << 1) << 25) | (4 << 20) | (10 << 15) | (0b010 << 12) | (5 << 7) | 0x0b,
                Xlen::Rv64,
                &isa,
            )
            .to_string(),
            "th.ext t0, a0, 12, 4"
        );
        assert_eq!(
            decode(
                (0x0d << 25) | (0b11110 << 20) | (11 << 15) | (0b100 << 12) | (10 << 7) | 0x0b,
                Xlen::Rv64,
                &isa,
            )
            .to_string(),
            "th.lbia a0, (a1), -2, 1"
        );
        assert_eq!(
            decode(
                (0x72 << 25) | (12 << 20) | (11 << 15) | (0b100 << 12) | (10 << 7) | 0x0b,
                Xlen::Rv64,
                &isa,
            )
            .to_string(),
            "th.lwd a0, a2, 16(a1)"
        );
        assert_eq!(
            decode(
                (0x22 << 25) | (12 << 20) | (11 << 15) | (0b110 << 12) | (10 << 7) | 0x0b,
                Xlen::Rv64,
                &isa,
            )
            .to_string(),
            "th.flrw fa0, a1, a2, 2"
        );
        assert_eq!(
            decode(
                (((0x20 << 1) | 1) << 25)
                    | (12 << 20)
                    | (11 << 15)
                    | (0b110 << 12)
                    | (10 << 7)
                    | 0x0b,
                Xlen::Rv64,
                &isa
            )
            .to_string(),
            "th.vmaqa.vv v10, v11, v12"
        );
        assert_eq!(
            decode(
                ((0x25 << 1) << 25) | (12 << 20) | (11 << 15) | (0b110 << 12) | (10 << 7) | 0x0b,
                Xlen::Rv64,
                &isa
            )
            .to_string(),
            "th.vmaqasu.vx v10, a1, v12, v0.t"
        );
        assert_eq!(
            decode(
                (((0x29 << 1) | 1) << 25)
                    | (12 << 20)
                    | (11 << 15)
                    | (0b111 << 12)
                    | (10 << 7)
                    | 0x0b,
                Xlen::Rv64,
                &isa
            )
            .to_string(),
            "th.vpnclip.vx v10, v12, a1"
        );
    }
}
