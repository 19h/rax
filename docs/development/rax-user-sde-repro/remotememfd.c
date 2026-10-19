#define _GNU_SOURCE
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>

int main(void) {
    int ids[2], gate[2], done[2];
    if (pipe(ids) || pipe(gate) || pipe(done)) return 1;
    pid_t owner = fork();
    if (owner < 0) return 2;
    if (!owner) {
        close(gate[1]);
        int fd = memfd_create("foreign", MFD_CLOEXEC);
        if (fd < 0 || write(fd, "abcdef", 6) != 6 || ftruncate(fd, 4096)) _exit(3);
        write(ids[1], &fd, sizeof(fd));
        char token, first;
        read(gate[0], &token, 1);
        unsigned char result = pread(fd, &first, 1, 0) == 1 && first == 'X' && lseek(fd, 0, SEEK_CUR) == 6 ? 0 : 1;
        write(done[1], &result, 1);
        _exit(result);
    }
    close(gate[0]);
    int remote;
    if (read(ids[0], &remote, sizeof(remote)) != sizeof(remote)) return 3;
    char path[80];
    snprintf(path, sizeof(path), "/proc/%d/fd/%d", owner, remote);
    int fd = open(path, O_RDWR | O_CLOEXEC);
    if (fd < 0) { close(gate[1]); return 4; }
    char bytes[3];
    if (read(fd, bytes, 3) != 3 || memcmp(bytes, "abc", 3)) return 5;
    pid_t reader = fork();
    if (reader < 0) return 6;
    if (!reader) {
        char byte;
        _exit(read(fd, &byte, 1) == 1 && byte == 'd' ? 0 : 7);
    }
    int status;
    if (waitpid(reader, &status, 0) != reader || status) return 8;
    char byte;
    if (read(fd, &byte, 1) != 1 || byte != 'e') return 9;
    char *shared = mmap(NULL, 4096, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    if (shared == MAP_FAILED) return 10;
    shared[0] = 'X';
    write(gate[1], "1", 1);
    unsigned char result;
    if (read(done[0], &result, 1) != 1 || result) return 11;
    if (waitpid(owner, &status, 0) != owner || status) return 12;
    if (pread(fd, &byte, 1, 0) != 1 || byte != 'X') return 13;
    puts("remote memfd: independent reopen, shared fork offset, shared mapping, lifetime: ok");
    return 0;
}
