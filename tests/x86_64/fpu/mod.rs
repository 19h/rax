//! x87 FPU instruction tests
//!
//! This module contains comprehensive tests for x87 Floating Point Unit instructions.

#[path = "../common/mod.rs"]
mod common;

mod f2xm1;
mod fabs;
mod fadd;
mod fchs;
mod fclex_fnclex;
mod fcom;
mod fdiv;
mod ffree;
mod fincstp_fdecstp;
mod finit_fninit;
mod fld;
mod fld_constants;
mod fldcw_fstcw;
mod fldenv_fstenv;
mod fmul;
mod fnop;
mod fpatan;
mod fprem;
mod fprem1;
mod fptan;
mod frndint;
mod fsave_frstor;
mod fscale;
mod fsin_fcos;
mod fsincos;
mod fsqrt;
mod fst_fstp;
mod fsub;
mod fstsw_fnstsw;
mod ftst;
mod fxam;
mod fxch;
mod fxsave_fxrstor;
mod fxtract;
mod fyl2x;
mod fyl2xp1;
