#define _GNU_SOURCE
#include <errno.h>
#include <stdint.h>
#include <fcntl.h>
#include <string.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/prctl.h>
#include <sys/mman.h>
#include <sys/ptrace.h>
#include <sys/wait.h>
#include <unistd.h>

static volatile unsigned long magic = 0x8877665544332211UL;

static int read_all(int fd, void *buffer, size_t size) {
    char *at = buffer;
    while (size) {
        ssize_t n = read(fd, at, size);
        if (n < 0 && errno == EINTR) {
            continue;
        }
        if (n <= 0) {
            return -1;
        }
        at += n;
        size -= n;
    }
    return 0;
}

int main(void) {
    void *mapping = mmap((void *)0x1234500000UL, 4096, PROT_READ | PROT_WRITE,
                         MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED_NOREPLACE, -1, 0);
    if (mapping != (void *)0x1234500000UL) {
        return 7;
    }
    int ids[2], gate[2], done[2];
    if (pipe(ids) || pipe(gate) || pipe(done)) {
        return 1;
    }
    pid_t target = getpid();
    pid_t middle = fork();
    if (middle < 0) {
        return 2;
    }
    if (middle == 0) {
        pid_t tracer = fork();
        if (tracer < 0) {
            _exit(3);
        }
        if (tracer == 0) {
            close(gate[1]);
            char token;
            while (read(gate[0], &token, 1) < 0 && errno == EINTR) {}
            unsigned char result = 1;
            if (ptrace(PTRACE_ATTACH, target, 0, 0) == 0) {
                int status;
                if (waitpid(target, &status, __WALL) == target && WIFSTOPPED(status)) {
                    errno = 0;
                    long value = ptrace(PTRACE_PEEKDATA, target, &magic, 0);
                    if (errno == 0 && (unsigned long)value == magic) {
                        char path[80], executable[4096];
                        snprintf(path, sizeof(path), "/proc/%d/exe", target);
                        ssize_t n = readlink(path, executable, sizeof(executable));
                        if (n > 0 && access(path, R_OK | X_OK) == 0) {
                            snprintf(path, sizeof(path), "/proc/%d/maps", target);
                            int fd = open(path, O_RDONLY);
                            char mappings[16384];
                            ssize_t count = fd < 0 ? -1 : read(fd, mappings, sizeof(mappings) - 1);
                            if (count > 0) {
                                mappings[count] = 0;
                                if (strstr(mappings, "1234500000-1234501000 rw-p")) {
                                    result = 0;
                                }
                            }
                            if (fd >= 0) {
                                close(fd);
                            }
                        }
                    }
                    if (ptrace(PTRACE_DETACH, target, 0, 0) != 0) {
                        result = 1;
                    }
                }
            }
            write(done[1], &result, 1);
            _exit(result);
        }
        write(ids[1], &tracer, sizeof(tracer));
        _exit(0);
    }
    close(gate[0]);
    pid_t tracer;
    if (read_all(ids[0], &tracer, sizeof(tracer))) {
        return 4;
    }
    int status;
    waitpid(middle, &status, 0);
    if (prctl(PR_SET_PTRACER, tracer, 0, 0, 0)) {
        return 4;
    }
    close(gate[1]);
    unsigned char result;
    if (read_all(done[0], &result, 1)) {
        return 5;
    }
    if (result) {
        return 6;
    }
    puts("grandchild attach, stopped memory, proc exe/maps, detach: ok");
    return 0;
}
