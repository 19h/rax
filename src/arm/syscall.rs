//! Linux ARM syscall emulation.
//!
//! This module provides emulation of Linux system calls for ARM user-space
//! programs. It implements the ARM EABI calling convention where:
//! - Syscall number is in R7
//! - Arguments are in R0-R6
//! - Return value is in R0 (negative for error)
//!
//! # Implemented Syscalls
//!
//! Basic I/O:
//! - read (3)
//! - write (4)
//! - open (5)
//! - close (6)
//!
//! Memory management:
//! - brk (45)
//! - mmap2 (192)
//! - munmap (91)
//!
//! Process control:
//! - exit (1)
//! - exit_group (248)
//! - getpid (20)
//! - gettid (224)
//!
//! Time:
//! - gettimeofday (78)
//! - clock_gettime (263)
//!
//! File system:
//! - stat64 (195)
//! - fstat64 (197)
//! - lstat64 (196)
//!
//! Architecture-specific:
//! - cacheflush (983042)
//! - set_tls (983045)

use std::collections::HashMap;
use std::io::{Read, Write};
use std::time::{SystemTime, UNIX_EPOCH};
use crate::arm::execution::{Armv7Cpu, ArmMemory, MemoryError};

// =============================================================================
// ARM EABI Syscall Numbers
// =============================================================================

/// Linux ARM EABI syscall numbers.
pub mod nr {
    pub const EXIT: u32 = 1;
    pub const FORK: u32 = 2;
    pub const READ: u32 = 3;
    pub const WRITE: u32 = 4;
    pub const OPEN: u32 = 5;
    pub const CLOSE: u32 = 6;
    pub const WAITPID: u32 = 7;
    pub const CREAT: u32 = 8;
    pub const LINK: u32 = 9;
    pub const UNLINK: u32 = 10;
    pub const EXECVE: u32 = 11;
    pub const CHDIR: u32 = 12;
    pub const TIME: u32 = 13;
    pub const MKNOD: u32 = 14;
    pub const CHMOD: u32 = 15;
    pub const LCHOWN: u32 = 16;
    pub const LSEEK: u32 = 19;
    pub const GETPID: u32 = 20;
    pub const MOUNT: u32 = 21;
    pub const UMOUNT: u32 = 22;
    pub const SETUID: u32 = 23;
    pub const GETUID: u32 = 24;
    pub const STIME: u32 = 25;
    pub const PTRACE: u32 = 26;
    pub const ALARM: u32 = 27;
    pub const PAUSE: u32 = 29;
    pub const UTIME: u32 = 30;
    pub const ACCESS: u32 = 33;
    pub const NICE: u32 = 34;
    pub const SYNC: u32 = 36;
    pub const KILL: u32 = 37;
    pub const RENAME: u32 = 38;
    pub const MKDIR: u32 = 39;
    pub const RMDIR: u32 = 40;
    pub const DUP: u32 = 41;
    pub const PIPE: u32 = 42;
    pub const TIMES: u32 = 43;
    pub const BRK: u32 = 45;
    pub const SETGID: u32 = 46;
    pub const GETGID: u32 = 47;
    pub const SIGNAL: u32 = 48;
    pub const GETEUID: u32 = 49;
    pub const GETEGID: u32 = 50;
    pub const ACCT: u32 = 51;
    pub const UMOUNT2: u32 = 52;
    pub const IOCTL: u32 = 54;
    pub const FCNTL: u32 = 55;
    pub const SETPGID: u32 = 57;
    pub const UMASK: u32 = 60;
    pub const CHROOT: u32 = 61;
    pub const USTAT: u32 = 62;
    pub const DUP2: u32 = 63;
    pub const GETPPID: u32 = 64;
    pub const GETPGRP: u32 = 65;
    pub const SETSID: u32 = 66;
    pub const SIGACTION: u32 = 67;
    pub const SETREUID: u32 = 70;
    pub const SETREGID: u32 = 71;
    pub const SIGSUSPEND: u32 = 72;
    pub const SIGPENDING: u32 = 73;
    pub const SETHOSTNAME: u32 = 74;
    pub const SETRLIMIT: u32 = 75;
    pub const GETRLIMIT: u32 = 76;
    pub const GETRUSAGE: u32 = 77;
    pub const GETTIMEOFDAY: u32 = 78;
    pub const SETTIMEOFDAY: u32 = 79;
    pub const GETGROUPS: u32 = 80;
    pub const SETGROUPS: u32 = 81;
    pub const SELECT: u32 = 82;
    pub const SYMLINK: u32 = 83;
    pub const READLINK: u32 = 85;
    pub const USELIB: u32 = 86;
    pub const SWAPON: u32 = 87;
    pub const REBOOT: u32 = 88;
    pub const READDIR: u32 = 89;
    pub const MMAP: u32 = 90;
    pub const MUNMAP: u32 = 91;
    pub const TRUNCATE: u32 = 92;
    pub const FTRUNCATE: u32 = 93;
    pub const FCHMOD: u32 = 94;
    pub const FCHOWN: u32 = 95;
    pub const GETPRIORITY: u32 = 96;
    pub const SETPRIORITY: u32 = 97;
    pub const STATFS: u32 = 99;
    pub const FSTATFS: u32 = 100;
    pub const SOCKETCALL: u32 = 102;
    pub const SYSLOG: u32 = 103;
    pub const SETITIMER: u32 = 104;
    pub const GETITIMER: u32 = 105;
    pub const STAT: u32 = 106;
    pub const LSTAT: u32 = 107;
    pub const FSTAT: u32 = 108;
    pub const VHANGUP: u32 = 111;
    pub const WAIT4: u32 = 114;
    pub const SWAPOFF: u32 = 115;
    pub const SYSINFO: u32 = 116;
    pub const IPC: u32 = 117;
    pub const FSYNC: u32 = 118;
    pub const SIGRETURN: u32 = 119;
    pub const CLONE: u32 = 120;
    pub const SETDOMAINNAME: u32 = 121;
    pub const UNAME: u32 = 122;
    pub const ADJTIMEX: u32 = 124;
    pub const MPROTECT: u32 = 125;
    pub const SIGPROCMASK: u32 = 126;
    pub const INIT_MODULE: u32 = 128;
    pub const DELETE_MODULE: u32 = 129;
    pub const QUOTACTL: u32 = 131;
    pub const GETPGID: u32 = 132;
    pub const FCHDIR: u32 = 133;
    pub const BDFLUSH: u32 = 134;
    pub const SYSFS: u32 = 135;
    pub const PERSONALITY: u32 = 136;
    pub const SETFSUID: u32 = 138;
    pub const SETFSGID: u32 = 139;
    pub const LLSEEK: u32 = 140;
    pub const GETDENTS: u32 = 141;
    pub const NEWSELECT: u32 = 142;
    pub const FLOCK: u32 = 143;
    pub const MSYNC: u32 = 144;
    pub const READV: u32 = 145;
    pub const WRITEV: u32 = 146;
    pub const GETSID: u32 = 147;
    pub const FDATASYNC: u32 = 148;
    pub const SYSCTL: u32 = 149;
    pub const MLOCK: u32 = 150;
    pub const MUNLOCK: u32 = 151;
    pub const MLOCKALL: u32 = 152;
    pub const MUNLOCKALL: u32 = 153;
    pub const SCHED_SETPARAM: u32 = 154;
    pub const SCHED_GETPARAM: u32 = 155;
    pub const SCHED_SETSCHEDULER: u32 = 156;
    pub const SCHED_GETSCHEDULER: u32 = 157;
    pub const SCHED_YIELD: u32 = 158;
    pub const SCHED_GET_PRIORITY_MAX: u32 = 159;
    pub const SCHED_GET_PRIORITY_MIN: u32 = 160;
    pub const SCHED_RR_GET_INTERVAL: u32 = 161;
    pub const NANOSLEEP: u32 = 162;
    pub const MREMAP: u32 = 163;
    pub const SETRESUID: u32 = 164;
    pub const GETRESUID: u32 = 165;
    pub const POLL: u32 = 168;
    pub const SETRESGID: u32 = 170;
    pub const GETRESGID: u32 = 171;
    pub const PRCTL: u32 = 172;
    pub const RT_SIGRETURN: u32 = 173;
    pub const RT_SIGACTION: u32 = 174;
    pub const RT_SIGPROCMASK: u32 = 175;
    pub const RT_SIGPENDING: u32 = 176;
    pub const RT_SIGTIMEDWAIT: u32 = 177;
    pub const RT_SIGQUEUEINFO: u32 = 178;
    pub const RT_SIGSUSPEND: u32 = 179;
    pub const PREAD64: u32 = 180;
    pub const PWRITE64: u32 = 181;
    pub const CHOWN: u32 = 182;
    pub const GETCWD: u32 = 183;
    pub const CAPGET: u32 = 184;
    pub const CAPSET: u32 = 185;
    pub const SIGALTSTACK: u32 = 186;
    pub const SENDFILE: u32 = 187;
    pub const VFORK: u32 = 190;
    pub const UGETRLIMIT: u32 = 191;
    pub const MMAP2: u32 = 192;
    pub const TRUNCATE64: u32 = 193;
    pub const FTRUNCATE64: u32 = 194;
    pub const STAT64: u32 = 195;
    pub const LSTAT64: u32 = 196;
    pub const FSTAT64: u32 = 197;
    pub const LCHOWN32: u32 = 198;
    pub const GETUID32: u32 = 199;
    pub const GETGID32: u32 = 200;
    pub const GETEUID32: u32 = 201;
    pub const GETEGID32: u32 = 202;
    pub const SETREUID32: u32 = 203;
    pub const SETREGID32: u32 = 204;
    pub const GETGROUPS32: u32 = 205;
    pub const SETGROUPS32: u32 = 206;
    pub const FCHOWN32: u32 = 207;
    pub const SETRESUID32: u32 = 208;
    pub const GETRESUID32: u32 = 209;
    pub const SETRESGID32: u32 = 210;
    pub const GETRESGID32: u32 = 211;
    pub const CHOWN32: u32 = 212;
    pub const SETUID32: u32 = 213;
    pub const SETGID32: u32 = 214;
    pub const SETFSUID32: u32 = 215;
    pub const SETFSGID32: u32 = 216;
    pub const GETDENTS64: u32 = 217;
    pub const PIVOT_ROOT: u32 = 218;
    pub const MINCORE: u32 = 219;
    pub const MADVISE: u32 = 220;
    pub const FCNTL64: u32 = 221;
    pub const GETTID: u32 = 224;
    pub const READAHEAD: u32 = 225;
    pub const SETXATTR: u32 = 226;
    pub const LSETXATTR: u32 = 227;
    pub const FSETXATTR: u32 = 228;
    pub const GETXATTR: u32 = 229;
    pub const LGETXATTR: u32 = 230;
    pub const FGETXATTR: u32 = 231;
    pub const LISTXATTR: u32 = 232;
    pub const LLISTXATTR: u32 = 233;
    pub const FLISTXATTR: u32 = 234;
    pub const REMOVEXATTR: u32 = 235;
    pub const LREMOVEXATTR: u32 = 236;
    pub const FREMOVEXATTR: u32 = 237;
    pub const TKILL: u32 = 238;
    pub const SENDFILE64: u32 = 239;
    pub const FUTEX: u32 = 240;
    pub const SCHED_SETAFFINITY: u32 = 241;
    pub const SCHED_GETAFFINITY: u32 = 242;
    pub const IO_SETUP: u32 = 243;
    pub const IO_DESTROY: u32 = 244;
    pub const IO_GETEVENTS: u32 = 245;
    pub const IO_SUBMIT: u32 = 246;
    pub const IO_CANCEL: u32 = 247;
    pub const EXIT_GROUP: u32 = 248;
    pub const LOOKUP_DCOOKIE: u32 = 249;
    pub const EPOLL_CREATE: u32 = 250;
    pub const EPOLL_CTL: u32 = 251;
    pub const EPOLL_WAIT: u32 = 252;
    pub const REMAP_FILE_PAGES: u32 = 253;
    pub const SET_TID_ADDRESS: u32 = 256;
    pub const TIMER_CREATE: u32 = 257;
    pub const TIMER_SETTIME: u32 = 258;
    pub const TIMER_GETTIME: u32 = 259;
    pub const TIMER_GETOVERRUN: u32 = 260;
    pub const TIMER_DELETE: u32 = 261;
    pub const CLOCK_SETTIME: u32 = 262;
    pub const CLOCK_GETTIME: u32 = 263;
    pub const CLOCK_GETRES: u32 = 264;
    pub const CLOCK_NANOSLEEP: u32 = 265;
    pub const TGKILL: u32 = 268;
    pub const UTIMES: u32 = 269;
    pub const OPENAT: u32 = 322;
    pub const MKDIRAT: u32 = 323;
    pub const MKNODAT: u32 = 324;
    pub const FCHOWNAT: u32 = 325;
    pub const FUTIMESAT: u32 = 326;
    pub const FSTATAT64: u32 = 327;
    pub const UNLINKAT: u32 = 328;
    pub const RENAMEAT: u32 = 329;
    pub const LINKAT: u32 = 330;
    pub const SYMLINKAT: u32 = 331;
    pub const READLINKAT: u32 = 332;
    pub const FCHMODAT: u32 = 333;
    pub const FACCESSAT: u32 = 334;
    
    // ARM-specific syscalls
    pub const ARM_BREAKPOINT: u32 = 0x0F0001;
    pub const ARM_CACHEFLUSH: u32 = 0x0F0002;
    pub const ARM_SET_TLS: u32 = 0x0F0005;
}

// Linux error codes (negated for return values)
pub mod errno {
    pub const EPERM: i32 = 1;
    pub const ENOENT: i32 = 2;
    pub const ESRCH: i32 = 3;
    pub const EINTR: i32 = 4;
    pub const EIO: i32 = 5;
    pub const ENXIO: i32 = 6;
    pub const E2BIG: i32 = 7;
    pub const ENOEXEC: i32 = 8;
    pub const EBADF: i32 = 9;
    pub const ECHILD: i32 = 10;
    pub const EAGAIN: i32 = 11;
    pub const ENOMEM: i32 = 12;
    pub const EACCES: i32 = 13;
    pub const EFAULT: i32 = 14;
    pub const ENOTBLK: i32 = 15;
    pub const EBUSY: i32 = 16;
    pub const EEXIST: i32 = 17;
    pub const EXDEV: i32 = 18;
    pub const ENODEV: i32 = 19;
    pub const ENOTDIR: i32 = 20;
    pub const EISDIR: i32 = 21;
    pub const EINVAL: i32 = 22;
    pub const ENFILE: i32 = 23;
    pub const EMFILE: i32 = 24;
    pub const ENOTTY: i32 = 25;
    pub const ETXTBSY: i32 = 26;
    pub const EFBIG: i32 = 27;
    pub const ENOSPC: i32 = 28;
    pub const ESPIPE: i32 = 29;
    pub const EROFS: i32 = 30;
    pub const EMLINK: i32 = 31;
    pub const EPIPE: i32 = 32;
    pub const EDOM: i32 = 33;
    pub const ERANGE: i32 = 34;
    pub const ENAMETOOLONG: i32 = 36;
    pub const ENOSYS: i32 = 38;
}

/// Result of syscall handling.
#[derive(Clone, Debug)]
pub enum SyscallResult {
    /// Continue execution after syscall.
    Continue,
    /// Program exited with given exit code.
    Exit(i32),
    /// Syscall not implemented.
    NotImplemented(u32),
}

/// File descriptor entry for emulated I/O.
#[derive(Clone, Debug)]
pub enum FileDescriptor {
    /// Standard input (read from host stdin).
    Stdin,
    /// Standard output (write to host stdout).
    Stdout,
    /// Standard error (write to host stderr).
    Stderr,
    /// Null device (discard output, return 0 on read).
    Null,
    /// Memory buffer (for testing).
    Buffer(Vec<u8>, usize), // data, position
}

/// Syscall handler for Linux ARM emulation.
pub struct SyscallHandler {
    /// File descriptor table.
    file_descriptors: HashMap<u32, FileDescriptor>,
    /// Next available file descriptor.
    next_fd: u32,
    /// Current program break (for brk).
    brk: u32,
    /// Initial break value.
    initial_brk: u32,
    /// Thread-local storage pointer.
    tls: u32,
    /// Process ID.
    pid: u32,
    /// Thread ID.
    tid: u32,
    /// Parent process ID.
    ppid: u32,
    /// User ID.
    uid: u32,
    /// Group ID.
    gid: u32,
    /// Current working directory.
    cwd: String,
}

impl Default for SyscallHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl SyscallHandler {
    /// Create a new syscall handler with default configuration.
    pub fn new() -> Self {
        let mut handler = SyscallHandler {
            file_descriptors: HashMap::new(),
            next_fd: 3,
            brk: 0,
            initial_brk: 0,
            tls: 0,
            pid: 1000,
            tid: 1000,
            ppid: 1,
            uid: 1000,
            gid: 1000,
            cwd: "/".to_string(),
        };
        
        // Set up standard file descriptors.
        handler.file_descriptors.insert(0, FileDescriptor::Stdin);
        handler.file_descriptors.insert(1, FileDescriptor::Stdout);
        handler.file_descriptors.insert(2, FileDescriptor::Stderr);
        
        handler
    }
    
    /// Set the initial program break.
    pub fn set_brk(&mut self, brk: u32) {
        self.brk = brk;
        self.initial_brk = brk;
    }
    
    /// Handle a syscall.
    /// 
    /// Reads syscall number from R7 and arguments from R0-R6.
    /// Writes result to R0.
    pub fn handle<M: ArmMemory>(&mut self, cpu: &mut Armv7Cpu, mem: &mut M) -> SyscallResult {
        let syscall_nr = cpu.regs[7];
        let arg0 = cpu.regs[0];
        let arg1 = cpu.regs[1];
        let arg2 = cpu.regs[2];
        let arg3 = cpu.regs[3];
        let arg4 = cpu.regs[4];
        let arg5 = cpu.regs[5];
        
        let result = match syscall_nr {
            nr::EXIT => {
                return SyscallResult::Exit(arg0 as i32);
            }
            nr::EXIT_GROUP => {
                return SyscallResult::Exit(arg0 as i32);
            }
            nr::READ => self.sys_read(mem, arg0, arg1, arg2),
            nr::WRITE => self.sys_write(mem, arg0, arg1, arg2),
            nr::OPEN => self.sys_open(mem, arg0, arg1, arg2),
            nr::CLOSE => self.sys_close(arg0),
            nr::BRK => self.sys_brk(arg0),
            nr::MMAP2 => self.sys_mmap2(mem, arg0, arg1, arg2, arg3, arg4, arg5),
            nr::MUNMAP => self.sys_munmap(arg0, arg1),
            nr::GETPID => Ok(self.pid as i32),
            nr::GETTID => Ok(self.tid as i32),
            nr::GETPPID => Ok(self.ppid as i32),
            nr::GETUID | nr::GETUID32 => Ok(self.uid as i32),
            nr::GETGID | nr::GETGID32 => Ok(self.gid as i32),
            nr::GETEUID | nr::GETEUID32 => Ok(self.uid as i32),
            nr::GETEGID | nr::GETEGID32 => Ok(self.gid as i32),
            nr::GETTIMEOFDAY => self.sys_gettimeofday(mem, arg0, arg1),
            nr::CLOCK_GETTIME => self.sys_clock_gettime(mem, arg0, arg1),
            nr::UNAME => self.sys_uname(mem, arg0),
            nr::STAT64 | nr::LSTAT64 => self.sys_stat64(mem, arg0, arg1),
            nr::FSTAT64 => self.sys_fstat64(mem, arg0, arg1),
            nr::SET_TID_ADDRESS => Ok(self.tid as i32),
            nr::FUTEX => Ok(0), // Stub: always succeed
            nr::RT_SIGACTION => Ok(0), // Stub: ignore
            nr::RT_SIGPROCMASK => Ok(0), // Stub: ignore
            nr::NANOSLEEP => Ok(0), // Stub: instant wake
            nr::GETCWD => self.sys_getcwd(mem, arg0, arg1),
            nr::ACCESS | nr::FACCESSAT => Ok(0), // Stub: always accessible
            nr::IOCTL => self.sys_ioctl(arg0, arg1, arg2),
            nr::FCNTL | nr::FCNTL64 => Ok(0), // Stub
            nr::MPROTECT => Ok(0), // Stub: always succeed
            nr::ARM_CACHEFLUSH => Ok(0), // No-op in emulation
            nr::ARM_SET_TLS => {
                self.tls = arg0;
                Ok(0)
            }
            _ => {
                return SyscallResult::NotImplemented(syscall_nr);
            }
        };
        
        cpu.regs[0] = match result {
            Ok(v) => v as u32,
            Err(e) => (-e) as u32, // Return negated errno
        };
        
        SyscallResult::Continue
    }
    
    // =========================================================================
    // File I/O Syscalls
    // =========================================================================
    
    fn sys_read<M: ArmMemory>(&mut self, mem: &mut M, fd: u32, buf: u32, count: u32) -> Result<i32, i32> {
        let fd_entry = self.file_descriptors.get_mut(&fd).ok_or(errno::EBADF)?;
        
        let count = count.min(0x10000) as usize; // Limit read size
        let mut data = vec![0u8; count];
        
        let bytes_read = match fd_entry {
            FileDescriptor::Stdin => {
                std::io::stdin().read(&mut data).unwrap_or(0)
            }
            FileDescriptor::Null => 0,
            FileDescriptor::Buffer(buf_data, pos) => {
                let available = buf_data.len().saturating_sub(*pos);
                let to_read = count.min(available);
                data[..to_read].copy_from_slice(&buf_data[*pos..*pos + to_read]);
                *pos += to_read;
                to_read
            }
            _ => return Err(errno::EINVAL),
        };
        
        for (i, &byte) in data[..bytes_read].iter().enumerate() {
            mem.write_byte(buf + i as u32, byte).map_err(|_| errno::EFAULT)?;
        }
        
        Ok(bytes_read as i32)
    }
    
    fn sys_write<M: ArmMemory>(&mut self, mem: &M, fd: u32, buf: u32, count: u32) -> Result<i32, i32> {
        let fd_entry = self.file_descriptors.get_mut(&fd).ok_or(errno::EBADF)?;
        
        let count = count.min(0x10000) as usize;
        let mut data = vec![0u8; count];
        
        for i in 0..count {
            data[i] = mem.read_byte(buf + i as u32).map_err(|_| errno::EFAULT)?;
        }
        
        let bytes_written = match fd_entry {
            FileDescriptor::Stdout => {
                std::io::stdout().write_all(&data).map_err(|_| errno::EIO)?;
                count
            }
            FileDescriptor::Stderr => {
                std::io::stderr().write_all(&data).map_err(|_| errno::EIO)?;
                count
            }
            FileDescriptor::Null => count,
            FileDescriptor::Buffer(buf_data, _) => {
                buf_data.extend_from_slice(&data);
                count
            }
            _ => return Err(errno::EINVAL),
        };
        
        Ok(bytes_written as i32)
    }
    
    fn sys_open<M: ArmMemory>(&mut self, mem: &M, path: u32, _flags: u32, _mode: u32) -> Result<i32, i32> {
        // Read path from memory
        let mut path_str = String::new();
        let mut addr = path;
        loop {
            let byte = mem.read_byte(addr).map_err(|_| errno::EFAULT)?;
            if byte == 0 {
                break;
            }
            path_str.push(byte as char);
            addr += 1;
            if path_str.len() > 4096 {
                return Err(errno::ENAMETOOLONG);
            }
        }
        
        // Handle special paths
        if path_str == "/dev/null" {
            let fd = self.next_fd;
            self.file_descriptors.insert(fd, FileDescriptor::Null);
            self.next_fd += 1;
            return Ok(fd as i32);
        }
        
        // For now, just fail on other files
        Err(errno::ENOENT)
    }
    
    fn sys_close(&mut self, fd: u32) -> Result<i32, i32> {
        if fd < 3 {
            // Don't close standard streams
            return Ok(0);
        }
        
        if self.file_descriptors.remove(&fd).is_some() {
            Ok(0)
        } else {
            Err(errno::EBADF)
        }
    }
    
    fn sys_ioctl(&mut self, fd: u32, cmd: u32, _arg: u32) -> Result<i32, i32> {
        if !self.file_descriptors.contains_key(&fd) {
            return Err(errno::EBADF);
        }
        
        // Handle common terminal ioctls
        const TCGETS: u32 = 0x5401;
        const TIOCGWINSZ: u32 = 0x5413;
        
        match cmd {
            TCGETS | TIOCGWINSZ => {
                // Not a terminal
                Err(errno::ENOTTY)
            }
            _ => Ok(0)
        }
    }
    
    // =========================================================================
    // Memory Management Syscalls
    // =========================================================================
    
    fn sys_brk(&mut self, addr: u32) -> Result<i32, i32> {
        if addr == 0 {
            // Query current break
            return Ok(self.brk as i32);
        }
        
        if addr >= self.initial_brk && addr < 0xC000_0000 {
            self.brk = addr;
            Ok(addr as i32)
        } else {
            Ok(self.brk as i32)
        }
    }
    
    fn sys_mmap2<M: ArmMemory>(
        &mut self,
        _mem: &mut M,
        addr: u32,
        length: u32,
        _prot: u32,
        flags: u32,
        fd: u32,
        _offset: u32,
    ) -> Result<i32, i32> {
        const MAP_ANONYMOUS: u32 = 0x20;
        const MAP_FIXED: u32 = 0x10;
        
        if (flags & MAP_ANONYMOUS) != 0 {
            // Anonymous mapping - just allocate from brk
            let alloc_addr = if (flags & MAP_FIXED) != 0 && addr != 0 {
                addr
            } else {
                let result = self.brk;
                self.brk = self.brk.wrapping_add((length + 0xFFF) & !0xFFF);
                result
            };
            
            return Ok(alloc_addr as i32);
        }
        
        // File-backed mapping
        if !self.file_descriptors.contains_key(&fd) {
            return Err(errno::EBADF);
        }
        
        // For now, treat as anonymous
        let result = self.brk;
        self.brk = self.brk.wrapping_add((length + 0xFFF) & !0xFFF);
        Ok(result as i32)
    }
    
    fn sys_munmap(&mut self, _addr: u32, _length: u32) -> Result<i32, i32> {
        // Stub: always succeed (we don't track mappings precisely)
        Ok(0)
    }
    
    // =========================================================================
    // Time Syscalls
    // =========================================================================
    
    fn sys_gettimeofday<M: ArmMemory>(&self, mem: &mut M, tv: u32, _tz: u32) -> Result<i32, i32> {
        if tv == 0 {
            return Ok(0);
        }
        
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        
        let tv_sec = now.as_secs() as u32;
        let tv_usec = now.subsec_micros();
        
        mem.write_word(tv, tv_sec).map_err(|_| errno::EFAULT)?;
        mem.write_word(tv + 4, tv_usec).map_err(|_| errno::EFAULT)?;
        
        Ok(0)
    }
    
    fn sys_clock_gettime<M: ArmMemory>(&self, mem: &mut M, clock_id: u32, tp: u32) -> Result<i32, i32> {
        if tp == 0 {
            return Ok(0);
        }
        
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        
        let tv_sec = now.as_secs() as u32;
        let tv_nsec = now.subsec_nanos();
        
        // Handle different clock types
        let _ = clock_id; // Accept any clock for simplicity
        
        mem.write_word(tp, tv_sec).map_err(|_| errno::EFAULT)?;
        mem.write_word(tp + 4, tv_nsec).map_err(|_| errno::EFAULT)?;
        
        Ok(0)
    }
    
    // =========================================================================
    // System Info Syscalls
    // =========================================================================
    
    fn sys_uname<M: ArmMemory>(&self, mem: &mut M, buf: u32) -> Result<i32, i32> {
        // struct utsname has 5 fields of 65 bytes each
        let sysname = b"Linux\0";
        let nodename = b"localhost\0";
        let release = b"5.10.0-arm\0";
        let version = b"#1 SMP ARM\0";
        let machine = b"armv7l\0";
        
        let mut write_field = |offset: u32, data: &[u8]| -> Result<(), i32> {
            for (i, &byte) in data.iter().enumerate() {
                mem.write_byte(buf + offset + i as u32, byte).map_err(|_| errno::EFAULT)?;
            }
            Ok(())
        };
        
        write_field(0, sysname)?;
        write_field(65, nodename)?;
        write_field(130, release)?;
        write_field(195, version)?;
        write_field(260, machine)?;
        
        Ok(0)
    }
    
    fn sys_stat64<M: ArmMemory>(&self, _mem: &mut M, _path: u32, _statbuf: u32) -> Result<i32, i32> {
        // Stub: file not found
        Err(errno::ENOENT)
    }
    
    fn sys_fstat64<M: ArmMemory>(&self, mem: &mut M, fd: u32, statbuf: u32) -> Result<i32, i32> {
        if !self.file_descriptors.contains_key(&fd) {
            return Err(errno::EBADF);
        }
        
        // Zero out the stat buffer (96 bytes for stat64)
        for i in 0..96 {
            mem.write_byte(statbuf + i, 0).map_err(|_| errno::EFAULT)?;
        }
        
        // Set st_mode for character device (for stdin/stdout/stderr)
        let st_mode = 0o20666u32; // S_IFCHR | 0666
        mem.write_word(statbuf + 16, st_mode).map_err(|_| errno::EFAULT)?;
        
        Ok(0)
    }
    
    fn sys_getcwd<M: ArmMemory>(&self, mem: &mut M, buf: u32, size: u32) -> Result<i32, i32> {
        let cwd_bytes = self.cwd.as_bytes();
        
        if size == 0 {
            return Err(errno::EINVAL);
        }
        
        if cwd_bytes.len() + 1 > size as usize {
            return Err(errno::ERANGE);
        }
        
        for (i, &byte) in cwd_bytes.iter().enumerate() {
            mem.write_byte(buf + i as u32, byte).map_err(|_| errno::EFAULT)?;
        }
        mem.write_byte(buf + cwd_bytes.len() as u32, 0).map_err(|_| errno::EFAULT)?;
        
        Ok(buf as i32)
    }
    
    /// Get the TLS pointer.
    pub fn get_tls(&self) -> u32 {
        self.tls
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arm::execution::FlatMemory;
    
    fn make_cpu_mem() -> (Armv7Cpu, FlatMemory) {
        (Armv7Cpu::new(), FlatMemory::new(0x10000, 0))
    }
    
    #[test]
    fn test_exit() {
        let (mut cpu, mut mem) = make_cpu_mem();
        let mut handler = SyscallHandler::new();
        
        cpu.regs[7] = nr::EXIT;
        cpu.regs[0] = 42;
        
        let result = handler.handle(&mut cpu, &mut mem);
        assert!(matches!(result, SyscallResult::Exit(42)));
    }
    
    #[test]
    fn test_getpid() {
        let (mut cpu, mut mem) = make_cpu_mem();
        let mut handler = SyscallHandler::new();
        
        cpu.regs[7] = nr::GETPID;
        
        let result = handler.handle(&mut cpu, &mut mem);
        assert!(matches!(result, SyscallResult::Continue));
        assert_eq!(cpu.regs[0], 1000); // Default PID
    }
    
    #[test]
    fn test_brk() {
        let (mut cpu, mut mem) = make_cpu_mem();
        let mut handler = SyscallHandler::new();
        handler.set_brk(0x1000);
        
        // Query current brk
        cpu.regs[7] = nr::BRK;
        cpu.regs[0] = 0;
        handler.handle(&mut cpu, &mut mem);
        assert_eq!(cpu.regs[0], 0x1000);
        
        // Set new brk
        cpu.regs[0] = 0x2000;
        handler.handle(&mut cpu, &mut mem);
        assert_eq!(cpu.regs[0], 0x2000);
    }
    
    #[test]
    fn test_write() {
        let (mut cpu, mut mem) = make_cpu_mem();
        let mut handler = SyscallHandler::new();
        
        // Replace stdout with buffer
        handler.file_descriptors.insert(1, FileDescriptor::Buffer(Vec::new(), 0));
        
        // Write "Hello"
        let msg = b"Hello";
        for (i, &byte) in msg.iter().enumerate() {
            mem.write_byte(0x100 + i as u32, byte).unwrap();
        }
        
        cpu.regs[7] = nr::WRITE;
        cpu.regs[0] = 1; // stdout
        cpu.regs[1] = 0x100; // buffer
        cpu.regs[2] = 5; // count
        
        handler.handle(&mut cpu, &mut mem);
        assert_eq!(cpu.regs[0], 5);
        
        if let FileDescriptor::Buffer(ref data, _) = handler.file_descriptors[&1] {
            assert_eq!(data, msg);
        } else {
            panic!("Expected buffer");
        }
    }
    
    #[test]
    fn test_gettimeofday() {
        let (mut cpu, mut mem) = make_cpu_mem();
        let mut handler = SyscallHandler::new();
        
        cpu.regs[7] = nr::GETTIMEOFDAY;
        cpu.regs[0] = 0x100; // tv
        cpu.regs[1] = 0; // tz (NULL)
        
        handler.handle(&mut cpu, &mut mem);
        assert_eq!(cpu.regs[0], 0);
        
        // Check that tv_sec is reasonable (after year 2020)
        let tv_sec = mem.read_word(0x100).unwrap();
        assert!(tv_sec > 1577836800); // 2020-01-01
    }
    
    #[test]
    fn test_close_invalid_fd() {
        let (mut cpu, mut mem) = make_cpu_mem();
        let mut handler = SyscallHandler::new();
        
        cpu.regs[7] = nr::CLOSE;
        cpu.regs[0] = 999; // Invalid fd
        
        handler.handle(&mut cpu, &mut mem);
        assert_eq!(cpu.regs[0] as i32, -errno::EBADF);
    }
    
    #[test]
    fn test_uname() {
        let (mut cpu, mut mem) = make_cpu_mem();
        let mut handler = SyscallHandler::new();
        
        cpu.regs[7] = nr::UNAME;
        cpu.regs[0] = 0x100;
        
        handler.handle(&mut cpu, &mut mem);
        assert_eq!(cpu.regs[0], 0);
        
        // Check sysname
        assert_eq!(mem.read_byte(0x100).unwrap(), b'L');
        assert_eq!(mem.read_byte(0x101).unwrap(), b'i');
        assert_eq!(mem.read_byte(0x102).unwrap(), b'n');
        assert_eq!(mem.read_byte(0x103).unwrap(), b'u');
        assert_eq!(mem.read_byte(0x104).unwrap(), b'x');
    }
    
    #[test]
    fn test_set_tls() {
        let (mut cpu, mut mem) = make_cpu_mem();
        let mut handler = SyscallHandler::new();
        
        cpu.regs[7] = nr::ARM_SET_TLS;
        cpu.regs[0] = 0xDEADBEEF;
        
        handler.handle(&mut cpu, &mut mem);
        assert_eq!(cpu.regs[0], 0);
        assert_eq!(handler.get_tls(), 0xDEADBEEF);
    }
}
