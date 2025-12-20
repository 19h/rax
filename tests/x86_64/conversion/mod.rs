// Data conversion instructions test module
//
// This module contains comprehensive tests for x86_64 data conversion instructions:
// - MOVSX: Move with Sign Extension
// - MOVZX: Move with Zero Extension
// - CBW/CWDE/CDQE: Sign-extension accumulator operations
// - CWD/CDQ/CQO: Convert Word/Doubleword/Quadword to Doubleword/Quadword/Octaword

mod cbw_cwde_cdqe;
mod cwd_cdq_cqo;
mod movsx;
mod movzx;
