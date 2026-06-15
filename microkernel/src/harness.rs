//! Tiny self-checking test harness.
//!
//! Tests call the `eq_*`/`check` helpers with a name, the value they computed,
//! and the value they expect. Every check prints a one-line verdict so a CI log
//! shows exactly what ran, and [`Harness::finish`] prints the machine-readable
//! sentinel the host runner greps for:
//!
//! ```text
//! RAX-MK: RESULT PASS        // every check passed
//! RAX-MK: RESULT FAIL ...    // at least one check failed (or a panic)
//! ```

pub struct Harness {
    arch: &'static str,
    group: &'static str,
    pub total: u32,
    pub passed: u32,
    pub failed: u32,
    first_fail: Option<&'static str>,
}

impl Harness {
    pub const fn new(arch: &'static str) -> Self {
        Harness {
            arch,
            group: "",
            total: 0,
            passed: 0,
            failed: 0,
            first_fail: None,
        }
    }

    pub fn banner(&self) {
        println!("========================================");
        println!("  RAX multi-arch microkernel test suite");
        println!("  arch = {}", self.arch);
        println!("========================================");
    }

    /// Start a named group of checks (purely for readable output).
    pub fn group(&mut self, name: &'static str) {
        self.group = name;
        println!("--- {name} ---");
    }

    fn record(&mut self, name: &'static str, ok: bool) {
        self.total += 1;
        if ok {
            self.passed += 1;
            println!("[ ok ] {}/{}", self.group, name);
        } else {
            self.failed += 1;
            if self.first_fail.is_none() {
                self.first_fail = Some(name);
            }
        }
    }

    /// Boolean check (use for invariants where there is no single expected int).
    pub fn check(&mut self, name: &'static str, ok: bool) {
        if !ok {
            println!("[FAIL] {}/{}: invariant violated", self.group, name);
        }
        self.record(name, ok);
    }

    pub fn eq_u64(&mut self, name: &'static str, got: u64, exp: u64) {
        if got != exp {
            println!(
                "[FAIL] {}/{}: got={:#x} exp={:#x}",
                self.group, name, got, exp
            );
        }
        self.record(name, got == exp);
    }

    pub fn eq_i64(&mut self, name: &'static str, got: i64, exp: i64) {
        if got != exp {
            println!("[FAIL] {}/{}: got={} exp={}", self.group, name, got, exp);
        }
        self.record(name, got == exp);
    }

    pub fn eq_u32(&mut self, name: &'static str, got: u32, exp: u32) {
        self.eq_u64(name, got as u64, exp as u64);
    }

    pub fn eq_usize(&mut self, name: &'static str, got: usize, exp: usize) {
        self.eq_u64(name, got as u64, exp as u64);
    }

    /// Print the final summary and the result sentinel, then return whether the
    /// whole suite passed.
    pub fn finish(&self) -> bool {
        let ok = self.failed == 0;
        println!("========================================");
        println!(
            "RAX-MK arch={} total={} passed={} failed={}",
            self.arch, self.total, self.passed, self.failed
        );
        if ok {
            println!("RAX-MK: RESULT PASS");
        } else {
            match self.first_fail {
                Some(name) => println!("RAX-MK: RESULT FAIL first={name}"),
                None => println!("RAX-MK: RESULT FAIL"),
            }
        }
        println!("========================================");
        ok
    }
}
