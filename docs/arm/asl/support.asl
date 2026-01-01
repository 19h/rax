////////////////////////////////////////////////////////////////
// Functions to support AES
//
// The following functions are not defined in the current
// XML release but are necessary to build a working simulator
////////////////////////////////////////////////////////////////

bits(128) AESInvMixColumns(bits (128) op)
    assert FALSE;
    return Zeros(128);

bits(128) AESInvShiftRows(bits(128) op)
    assert FALSE;
    return Zeros(128);

bits(128) AESInvSubBytes(bits(128) op)
    assert FALSE;
    return Zeros(128);

bits(128) AESMixColumns(bits (128) op)
    assert FALSE;
    return Zeros(128);

bits(128) AESShiftRows(bits(128) op)
    assert FALSE;
    return Zeros(128);

bits(128) AESSubBytes(bits(128) op)
    assert FALSE;
    return Zeros(128);

////////////////////////////////////////////////////////////////
// End
////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////
// Functions to perform barrier operations
//
// The following functions are not defined in the current
// XML release but are necessary to build a working simulator
////////////////////////////////////////////////////////////////

DataMemoryBarrier(MBReqDomain domain, MBReqTypes types)
    return;

DataSynchronizationBarrier(MBReqDomain domain, MBReqTypes types)
    return;

ErrorSynchronizationBarrier(MBReqDomain domain, MBReqTypes types)
    return;

InstructionSynchronizationBarrier()
    return;

ProfilingSynchronizationBarrier()
    return;

SynchronizeContext()
    return;

////////////////////////////////////////////////////////////////
// End
////////////////////////////////////////////////////////////////

////////////////////////////////////////////////////////////////
// Functions to support debug features
//
// The following functions are not defined in the current
// XML release but are necessary to build a working simulator
////////////////////////////////////////////////////////////////

CTI_SetEventLevel(CrossTriggerIn id, signal level)
    assert FALSE;

CTI_SignalEvent(CrossTriggerIn id)
    assert FALSE;

DisableITRAndResumeInstructionPrefetch()
    assert FALSE;

boolean HaltingStep_DidNotStep()
    assert FALSE;
    return FALSE;

boolean HaltingStep_SteppedEX()
    assert FALSE;
    return FALSE;

ResetExternalDebugRegisters(boolean cold_reset)
    assert FALSE;

boolean SoftwareStep_DidNotStep()
    assert FALSE;
    return FALSE;

boolean SoftwareStep_SteppedEX()
    assert FALSE;
    return FALSE;

StopInstructionPrefetchAndEnableITR()
    assert FALSE;

////////////////////////////////////////////////////////////////
// End
////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////
// Feature support
////////////////////////////////////////////////////////////////

boolean HaveAnyAArch32()
    // return boolean IMPLEMENTATION_DEFINED;
    return TRUE;

boolean HighestELUsingAArch32()
    if !HaveAnyAArch32() then return FALSE;
    // return boolean IMPLEMENTATION_DEFINED;       // e.g. CFG32SIGNAL == HIGH
    return FALSE;

boolean HaveEL(bits(2) el)
    if el IN {EL1,EL0} then
        return TRUE;                             // EL1 and EL0 must exist
    // return boolean IMPLEMENTATION_DEFINED;
    return TRUE;

boolean IsSecureBelowEL3()
    if HaveEL(EL3) then
        return SCR_GEN[].NS == '0';
    elsif HaveEL(EL2) then
        return FALSE;
    else
        // TRUE if processor is Secure or FALSE if Non-secure;
        // return boolean IMPLEMENTATION_DEFINED;
        return FALSE;

boolean HasArchVersion(ArchVersion version)
    // return version == ARMv8p0 || boolean IMPLEMENTATION_DEFINED;
    return version IN {ARMv8p0, ARMv8p1, ARMv8p2, ARMv8p3};

boolean HaveAArch32EL(bits(2) el)
    // Return TRUE if Exception level 'el' supports AArch32 in this implementation
    if !HaveEL(el) then
        return FALSE;                    // The Exception level is not implemented
    elsif !HaveAnyAArch32() then
        return FALSE;                    // No Exception level can use AArch32
    elsif HighestELUsingAArch32() then
        return TRUE;                     // All Exception levels are using AArch32
    elsif el == HighestEL() then
        return FALSE;                    // The highest Exception level is using AArch64
    elsif el == EL0 then
        return TRUE;                     // EL0 must support using AArch32 if any AArch32
    // return boolean IMPLEMENTATION_DEFINED;
    return TRUE;

boolean Have16bitVMID()
    // return HaveEL(EL2) && boolean IMPLEMENTATION_DEFINED;
    return HaveEL(EL2);

boolean HaveFP16Ext()
    // return boolean IMPLEMENTATION_DEFINED;
    return TRUE;

////////////////////////////////////////////////////////////////
// End
////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////
// Functions to support instruction fetch/decode
//
// The following functions are not defined in the current
// XML release but are necessary to build a working simulator
////////////////////////////////////////////////////////////////

EndOfInstruction()
    __ExceptionTaken();


boolean __Sleeping;

EnterLowPowerState()
    __Sleeping = TRUE;

ExitLowPowerState()
    __Sleeping = FALSE;

__ResetExecuteState()
    __Sleeping    = FALSE;

ExecuteA64(bits(32) instr)
    __DecodeA64(integer UNKNOWN, instr);

ExecuteA32(bits(32) instr)
    __DecodeA32(integer UNKNOWN, instr);

ExecuteT32(bits(16) hw1, bits(16) hw2)
    __DecodeT32(integer UNKNOWN, hw1 : hw2);

ExecuteT16(bits(16) instr)
    __DecodeT16(integer UNKNOWN, instr);

// Implementation of BranchTo and BranchToAddr modified so that we can
// tell that a branch was taken - this is essential for implementing
// PC advance correctly.

boolean __BranchTaken;

BranchTo(bits(N) target, BranchType branch_type)
    __BranchTaken = TRUE; // extra line added
    Hint_Branch(branch_type);
    if N == 32 then
        assert UsingAArch32();
        _PC = ZeroExtend(target);
    else
        assert N == 64 && !UsingAArch32();
        _PC = AArch64.BranchAddr(target[63:0]);
    return;

BranchToAddr(bits(N) target, BranchType branch_type)
    __BranchTaken = TRUE; // extra line added
    Hint_Branch(branch_type);
    if N == 32 then
        assert UsingAArch32();
        _PC = ZeroExtend(target);
    else
        assert N == 64 && !UsingAArch32();
        _PC = target[63:0];
    return;

enumeration __InstrEnc { __A64, __A32, __T16, __T32 };

bits(32)   __ThisInstr;
__InstrEnc __ThisInstrEnc;
bits(4)    __currentCond;


__SetThisInstrDetails(__InstrEnc enc, bits(32) opcode, bits(4) cond)
    __ThisInstrEnc = enc;
    __ThisInstr    = opcode;
    __currentCond  = cond;
    return;

bits(32) ThisInstr()
    return __ThisInstr;

// Length in bits of instruction
integer ThisInstrLength()
    return if __ThisInstrEnc == __T16 then 16 else 32;

bits(4) AArch32.CurrentCond()
    return __currentCond;

bits(N) ThisInstrAddr()
    return _PC[0 +: N];

bits(N) NextInstrAddr()
    return (_PC + (ThisInstrLength() DIV 8))[N-1:0];

(__InstrEnc, bits(32)) __FetchInstr(bits(64) pc)
    __InstrEnc enc;
    bits(32)   instr;

    CheckSoftwareStep();

    if PSTATE.nRW == '0' then
        AArch64.CheckPCAlignment();
        enc = __A64;
        instr = AArch64.MemSingle[pc, 4, AccType_IFETCH, TRUE];
        AArch64.CheckIllegalState();
    else
        AArch32.CheckPCAlignment();
        if PSTATE.T == '1' then
            hw1 = AArch32.MemSingle[pc[31:0], 2, AccType_IFETCH, TRUE];
            isT16 = UInt(hw1[15:11]) < UInt('11101');
            if isT16 then
                enc = __T16;
                instr = Zeros(16) : hw1;
            else
                hw2 = AArch32.MemSingle[pc[31:0]+2, 2, AccType_IFETCH, TRUE];
                enc   = __T32;
                instr = hw1 : hw2;
        else
            enc   = __A32;
            instr = AArch32.MemSingle[pc[31:0], 4, AccType_IFETCH, TRUE];
        AArch32.CheckIllegalState();

    return (enc, instr);

__DecodeExecute(__InstrEnc enc, bits(32) instr)
    case enc of
        when __A64
            ExecuteA64(instr);
        when __A32
            ExecuteA32(instr);
        when __T16
            ExecuteT16(instr[15:0]);
        when __T32
            ExecuteT32(instr[31:16], instr[15:0]);
    return;

// Default condition for an instruction with encoding 'enc'.
// This may be overridden for instructions with explicit condition field.
bits(4) __DefaultCond(__InstrEnc enc)
    if enc IN {__A64, __A32} || PSTATE.IT[3:0] == Zeros(4) then
        cond = 0xE[3:0];
    else
        cond = PSTATE.IT[7:4];
    return cond;

__InstructionExecute()
    try
        __BranchTaken = FALSE;
        bits(64) pc   = ThisInstrAddr();
        (enc, instr)  = __FetchInstr(pc);
        __SetThisInstrDetails(enc, instr, __DefaultCond(enc));
        __DecodeExecute(enc, instr);

    catch exn
        // Do not catch UNPREDICTABLE or internal errors
        when IsSEE(exn) || IsUNDEFINED(exn)
            if UsingAArch32() then
                if ConditionHolds(AArch32.CurrentCond()) then
                    AArch32.UndefinedFault();
            else
                AArch64.UndefinedFault();

        when IsExceptionTaken(exn)
            // Do nothing
            assert TRUE; // todo: this is a bodge around lack of support for empty statements

    if !__BranchTaken then
        _PC = (_PC + (ThisInstrLength() DIV 8))[63:0];

    boolean itExecuted = __ThisInstrEnc == __T16 && __ThisInstr[15:0] IN '1011 1111 xxxx xxxx' && __ThisInstr[3:0] != '0000';
    if PSTATE.nRW == '1' && PSTATE.T == '1' && !itExecuted then
        AArch32.ITAdvance();

    return;

////////////////////////////////////////////////////////////////
// The following functions define the IMPLEMENTATION_DEFINED behaviour
// of this execution
////////////////////////////////////////////////////////////////

boolean __IMPDEF_boolean(string x)
    if x == "Condition valid for trapped T32" then return TRUE;
    elsif x == "Has Dot Product extension" then return TRUE;
    elsif x == "Has RAS extension" then return TRUE;
    elsif x == "Has SHA512 and SHA3 Crypto instructions" then return TRUE;
    elsif x == "Has SM3 and SM4 Crypto instructions" then return TRUE;
    elsif x == "Has basic Crypto instructions" then return TRUE;
    elsif x == "Have CRC extension" then return TRUE;
    elsif x == "Report I-cache maintenance fault in IFSR" then return TRUE;
    elsif x == "Reserved Control Space EL0 Trapped" then return TRUE;
    elsif x == "Translation fault on misprogrammed contiguous bit" then return TRUE;
    elsif x == "UNDEF unallocated CP15 access at NS EL0" then return TRUE;
    elsif x == "UNDEF unallocated CP15 access at NS EL0" then return TRUE;

    return FALSE;

integer __IMPDEF_integer(string x)
    if x == "Maximum Physical Address Size" then return 52;
    elsif x == "Maximum Virtual Address Size" then return 56;

    return 0;

bits(N) __IMPDEF_bits(integer N, string x)
    if x == "0 or 1" then return Zeros(N);
    elsif x == "FPEXC.EN value when TGE==1 and RW==0" then return Zeros(N);
    elsif x == "reset vector address" then return Zeros(N);

    return Zeros(N);

MemoryAttributes __IMPDEF_MemoryAttributes(string x)
    return MemoryAttributes UNKNOWN;

// todo: implement defaults for these behaviours
// IMPLEMENTATION_DEFINED "floating-point trap handling";
// IMPLEMENTATION_DEFINED "signal slave-generated error";

////////////////////////////////////////////////////////////////
// The following functions are required by my simulator:
// - __TopLevel(): take one atomic step
// - __setPC(): set PC to particular value (used after loading an ELF file)
// - __getPC(): read current value of PC (used to support breakpointing)
// - __conditionPassed: set if executing a conditional instruction
// - __CycleEnd(): deprecated hook called after every instruction execution
// - __ModeString(): generate summary of current mode (used to support tracing)
////////////////////////////////////////////////////////////////

__TakeColdReset()
    PSTATE.nRW = '0'; // boot into A64 mode
    PSTATE.SS = '0';
    __ResetInterruptState();
    __ResetMemoryState();
    __ResetExecuteState();
    AArch64.TakeReset(TRUE);

__TopLevel()
    __InstructionExecute();

__setPC(integer x)
    _PC = x[63:0];
    return;

integer __getPC()
    return UInt(_PC);

boolean __conditionPassed;

__CycleEnd()
    return;

// Function used to generate summary of current state of processor
// (used when generating debug traces)
string __ModeString()
    return "";

////////////////////////////////////////////////////////////////
// End
////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////
// Functions to implement hint instructions
//
// The following functions are not defined in the current
// XML release but are necessary to build a working simulator
////////////////////////////////////////////////////////////////

Hint_Branch(BranchType hint)
    return;

Hint_Prefetch(bits(64) address, PrefetchHint hint, integer target, boolean stream)
    return;

Hint_PreloadDataForWrite(bits(32) address)
    return;

Hint_PreloadData(bits(32) address)
    return;

Hint_PreloadDataForWrite(bits(32) address)
    return;

Hint_PreloadInstr(bits(32) address)
    return;

Hint_Yield()
    return;

////////////////////////////////////////////////////////////////
// End
////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////
// Functions to support interrupts and System Errors
//
// The following functions are not defined in the current
// XML release but are necessary to build a working simulator
////////////////////////////////////////////////////////////////

boolean __PendingPhysicalSError;
boolean __PendingInterrupt;

__ResetInterruptState()
    __PendingPhysicalSError = FALSE;
    __PendingInterrupt = FALSE;

boolean InterruptPending()
    return __PendingInterrupt;

SendEvent()
    assert FALSE;

SetInterruptRequestLevel(InterruptID id, signal level)
    assert FALSE;

AArch32.SErrorSyndrome AArch32.PhysicalSErrorSyndrome()
    assert FALSE;
    AArch32.SErrorSyndrome r;
    r.AET = Zeros(2);
    r.ExT = Zeros(1);
    return r;

bits(25) AArch64.PhysicalSErrorSyndrome(boolean implicit_esb)
    assert FALSE;
    return Zeros(25);

__SetPendingPhysicalSError()
    __PendingPhysicalSError = TRUE;
    return;

ClearPendingPhysicalSError()
    __PendingPhysicalSError = FALSE;
    return;

boolean SErrorPending()
    // todo: can there be a pending virtual SError?
    return __PendingPhysicalSError;

TakeUnmaskedPhysicalSErrorInterrupts(boolean iesb_req)
    assert FALSE;

TakeUnmaskedSErrorInterrupts()
    assert FALSE;

////////////////////////////////////////////////////////////////
// End
////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////
// Functions to support exclusive memory accesses
//
// The following functions are not defined in the current
// XML release but are necessary to build a working simulator
////////////////////////////////////////////////////////////////

__RAM(52) __Memory;

boolean __ExclusiveLocal;

__ResetMemoryState()
    __InitRAM(52, 1, __Memory, Zeros(8)); // zero memory on reset
    __ExclusiveLocal = FALSE;

__ELFWriteMemory(bits(64) address, bits(8) val)
    __WriteRAM(52, 1, __Memory, address[0 +: 52], val);
    return;

bits(8*size) _Mem[AddressDescriptor desc, integer size, AccessDescriptor accdesc]
    assert size IN {1, 2, 4, 8, 16};
    bits(52) address = desc.paddress.address;
    assert address == Align(address, size);
    return __ReadRAM(52, size, __Memory, address);

_Mem[AddressDescriptor desc, integer size, AccessDescriptor accdesc] = bits(8*size) value
    assert size IN {1, 2, 4, 8, 16};
    bits(52) address = desc.paddress.address;
    assert address == Align(address, size);

    if address == 0x13000000[51:0] then // TUBE
        if UInt(value) == 0x4 then
            print("Program exited by writing ^D to TUBE\n");
            __abort();
        else
            putchar(UInt(value[7:0]));
    else
        __WriteRAM(52, size, __Memory, address, value);
    return;

ClearExclusiveLocal(integer processorid)
    __ExclusiveLocal = FALSE;
    return;

MarkExclusiveLocal(FullAddress paddress, integer processorid, integer size)
    __ExclusiveLocal = FALSE;

boolean IsExclusiveLocal(FullAddress paddress, integer processorid, integer size)
    return __ExclusiveLocal;


boolean AArch32.IsExclusiveVA(bits(32) address, integer processorid, integer size)
    assert FALSE;
    return FALSE;

AArch32.MarkExclusiveVA(bits(32) address, integer processorid, integer size)
    assert FALSE;

boolean AArch64.IsExclusiveVA(bits(64) address, integer processorid, integer size)
    assert FALSE;
    return FALSE;

AArch64.MarkExclusiveVA(bits(64) address, integer processorid, integer size)
    assert FALSE;

ClearExclusiveByAddress(FullAddress paddress, integer processorid, integer size)
    assert TRUE; // todo

bit ExclusiveMonitorsStatus()
    assert FALSE;
    return '0'; // '0' indicates success

boolean IsExclusiveGlobal(FullAddress paddress, integer processorid, integer size)
    assert FALSE;
    return FALSE;

MarkExclusiveGlobal(FullAddress paddress, integer processorid, integer size)
    assert FALSE;

integer ProcessorID()
    return 0;

bits(4) _MemTag[AddressDescriptor desc]
    assert FALSE;
    return Zeros(4);

_MemTag[AddressDescriptor desc] = bits(4) value
    assert FALSE;
    return;

boolean IsNonTagCheckedInstruction()
    assert FALSE;
    return FALSE;

SetNotTagCheckedInstruction(boolean unchecked)
    assert FALSE;
    return;

bits(4) _ChooseRandomNonExcludedTag(bits(16) exclude)
    assert FALSE;
    return Zeros(4);

(bits(64), integer) ImpDefTagArrayStartAndCount(bits(64) address)
    assert FALSE;
    return (Zeros(64), 0);

////////////////////////////////////////////////////////////////
// End
////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////
// Miscellaneous stub functions
//
// The following functions are not defined in the current
// XML release.
////////////////////////////////////////////////////////////////

AArch32.ResetControlRegisters(boolean cold_reset)
    assert FALSE;

AArch32.ResetSystemRegisters(boolean cold_reset)
    assert FALSE;

AArch64.ResetControlRegisters(boolean cold_reset)
    return;

AArch64.ResetSystemRegisters(boolean cold_reset)
    return;

ResetExternalDebugRegisters(boolean cold_reset)
    return;


bits(64) AArch32.SysRegRead64(integer cp_num, bits(32) instr)
    assert FALSE;
    return Zeros(64);

boolean AArch32.SysRegReadCanWriteAPSR(integer cp_num, bits(32) instr)
    assert FALSE;
    return FALSE;

bits(32) AArch32.SysRegRead(integer cp_num, bits(32) instr)
    assert FALSE;
    return Zeros(32);

bits(64) AArch32.SysRegRead64(integer cp_num, bits(32) instr)
    assert FALSE;
    return Zeros(64);

AArch32.SysRegWrite(integer cp_num, bits(32) instr, bits(32) val)
    assert FALSE;

AArch32.SysRegWrite64(integer cp_num, bits(32) instr, bits(64) val)
    assert FALSE;

AArch32.SysRegWrite64(integer cp_num, bits(32) instr, bits(64) val)
    assert FALSE;

(boolean, bits(2)) AArch64.CheckAdvSIMDFPSystemRegisterTraps(bits(2) op0, bits(3) op1, bits(4) crn, bits(4) crm, bits(3) op2, bit read)
    assert FALSE;
    return (FALSE, '00');

(boolean, bits(2)) AArch64.CheckAdvSIMDFPSystemRegisterTraps(bits(2) op0, bits(3) op1, bits(4) crn, bits(4) crm, bits(3) op2, bit read)
    assert FALSE;
    return (FALSE, '00');

(boolean, bits(2)) AArch64.CheckSystemRegisterTraps(bits(2) op0, bits(3) op1, bits(4) crn, bits(4) crm, bits(3) op2, bit read)
    assert FALSE;
    return (FALSE, '00');

boolean AArch64.CheckUnallocatedSystemAccess(bits(2) op0, bits(3) op1, bits(4) crn, bits(4) crm, bits(3) op2, bit read)
    assert FALSE;
    return FALSE;

bits(64) AArch64.SysInstrWithResult(integer op0, integer op1, integer crn, integer crm, integer op2)
    assert FALSE;
    return Zeros(64);

AArch64.SysInstr(integer op0, integer op1, integer crn, integer crm, integer op2, bits(64) val)
    assert FALSE;

bits(64) AArch64.SysInstrWithResult(integer op0, integer op1, integer crn, integer crm, integer op2)
    assert FALSE;
    return Zeros(64);

AArch64.SysRegWrite(integer op0, integer op1, integer crn, integer crm, integer op2, bits(64) val)
    assert FALSE;

bits(64) System_Get(integer op0, integer op1, integer crn, integer crm, integer op2)
    assert FALSE;
    return Zeros(64);

boolean CP14DebugInstrDecode(bits(32) instr)
    assert FALSE;
    return FALSE;

boolean CP14JazelleInstrDecode(bits(32) instr)
    assert FALSE;
    return FALSE;

boolean CP14TraceInstrDecode(bits(32) instr)
    assert FALSE;
    return FALSE;

boolean CP15InstrDecode(bits(32) instr)
    assert FALSE;
    return FALSE;

bits(11) LSInstructionSyndrome()
    assert FALSE;
    return Zeros(11);

boolean RemapRegsHaveResetValues()
    assert FALSE;
    return FALSE;

UndefinedFault()
    assert FALSE;
    return;

////////////////////////////////////////////////////////////////
// End
////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////
// Functions to support usermode execution
//
// The following functions provide simplified implementations
// of some key system architecture functions that are sufficient
// to model usermode behaviour.
////////////////////////////////////////////////////////////////

// Simplified version of AArch64 bit normalization of virtual addresses
bits(64) AArch64.BranchAddr(bits(64) vaddress)
    assert !UsingAArch32();
    integer msbit = 51;
    return ZeroExtend(vaddress[msbit:0]);

bits(size*8) AArch64.MemSingle[bits(64) address, integer size, AccType acctype, boolean wasaligned]
    AddressDescriptor desc;
    AccessDescriptor accdesc;
    desc.paddress.address = address[0 +: 52];
    return _Mem[desc, size, accdesc];

AArch64.MemSingle[bits(64) address, integer size, AccType acctype, boolean wasaligned] = bits(size*8) value
    AddressDescriptor desc;
    AccessDescriptor accdesc;
    desc.paddress.address = address[0 +: 52];
    _Mem[desc, size, accdesc] = value;
    return;


bits(size*8) Mem[bits(64) address, integer size, AccType acctype]
    return AArch64.MemSingle[address, size, acctype, TRUE];

Mem[bits(64) address, integer size, AccType acctype] = bits(size*8) value
    AArch64.MemSingle[address, size, acctype, TRUE] = value;
    return;

AArch64.TakeException(bits(2) target_el, ExceptionRecord exception,
                      bits(64) preferred_exception_return, integer vect_offset)
    assert FALSE;

AArch64.UndefinedFault()
    assert FALSE;

AArch32.UndefinedFault()
    assert FALSE;

ReservedValue()
    assert FALSE;

UnallocatedEncoding()
    assert FALSE;

EndOfInstruction()
    assert FALSE;

CheckSoftwareStep()
    return;

bits(64) AuthDA(bits(64) X, bits(64) Y)
    assert FALSE;
    return bits(64) UNKNOWN;

bits(64) AuthDB(bits(64) X, bits(64) Y)
    assert FALSE;
    return bits(64) UNKNOWN;

CheckSPAlignment()
    return;

AArch32.CheckPCAlignment()
    return;

ResetExternalDebugRegisters(boolean cold_reset)
    return;

AArch32.CheckIllegalState()
    return;

////////////////////////////////////////////////////////////////
// End
////////////////////////////////////////////////////////////////
