//! CR8 transfers at the VMX control-access boundary.
//! Intel SDM086 Vol.3A12.8.6.1 and Vol.3C table29-3.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
    ToCr8,
    FromCr8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Access {
    pub direction: Direction,
    pub gpr: u8,
}

/// Recognize only the implemented MOV CR8 forms; other control exits stay explicit.
pub(crate) fn decode(qualification: u64) -> Option<Access> {
    if qualification & !0x0f1f != 0 || qualification & 0x0f != 8 {
        return None;
    }
    Some(Access {
        direction: if qualification & 0x10 == 0 {
            Direction::ToCr8
        } else {
            Direction::FromCr8
        },
        gpr: ((qualification >> 8) & 0x0f) as u8,
    })
}

/// APIC.TPR[7:4]=CR8[3:0], with APIC.TPR[3:0] cleared by MOV to CR8.
pub(crate) fn tpr_value(cr8: u64) -> Option<u64> {
    (cr8 & !0x0f == 0).then_some(cr8 << 4)
}

pub(crate) trait Cr8Access {
    type Error;
    fn read_gpr(&mut self, index: u8) -> Result<u64, Self::Error>;
    fn write_gpr(&mut self, index: u8, value: u64) -> Result<(), Self::Error>;
    fn read_tpr(&mut self) -> Result<u64, Self::Error>;
    fn write_tpr(&mut self, value: u64) -> Result<(), Self::Error>;
    fn inject_gp(&mut self) -> Result<(), Self::Error>;
    fn advance_rip(&mut self) -> Result<(), Self::Error>;
}

/// False means an unrepresented control form, with no host calls or state change.
/// Valid transfers commit their destination before RIP; #GP preserves faulting RIP.
pub(crate) fn handle<T: Cr8Access>(io: &mut T, qualification: u64) -> Result<bool, T::Error> {
    let Some(access) = decode(qualification) else {
        return Ok(false);
    };
    match access.direction {
        Direction::ToCr8 => {
            let value = io.read_gpr(access.gpr)?;
            let Some(tpr) = tpr_value(value) else {
                io.inject_gp()?;
                return Ok(true);
            };
            io.write_tpr(tpr)?;
        }
        Direction::FromCr8 => {
            let tpr = io.read_tpr()?;
            io.write_gpr(access.gpr, (tpr >> 4) & 0x0f)?;
        }
    }
    io.advance_rip()?;
    Ok(true)
}
