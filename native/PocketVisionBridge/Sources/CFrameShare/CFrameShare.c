#include "CFrameShare.h"

#include <fcntl.h>
#include <stdatomic.h>
#include <stdint.h>
#include <sys/mman.h>

#define POCKET_FRAME_SEQUENCE_OFFSET 16

int pocket_shm_open_readwrite(const char *name) {
    return shm_open(name, O_RDWR, 0);
}

static _Atomic(uint64_t) *sequence_at(void *mapping) {
    return (_Atomic(uint64_t) *)((unsigned char *)mapping + POCKET_FRAME_SEQUENCE_OFFSET);
}

void pocket_frame_publish_begin(void *mapping) {
    atomic_store_explicit(sequence_at(mapping), 0, memory_order_release);
}

void pocket_frame_publish_end(void *mapping, uint64_t sequence) {
    atomic_store_explicit(sequence_at(mapping), sequence, memory_order_release);
}
