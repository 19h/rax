// Freestanding Linux BPF execution harness for the RAX kernel differential
// campaign. It has no libc dependency, so one source builds for the
// little-endian aarch64 container host and for a big-endian s390x guest,
// where it may run as PID 1 from an initramfs.
//
// Input (all header integers little-endian):
//   "RAXBPFIN" u32 program_count
//   per program: u32 insn_count, u32 ctx_size, insns[insn_count * 8],
//                ctx[ctx_size]
// Instructions and ctx bytes are already in the kernel's byte order.
//
// Output:
//   "RAXBPFOU" u32 program_count
//   per program: i32 load_status, i32 run_status, u32 retval,
//                u32 log_len, log[log_len], u32 ctx_size, ctx[ctx_size]
// A program that fails to load reports -errno and the verifier log tail.
// Input magic "RAXBPFLD" selects admission mode: programs are only loaded
// (not run), the output carries a 256-byte log tail, and ctx is unused.
//
// Each program is a BPF_PROG_TYPE_SYSCALL program run once through
// BPF_PROG_TEST_RUN. The kernel copies the (possibly modified) ctx buffer
// back to the caller, which carries the final architectural state.
//
// Container mode reads /work/input.bin and writes /work/output.bin. Without
// that file (initramfs PID 1 mode) it reads /input.bin, writes the raw output
// to the first virtio-serial port (/dev/vportNp1) or, when none exists, to the
// console as base-16 lines framed by RAXBPF-BEGIN/RAXBPF-END, and powers off.

typedef unsigned char u8;
typedef unsigned int u32;
typedef int s32;
typedef unsigned long u64;
typedef long s64;

#define BPF_PROG_LOAD 5
#define BPF_PROG_TEST_RUN 10
#define BPF_PROG_TYPE_SYSCALL 31
#define BPF_F_SLEEPABLE (1U << 4)

#define AT_FDCWD (-100)
#define O_RDONLY 0
#define O_WRONLY 1
#define O_CREAT 0100
#define O_TRUNC 01000

#if defined(__aarch64__)
#define NR_openat 56
#define NR_close 57
#define NR_read 63
#define NR_write 64
#define NR_exit_group 94
#define NR_bpf 280
#define NR_mount 40
#define NR_reboot 142
#define NR_sync 81

static s64 syscall6(s64 n, s64 a, s64 b, s64 c, s64 d, s64 e, s64 f)
{
    register s64 x8 __asm__("x8") = n;
    register s64 x0 __asm__("x0") = a;
    register s64 x1 __asm__("x1") = b;
    register s64 x2 __asm__("x2") = c;
    register s64 x3 __asm__("x3") = d;
    register s64 x4 __asm__("x4") = e;
    register s64 x5 __asm__("x5") = f;
    __asm__ volatile("svc #0"
                     : "+r"(x0)
                     : "r"(x8), "r"(x1), "r"(x2), "r"(x3), "r"(x4), "r"(x5)
                     : "memory", "cc");
    return x0;
}
#elif defined(__s390x__)
#define NR_openat 288
#define NR_close 6
#define NR_read 3
#define NR_write 4
#define NR_exit_group 248
#define NR_bpf 351
#define NR_mount 21
#define NR_reboot 88
#define NR_sync 36

static s64 syscall6(s64 n, s64 a, s64 b, s64 c, s64 d, s64 e, s64 f)
{
    register s64 r1 __asm__("r1") = n;
    register s64 r2 __asm__("r2") = a;
    register s64 r3 __asm__("r3") = b;
    register s64 r4 __asm__("r4") = c;
    register s64 r5 __asm__("r5") = d;
    register s64 r6 __asm__("r6") = e;
    register s64 r7 __asm__("r7") = f;
    __asm__ volatile("svc 0"
                     : "+r"(r2)
                     : "r"(r1), "r"(r3), "r"(r4), "r"(r5), "r"(r6), "r"(r7)
                     : "memory", "cc");
    return r2;
}
#elif defined(__x86_64__)
#define NR_openat 257
#define NR_close 3
#define NR_read 0
#define NR_write 1
#define NR_exit_group 231
#define NR_bpf 321
#define NR_mount 165
#define NR_reboot 169
#define NR_sync 162

static s64 syscall6(s64 n, s64 a, s64 b, s64 c, s64 d, s64 e, s64 f)
{
    register s64 r10 __asm__("r10") = d;
    register s64 r8 __asm__("r8") = e;
    register s64 r9 __asm__("r9") = f;
    __asm__ volatile("syscall"
                     : "+a"(n)
                     : "D"(a), "S"(b), "d"(c), "r"(r10), "r"(r8), "r"(r9)
                     : "rcx", "r11", "memory", "cc");
    return n;
}
#else
#error "unsupported harness architecture"
#endif

#define INPUT_CAPACITY (384UL << 20)
#define OUTPUT_CAPACITY (384UL << 20)
#define LOG_CAPACITY (1UL << 20)
#define LOG_TAIL 768

static u8 input[INPUT_CAPACITY];
static u8 output[OUTPUT_CAPACITY];
static char verifier_log[LOG_CAPACITY];
static u8 ctx[1UL << 16];
static u64 output_len;

void *memset(void *dst, int value, u64 len)
{
    u8 *p = dst;
    while (len--) {
        *p++ = (u8)value;
    }
    return dst;
}

void *memcpy(void *dst, const void *src, u64 len)
{
    u8 *d = dst;
    const u8 *s = src;
    while (len--) {
        *d++ = *s++;
    }
    return dst;
}

static u64 cstrlen(const char *s)
{
    u64 n = 0;
    while (s[n]) {
        n++;
    }
    return n;
}

static void write_all(s64 fd, const void *data, u64 len)
{
    const u8 *p = data;
    while (len) {
        s64 n = syscall6(NR_write, fd, (s64)p, (s64)len, 0, 0, 0);
        if (n <= 0) {
            return;
        }
        p += n;
        len -= (u64)n;
    }
}

static void say(const char *text)
{
    write_all(2, text, cstrlen(text));
}

__attribute__((noreturn)) static void finish(int status, int as_init)
{
    if (as_init) {
        syscall6(NR_sync, 0, 0, 0, 0, 0, 0);
        // LINUX_REBOOT_MAGIC1, MAGIC2, LINUX_REBOOT_CMD_POWER_OFF
        syscall6(NR_reboot, 0xfee1dead, 672274793, 0x4321fedc, 0, 0, 0);
    }
    for (;;) {
        syscall6(NR_exit_group, status, 0, 0, 0, 0, 0);
    }
}

static int memcmp_magic(const u8 *data, const char *magic)
{
    for (u64 i = 0; magic[i]; i++) {
        if (data[i] != (u8)magic[i]) {
            return 1;
        }
    }
    return 0;
}

static u32 get_le32(const u8 *p)
{
    return (u32)p[0] | ((u32)p[1] << 8) | ((u32)p[2] << 16) | ((u32)p[3] << 24);
}

static void put_le32(u32 value)
{
    if (output_len + 4 > OUTPUT_CAPACITY) {
        return;
    }
    output[output_len++] = (u8)value;
    output[output_len++] = (u8)(value >> 8);
    output[output_len++] = (u8)(value >> 16);
    output[output_len++] = (u8)(value >> 24);
}

static void put_bytes(const void *data, u64 len)
{
    if (output_len + len > OUTPUT_CAPACITY) {
        return;
    }
    memcpy(output + output_len, data, len);
    output_len += len;
}

static s64 read_file(const char *path, u8 *buffer, u64 capacity)
{
    s64 fd = syscall6(NR_openat, AT_FDCWD, (s64)path, O_RDONLY, 0, 0, 0);
    if (fd < 0) {
        return fd;
    }
    u64 len = 0;
    for (;;) {
        s64 n = syscall6(NR_read, fd, (s64)(buffer + len), (s64)(capacity - len), 0, 0, 0);
        if (n < 0) {
            return n;
        }
        if (n == 0) {
            break;
        }
        len += (u64)n;
        if (len == capacity) {
            return -27; // EFBIG
        }
    }
    syscall6(NR_close, fd, 0, 0, 0, 0, 0);
    return (s64)len;
}

// struct bpf_attr prefix for BPF_PROG_LOAD (include/uapi/linux/bpf.h).
struct prog_load_attr {
    u32 prog_type;
    u32 insn_cnt;
    u64 insns;
    u64 license;
    u32 log_level;
    u32 log_size;
    u64 log_buf;
    u32 kern_version;
    u32 prog_flags;
    char prog_name[16];
    u32 prog_ifindex;
    u32 expected_attach_type;
    u8 reserved[64];
};

// struct bpf_attr prefix for BPF_PROG_TEST_RUN.
struct test_run_attr {
    u32 prog_fd;
    u32 retval;
    u32 data_size_in;
    u32 data_size_out;
    u64 data_in;
    u64 data_out;
    u32 repeat;
    u32 duration;
    u32 ctx_size_in;
    u32 ctx_size_out;
    u64 ctx_in;
    u64 ctx_out;
    u32 flags;
    u32 cpu;
    u32 batch_size;
    u32 pad;
};

static s64 load_program(const u8 *insns, u32 insn_count, u32 log_level)
{
    static const char license[] = "GPL";
    struct prog_load_attr attr;
    memset(&attr, 0, sizeof(attr));
    attr.prog_type = BPF_PROG_TYPE_SYSCALL;
    attr.insn_cnt = insn_count;
    attr.insns = (u64)insns;
    attr.license = (u64)license;
    attr.prog_flags = BPF_F_SLEEPABLE;
    if (log_level) {
        verifier_log[0] = 0;
        attr.log_level = log_level;
        attr.log_size = LOG_CAPACITY;
        attr.log_buf = (u64)verifier_log;
    }
    return syscall6(NR_bpf, BPF_PROG_LOAD, (s64)&attr, sizeof(attr), 0, 0, 0);
}

static int run_campaign(const char *input_path)
{
    s64 input_len = read_file(input_path, input, INPUT_CAPACITY);
    if (input_len < 12) {
        say("harness: cannot read input\n");
        return 2;
    }
    int load_only = !memcmp_magic(input, "RAXBPFLD");
    if (memcmp_magic(input, "RAXBPFIN") && !load_only) {
        say("harness: bad input magic\n");
        return 2;
    }
    u64 log_tail = load_only ? 256 : LOG_TAIL;
    u32 count = get_le32(input + 8);
    u64 cursor = 12;
    put_bytes("RAXBPFOU", 8);
    put_le32(count);
    for (u32 index = 0; index < count; index++) {
        if (cursor + 8 > (u64)input_len) {
            say("harness: truncated program header\n");
            return 2;
        }
        u32 insn_count = get_le32(input + cursor);
        u32 ctx_size = get_le32(input + cursor + 4);
        cursor += 8;
        u64 insn_bytes = (u64)insn_count * 8;
        if (ctx_size > sizeof(ctx) || cursor + insn_bytes + ctx_size > (u64)input_len) {
            say("harness: truncated program body\n");
            return 2;
        }
        const u8 *insns = input + cursor;
        cursor += insn_bytes;
        memcpy(ctx, input + cursor, ctx_size);
        cursor += ctx_size;

        s32 load_status = 0;
        s32 run_status = 0;
        u32 retval = 0;
        u64 log_len = 0;
        const char *log_start = verifier_log;

        s64 fd = load_program(insns, insn_count, 0);
        if (fd < 0) {
            load_status = (s32)fd;
            load_program(insns, insn_count, 1);
            log_len = cstrlen(verifier_log);
            if (log_len > log_tail) {
                log_start = verifier_log + (log_len - log_tail);
                log_len = log_tail;
            }
        } else if (load_only) {
            syscall6(NR_close, fd, 0, 0, 0, 0, 0);
        } else {
            struct test_run_attr attr;
            memset(&attr, 0, sizeof(attr));
            attr.prog_fd = (u32)fd;
            attr.ctx_size_in = ctx_size;
            attr.ctx_in = (u64)ctx;
            s64 status = syscall6(NR_bpf, BPF_PROG_TEST_RUN, (s64)&attr, sizeof(attr), 0, 0, 0);
            run_status = (s32)status;
            retval = attr.retval;
            syscall6(NR_close, fd, 0, 0, 0, 0, 0);
        }

        put_le32((u32)load_status);
        put_le32((u32)run_status);
        put_le32(retval);
        put_le32((u32)log_len);
        put_bytes(log_start, log_len);
        put_le32(ctx_size);
        put_bytes(ctx, ctx_size);
    }
    if (output_len > OUTPUT_CAPACITY - 16) {
        say("harness: output capacity exceeded\n");
        return 2;
    }
    return 0;
}

static void write_file(const char *path)
{
    s64 out = syscall6(NR_openat, AT_FDCWD, (s64)path, O_WRONLY | O_CREAT | O_TRUNC, 0644, 0, 0);
    if (out < 0) {
        say("harness: cannot open output\n");
        return;
    }
    write_all(out, output, output_len);
    syscall6(NR_close, out, 0, 0, 0, 0, 0);
}

static void write_console_hex(s64 fd)
{
    static const char digits[] = "0123456789abcdef";
    static char line[2 * 64 + 1];
    static const char begin[] = "\nRAXBPF-BEGIN\n";
    static const char end[] = "RAXBPF-END\n";
    write_all(fd, begin, sizeof(begin) - 1);
    for (u64 offset = 0; offset < output_len; offset += 64) {
        u64 chunk = output_len - offset < 64 ? output_len - offset : 64;
        u64 n = 0;
        for (u64 i = 0; i < chunk; i++) {
            line[n++] = digits[output[offset + i] >> 4];
            line[n++] = digits[output[offset + i] & 15];
        }
        line[n++] = '\n';
        write_all(fd, line, n);
    }
    write_all(fd, end, sizeof(end) - 1);
}

__attribute__((noreturn, used)) void harness_main(void)
{
    s64 probe = syscall6(NR_openat, AT_FDCWD, (s64)"/work/input.bin", O_RDONLY, 0, 0, 0);
    if (probe >= 0) {
        syscall6(NR_close, probe, 0, 0, 0, 0, 0);
        int status = run_campaign("/work/input.bin");
        if (status == 0) {
            write_file("/work/output.bin");
        }
        finish(status, 0);
    }
    syscall6(NR_mount, (s64)"devtmpfs", (s64)"/dev", (s64)"devtmpfs", 0, 0, 0);
    s64 console = syscall6(NR_openat, AT_FDCWD, (s64)"/dev/console", O_WRONLY, 0, 0, 0);
    if (console < 0) {
        console = 1;
    }
    int status = run_campaign("/input.bin");
    static const char *const ports[] = {
        "/dev/vport0p1", "/dev/vport1p1", "/dev/vport2p1", "/dev/vport3p1",
    };
    s64 port = -1;
    for (u64 i = 0; i < sizeof(ports) / sizeof(ports[0]) && port < 0; i++) {
        port = syscall6(NR_openat, AT_FDCWD, (s64)ports[i], O_WRONLY, 0, 0, 0);
    }
    if (status == 0 && port >= 0) {
        write_all(port, output, output_len);
        syscall6(NR_close, port, 0, 0, 0, 0, 0);
        write_all(console, "RAXBPF-PORT-DONE\n", 17);
    } else if (status == 0) {
        write_console_hex(console);
    } else {
        write_all(console, "RAXBPF-FAILED\n", 14);
    }
    finish(status, 1);
}

// Process entry: establish an ABI-conforming stack and call harness_main.
#if defined(__aarch64__)
__asm__(".globl _start\n"
        "_start:\n"
        "  mov x29, #0\n"
        "  mov x30, #0\n"
        "  bl harness_main\n");
#elif defined(__s390x__)
__asm__(".globl _start\n"
        "_start:\n"
        "  lghi %r0, 0\n"
        "  aghi %r15, -160\n"
        "  stg %r0, 0(%r15)\n"
        "  brasl %r14, harness_main\n");
#elif defined(__x86_64__)
__asm__(".globl _start\n"
        "_start:\n"
        "  xor %ebp, %ebp\n"
        "  and $-16, %rsp\n"
        "  call harness_main\n");
#endif
